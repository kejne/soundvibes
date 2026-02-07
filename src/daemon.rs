use chrono::{Local, Utc};
use signal_hook::consts::signal::{SIGINT, SIGTERM};
use signal_hook::flag;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::Shutdown;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::audio;
use crate::error::AppError;
use crate::ipc;
use crate::model::{self, ModelLanguage, ModelSize, ModelSpec};
use crate::output;
use crate::types::{AudioHost, OutputFormat, OutputMode, VadMode};
use crate::whisper::WhisperContext;

#[derive(Debug, Clone)]
pub struct DaemonConfig {
    pub download_model: bool,
    pub language: String,
    pub model_pool_languages: Vec<String>,
    pub device: Option<String>,
    pub audio_host: AudioHost,
    pub sample_rate: u32,
    pub format: OutputFormat,
    pub mode: OutputMode,
    pub vad: VadMode,
    pub vad_silence_ms: u64,
    pub vad_threshold: f32,
    pub vad_chunk_ms: u64,
    pub debug_audio: bool,
    pub debug_vad: bool,
    pub dump_audio: bool,
}

pub trait DaemonOutput {
    fn stdout(&mut self, message: &str);
    fn stderr(&mut self, message: &str);
}

pub struct StdoutOutput;

impl DaemonOutput for StdoutOutput {
    fn stdout(&mut self, message: &str) {
        println!("{message}");
    }

    fn stderr(&mut self, message: &str) {
        eprintln!("{message}");
    }
}

pub trait CaptureSource {
    fn drain(&mut self, output: &mut Vec<f32>);
}

pub trait AudioBackend {
    fn list_input_devices(&self, host: &cpal::Host) -> Result<Vec<String>, audio::AudioError>;
    fn start_capture(
        &self,
        host: &cpal::Host,
        device_name: Option<&str>,
        sample_rate: u32,
    ) -> Result<Box<dyn CaptureSource>, audio::AudioError>;
}

pub trait Transcriber {
    fn transcribe(&self, samples: &[f32], language: Option<&str>) -> Result<String, AppError>;
}

pub trait TranscriberFactory {
    fn load(
        &self,
        spec: &ModelSpec,
        allow_download: bool,
    ) -> Result<Box<dyn Transcriber>, AppError>;
}

pub struct DaemonDeps {
    pub audio: Box<dyn AudioBackend>,
    pub transcriber_factory: Box<dyn TranscriberFactory>,
}

impl Default for DaemonDeps {
    fn default() -> Self {
        Self {
            audio: Box::new(CpalAudioBackend),
            transcriber_factory: Box::new(WhisperFactory),
        }
    }
}

pub fn select_audio_host(audio_host: AudioHost) -> Result<cpal::Host, AppError> {
    match audio_host {
        AudioHost::Default => Ok(cpal::default_host()),
        _ => {
            let host_id = match audio_host {
                AudioHost::Alsa => cpal::HostId::Alsa,
                AudioHost::Default => cpal::HostId::Alsa,
            };
            if !cpal::available_hosts().contains(&host_id) {
                let available = cpal::available_hosts()
                    .into_iter()
                    .map(|host| format!("{host:?}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(AppError::config(format!(
                    "audio host {audio_host:?} not available (available: {available})"
                )));
            }
            cpal::host_from_id(host_id)
                .map_err(|err| AppError::runtime(format!("failed to init audio host: {err}")))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlEvent {
    Toggle { language: Option<String> },
    Status,
    SetLanguage { language: String },
    Stop,
    Error(String),
}

pub struct ControlMessage {
    pub event: ControlEvent,
    pub response: Option<mpsc::Sender<ipc::ControlResponse>>,
}

struct ModelPoolEntry {
    transcriber: Box<dyn Transcriber>,
    model_size: String,
    model_language: String,
}

struct ModelPool {
    entries: HashMap<String, ModelPoolEntry>,
}

impl ModelPool {
    fn preload(config: &DaemonConfig, deps: &DaemonDeps) -> Result<Self, AppError> {
        let mut pool = Self {
            entries: HashMap::new(),
        };

        let mut preload_languages = config
            .model_pool_languages
            .iter()
            .map(|language| normalize_language(language))
            .collect::<Vec<_>>();
        let active_language = normalize_language(&config.language);
        if !preload_languages.contains(&active_language) {
            preload_languages.push(active_language);
        }

        for language in preload_languages {
            pool.load_language(&language, config, deps)?;
        }

        Ok(pool)
    }

    fn ensure_language(
        &mut self,
        language: &str,
        config: &DaemonConfig,
        deps: &DaemonDeps,
    ) -> Result<bool, AppError> {
        if self.entries.contains_key(language) {
            return Ok(false);
        }
        self.load_language(language, config, deps)?;
        Ok(true)
    }

    fn set_entry(
        &mut self,
        language: &str,
        transcriber: Box<dyn Transcriber>,
        model_size: String,
        model_language: String,
    ) {
        self.entries.insert(
            language.to_string(),
            ModelPoolEntry {
                transcriber,
                model_size,
                model_language,
            },
        );
    }

    fn transcriber_for(&self, language: &str) -> Option<&dyn Transcriber> {
        self.entries
            .get(language)
            .map(|entry| entry.transcriber.as_ref())
    }

    fn metadata_for(&self, language: &str) -> Option<(&str, &str)> {
        self.entries
            .get(language)
            .map(|entry| (entry.model_size.as_str(), entry.model_language.as_str()))
    }

    fn load_language(
        &mut self,
        language: &str,
        config: &DaemonConfig,
        deps: &DaemonDeps,
    ) -> Result<(), AppError> {
        let spec = ModelSpec::new(
            ModelSize::Small,
            model::model_language_for_transcription(language),
        );
        let transcriber = deps
            .transcriber_factory
            .load(&spec, config.download_model)?;
        self.set_entry(
            language,
            transcriber,
            model_size_token(spec.size).to_string(),
            model_language_token(spec.language).to_string(),
        );
        Ok(())
    }
}

pub fn run_daemon(
    config: &DaemonConfig,
    deps: &DaemonDeps,
    output: &mut dyn DaemonOutput,
) -> Result<(), AppError> {
    let socket_path = daemon_socket_path()?;
    let (_control_guard, control_events) = start_socket_listener(&socket_path)?;
    output.stdout(&format!("Daemon listening on {}", socket_path.display()));

    let events_socket_path = daemon_events_socket_path()?;
    let (_events_guard, event_sender) = start_events_socket_listener(&events_socket_path)?;
    output.stdout(&format!(
        "Daemon events on {}",
        events_socket_path.display()
    ));

    let shutdown = Arc::new(AtomicBool::new(false));
    for signal in [SIGINT, SIGTERM] {
        flag::register(signal, Arc::clone(&shutdown)).map_err(|err| {
            AppError::runtime(format!("failed to register signal handler: {err}"))
        })?;
    }

    run_daemon_loop(
        config,
        deps,
        output,
        control_events,
        &shutdown,
        Some(&event_sender),
    )
}

pub fn run_daemon_loop(
    config: &DaemonConfig,
    deps: &DaemonDeps,
    output: &mut dyn DaemonOutput,
    control_events: Receiver<ControlMessage>,
    shutdown: &AtomicBool,
    event_sender: Option<&mpsc::Sender<ipc::DaemonEvent>>,
) -> Result<(), AppError> {
    let host = select_audio_host(config.audio_host)?;
    audio::configure_alsa_logging(config.debug_audio);
    let devices = deps
        .audio
        .list_input_devices(&host)
        .map_err(|err| AppError::audio(err.message))?;
    output.stdout("Input devices:");
    for name in &devices {
        output.stdout(&format!("  - {name}"));
    }

    if let Some(device) = config.device.as_deref() {
        if !devices.iter().any(|name| name.eq_ignore_ascii_case(device)) {
            return Err(AppError::audio(format!("input device not found: {device}")));
        }
    }

    let mut model_pool = ModelPool::preload(config, deps)?;
    let vad = audio::VadConfig::new(
        config.vad == VadMode::On,
        config.vad_silence_ms,
        config.vad_threshold,
        config.vad_chunk_ms,
        config.debug_vad,
    );

    let mut recording = false;
    let mut active_language = normalize_language(&config.language);
    let mut buffer = Vec::new();
    let mut utterance_index = 0u64;
    let mut capture: Option<Box<dyn CaptureSource>> = None;

    emit_daemon_event(event_sender, ipc::DaemonEventType::DaemonReady);
    emit_model_loaded_event(event_sender, &model_pool, &active_language);

    loop {
        if shutdown.load(Ordering::Relaxed) {
            if recording {
                let active_transcriber = active_transcriber(&model_pool, active_language.as_str())?;
                stop_recording(
                    active_transcriber,
                    config,
                    active_language.as_str(),
                    &vad,
                    &mut capture,
                    &mut buffer,
                    &mut utterance_index,
                    output,
                    event_sender,
                )?;
                emit_daemon_event(
                    event_sender,
                    ipc::DaemonEventType::RecordingStopped {
                        language: active_language.clone(),
                    },
                );
            }
            output.stdout("Daemon shutting down.");
            break;
        }
        match control_events.recv_timeout(Duration::from_millis(20)) {
            Ok(message) => {
                let response = match message.event {
                    ControlEvent::Toggle { language } => {
                        if let Some(language) = language {
                            let normalized = normalize_language(&language);
                            model_pool.ensure_language(&normalized, config, deps)?;
                            active_language = normalized;
                            emit_model_loaded_event(event_sender, &model_pool, &active_language);
                        }

                        if recording {
                            recording = false;
                            let active_transcriber =
                                active_transcriber(&model_pool, active_language.as_str())?;
                            stop_recording(
                                active_transcriber,
                                config,
                                active_language.as_str(),
                                &vad,
                                &mut capture,
                                &mut buffer,
                                &mut utterance_index,
                                output,
                                event_sender,
                            )?;
                            emit_daemon_event(
                                event_sender,
                                ipc::DaemonEventType::RecordingStopped {
                                    language: active_language.clone(),
                                },
                            );
                            control_ok_response("idle", active_language.as_str())
                        } else {
                            let new_capture = deps
                                .audio
                                .start_capture(&host, config.device.as_deref(), config.sample_rate)
                                .map_err(|err| match err.kind {
                                    audio::AudioErrorKind::DeviceNotFound
                                        if config.device.is_some() =>
                                    {
                                        AppError::audio(err.message)
                                    }
                                    _ => AppError::audio(err.message),
                                })?;
                            recording = true;
                            buffer.clear();
                            capture = Some(new_capture);
                            output.stdout("Toggle on. Recording...");
                            emit_daemon_event(
                                event_sender,
                                ipc::DaemonEventType::RecordingStarted {
                                    language: active_language.clone(),
                                },
                            );
                            control_ok_response("recording", active_language.as_str())
                        }
                    }
                    ControlEvent::Status => {
                        let state = if recording { "recording" } else { "idle" };
                        control_ok_response(state, active_language.as_str())
                    }
                    ControlEvent::SetLanguage { language } => {
                        let normalized = normalize_language(&language);
                        model_pool.ensure_language(&normalized, config, deps)?;
                        active_language = normalized;
                        emit_model_loaded_event(event_sender, &model_pool, &active_language);
                        let state = if recording { "recording" } else { "idle" };
                        control_ok_response(state, active_language.as_str())
                    }
                    ControlEvent::Stop => {
                        shutdown.store(true, Ordering::Relaxed);
                        control_ok_response(
                            if recording { "recording" } else { "idle" },
                            active_language.as_str(),
                        )
                    }
                    ControlEvent::Error(error_message) => {
                        emit_daemon_event(
                            event_sender,
                            ipc::DaemonEventType::Error {
                                message: error_message.clone(),
                            },
                        );
                        if let Some(sender) = message.response {
                            let _ = sender.send(control_error_response(
                                "listener_error",
                                error_message.clone(),
                            ));
                        }
                        return Err(AppError::runtime(error_message));
                    }
                };

                if let Some(sender) = message.response {
                    let _ = sender.send(response);
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                return Err(AppError::runtime("socket listener disconnected"))
            }
        }

        if recording {
            if let Some(active) = capture.as_mut() {
                active.drain(&mut buffer);
            }
        }
    }
    Ok(())
}

fn stop_recording(
    transcriber: &dyn Transcriber,
    config: &DaemonConfig,
    language: &str,
    vad: &audio::VadConfig,
    capture: &mut Option<Box<dyn CaptureSource>>,
    buffer: &mut Vec<f32>,
    utterance_index: &mut u64,
    output: &mut dyn DaemonOutput,
    event_sender: Option<&mpsc::Sender<ipc::DaemonEvent>>,
) -> Result<(), AppError> {
    let mut active = capture
        .take()
        .ok_or_else(|| AppError::runtime("capture stream missing"))?;
    active.drain(buffer);
    finalize_recording(
        transcriber,
        config,
        language,
        vad,
        buffer,
        utterance_index,
        output,
        event_sender,
    )?;
    Ok(())
}

fn finalize_recording(
    transcriber: &dyn Transcriber,
    config: &DaemonConfig,
    language: &str,
    vad: &audio::VadConfig,
    buffer: &[f32],
    utterance_index: &mut u64,
    output: &mut dyn DaemonOutput,
    event_sender: Option<&mpsc::Sender<ipc::DaemonEvent>>,
) -> Result<(), AppError> {
    let trimmed = audio::trim_trailing_silence(buffer, config.sample_rate, vad);
    if trimmed.is_empty() {
        return Ok(());
    }
    *utterance_index += 1;
    let duration_ms = audio::samples_to_ms(trimmed.len(), config.sample_rate);
    if config.dump_audio {
        dump_audio_samples(&trimmed, config.sample_rate, output)?;
    }
    let transcript = transcriber
        .transcribe(&trimmed, Some(language))
        .map_err(|err| {
            emit_daemon_event(
                event_sender,
                ipc::DaemonEventType::Error {
                    message: err.to_string(),
                },
            );
            AppError::runtime(err.to_string())
        })?;
    emit_transcript(
        config,
        output,
        &transcript,
        audio::SegmentInfo {
            index: *utterance_index,
            duration_ms,
        },
    )
    .map_err(AppError::runtime)?;
    emit_daemon_event(
        event_sender,
        ipc::DaemonEventType::TranscriptFinal {
            language: language.to_string(),
            utterance: *utterance_index,
            duration_ms,
            text: transcript,
        },
    );
    output.stdout("Ready for next utterance.");
    Ok(())
}

fn emit_daemon_event(sender: Option<&mpsc::Sender<ipc::DaemonEvent>>, event: ipc::DaemonEventType) {
    let Some(sender) = sender else {
        return;
    };
    let event = ipc::DaemonEvent::new(Utc::now().to_rfc3339(), event);
    let _ = sender.send(event);
}

fn emit_model_loaded_event(
    sender: Option<&mpsc::Sender<ipc::DaemonEvent>>,
    model_pool: &ModelPool,
    language: &str,
) {
    let Some((model_size, model_language)) = model_pool.metadata_for(language) else {
        return;
    };
    emit_daemon_event(
        sender,
        ipc::DaemonEventType::ModelLoaded {
            language: language.to_string(),
            model_size: model_size.to_string(),
            model_language: model_language.to_string(),
        },
    );
}

fn active_transcriber<'a>(
    model_pool: &'a ModelPool,
    language: &str,
) -> Result<&'a dyn Transcriber, AppError> {
    model_pool.transcriber_for(language).ok_or_else(|| {
        AppError::runtime(format!(
            "no model loaded for language '{language}'; set-language first"
        ))
    })
}

fn normalize_language(language: &str) -> String {
    language.trim().to_ascii_lowercase()
}

fn emit_transcript(
    config: &DaemonConfig,
    output: &mut dyn DaemonOutput,
    text: &str,
    info: audio::SegmentInfo,
) -> Result<(), String> {
    match config.mode {
        OutputMode::Stdout => emit_stdout(config.format, output, text, info),
        OutputMode::Inject => {
            if let Err(err) = output::inject_text(text) {
                output.stderr(&format!("warn: {err}; falling back to stdout"));
                emit_stdout(config.format, output, text, info)
            } else {
                Ok(())
            }
        }
    }
}

fn emit_stdout(
    format: OutputFormat,
    output: &mut dyn DaemonOutput,
    text: &str,
    info: audio::SegmentInfo,
) -> Result<(), String> {
    match format {
        OutputFormat::Plain => {
            output.stdout(&format!("Transcript {}: {}", info.index, text));
        }
        OutputFormat::Jsonl => {
            let escaped = json_escape(text);
            let timestamp = Utc::now().to_rfc3339();
            output.stdout(&format!(
                "{{\"type\":\"final\",\"utterance\":{},\"duration_ms\":{},\"timestamp\":\"{}\",\"text\":\"{}\"}}",
                info.index, info.duration_ms, timestamp, escaped
            ));
        }
    }
    Ok(())
}

fn json_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn dump_audio_samples(
    samples: &[f32],
    sample_rate: u32,
    output: &mut dyn DaemonOutput,
) -> Result<PathBuf, AppError> {
    let output_dir = env::current_dir()
        .map_err(|err| AppError::runtime(format!("failed to read current dir: {err}")))?;
    let timestamp = Local::now().format("%Y%m%d_%H%M%S").to_string();
    let filename = format!("sv_{timestamp}.wav");
    let path = output_dir.join(filename);
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&path, spec)
        .map_err(|err| AppError::runtime(format!("failed to create wav file: {err}")))?;
    for sample in samples {
        let clipped = sample.clamp(-1.0, 1.0);
        let value = (clipped * i16::MAX as f32) as i16;
        writer
            .write_sample(value)
            .map_err(|err| AppError::runtime(format!("failed to write wav data: {err}")))?;
    }
    writer
        .finalize()
        .map_err(|err| AppError::runtime(format!("failed to finalize wav: {err}")))?;
    output.stdout(&format!("Saved audio: {}", path.display()));
    Ok(path)
}

pub struct SocketGuard {
    path: PathBuf,
}

impl Drop for SocketGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub fn daemon_socket_path() -> Result<PathBuf, AppError> {
    let runtime_dir = env::var_os("XDG_RUNTIME_DIR").ok_or_else(|| {
        AppError::runtime(
            "XDG_RUNTIME_DIR is not set; set it to a writable runtime dir (e.g. /run/user/$(id -u))",
        )
    })?;
    Ok(PathBuf::from(runtime_dir)
        .join("soundvibes")
        .join("sv.sock"))
}

pub fn daemon_events_socket_path() -> Result<PathBuf, AppError> {
    let runtime_dir = env::var_os("XDG_RUNTIME_DIR").ok_or_else(|| {
        AppError::runtime(
            "XDG_RUNTIME_DIR is not set; set it to a writable runtime dir (e.g. /run/user/$(id -u))",
        )
    })?;
    Ok(PathBuf::from(runtime_dir)
        .join("soundvibes")
        .join("sv-events.sock"))
}

pub fn start_events_socket_listener(
    socket_path: &Path,
) -> Result<(SocketGuard, mpsc::Sender<ipc::DaemonEvent>), AppError> {
    prepare_socket_path(socket_path, "daemon events socket", None)?;

    let listener = UnixListener::bind(socket_path).map_err(|err| {
        AppError::runtime(format!(
            "failed to bind daemon events socket {}: {err}",
            socket_path.display()
        ))
    })?;
    listener.set_nonblocking(true).map_err(|err| {
        AppError::runtime(format!(
            "failed to set daemon events socket to nonblocking mode {}: {err}",
            socket_path.display()
        ))
    })?;
    let guard = SocketGuard {
        path: socket_path.to_path_buf(),
    };
    let (event_sender, event_receiver) = mpsc::channel();

    thread::spawn(move || {
        let mut subscribers: Vec<UnixStream> = Vec::new();
        loop {
            accept_event_subscribers(&listener, &mut subscribers);
            match event_receiver.recv_timeout(Duration::from_millis(20)) {
                Ok(event) => {
                    let Ok(line) = ipc::to_json_line(&event) else {
                        continue;
                    };
                    subscribers.retain_mut(|stream| stream.write_all(line.as_bytes()).is_ok());
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
    });

    Ok((guard, event_sender))
}

fn accept_event_subscribers(listener: &UnixListener, subscribers: &mut Vec<UnixStream>) {
    loop {
        match listener.accept() {
            Ok((stream, _addr)) => {
                let _ = stream.set_write_timeout(Some(Duration::from_millis(50)));
                subscribers.push(stream);
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(_) => break,
        }
    }
}

fn prepare_socket_path(
    socket_path: &Path,
    socket_name: &str,
    active_error: Option<&str>,
) -> Result<(), AppError> {
    if let Some(parent) = socket_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            AppError::runtime(format!(
                "failed to create socket directory {}: {err}",
                parent.display()
            ))
        })?;
    }

    if socket_path.exists() {
        if UnixStream::connect(socket_path).is_ok() {
            if let Some(active_error) = active_error {
                return Err(AppError::runtime(active_error));
            }
            return Err(AppError::runtime(format!(
                "{socket_name} already active at {}",
                socket_path.display()
            )));
        }
        fs::remove_file(socket_path).map_err(|err| {
            AppError::runtime(format!(
                "failed to remove stale socket {}: {err}",
                socket_path.display()
            ))
        })?;
    }

    Ok(())
}

pub fn start_socket_listener(
    socket_path: &Path,
) -> Result<(SocketGuard, Receiver<ControlMessage>), AppError> {
    prepare_socket_path(
        socket_path,
        "daemon control socket",
        Some("daemon already running; use `sv` to toggle capture"),
    )?;

    let listener = UnixListener::bind(socket_path).map_err(|err| {
        AppError::runtime(format!(
            "failed to bind daemon socket {}: {err}",
            socket_path.display()
        ))
    })?;
    let guard = SocketGuard {
        path: socket_path.to_path_buf(),
    };
    let (sender, receiver) = mpsc::channel();

    thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(mut stream) => {
                    let mut buffer = String::new();
                    if let Err(err) = stream.read_to_string(&mut buffer) {
                        let _ = write_control_response(
                            &mut stream,
                            &control_error_response(
                                "read_error",
                                format!("socket read error: {err}"),
                            ),
                        );
                        continue;
                    }
                    let command = buffer.trim();

                    let event = match control_event_from_command(command) {
                        Ok(event) => event,
                        Err(message) => {
                            let _ = write_control_response(
                                &mut stream,
                                &control_error_response("invalid_request", message),
                            );
                            continue;
                        }
                    };

                    let (response_sender, response_receiver) = mpsc::channel();
                    if sender
                        .send(ControlMessage {
                            event,
                            response: Some(response_sender),
                        })
                        .is_err()
                    {
                        let _ = write_control_response(
                            &mut stream,
                            &control_error_response(
                                "listener_error",
                                "daemon loop not available".to_string(),
                            ),
                        );
                        break;
                    }

                    let response = response_receiver
                        .recv_timeout(Duration::from_secs(2))
                        .unwrap_or_else(|_| {
                            control_error_response(
                                "timeout",
                                "daemon response timed out".to_string(),
                            )
                        });
                    let _ = write_control_response(&mut stream, &response);
                }
                Err(err) => {
                    let _ = sender.send(ControlMessage {
                        event: ControlEvent::Error(format!("socket listener error: {err}")),
                        response: None,
                    });
                    break;
                }
            }
        }
    });

    Ok((guard, receiver))
}

fn control_event_from_command(command: &str) -> Result<ControlEvent, String> {
    let request = ipc::parse_control_request(command)?;
    match request.command {
        ipc::ControlCommand::Toggle { lang } => Ok(ControlEvent::Toggle { language: lang }),
        ipc::ControlCommand::Status => Ok(ControlEvent::Status),
        ipc::ControlCommand::SetLanguage { lang } => {
            Ok(ControlEvent::SetLanguage { language: lang })
        }
        ipc::ControlCommand::Stop => Ok(ControlEvent::Stop),
    }
}

fn write_control_response(
    stream: &mut UnixStream,
    response: &ipc::ControlResponse,
) -> Result<(), AppError> {
    let line = ipc::to_json_line(response)
        .map_err(|err| AppError::runtime(format!("failed to serialize control response: {err}")))?;
    stream
        .write_all(line.as_bytes())
        .map_err(|err| AppError::runtime(format!("failed to write control response: {err}")))
}

fn control_ok_response(state: &str, language: &str) -> ipc::ControlResponse {
    ipc::ControlResponse::ok(Some(state.to_string()), Some(language.to_string()))
}

fn control_error_response(
    error: impl Into<String>,
    message: impl Into<String>,
) -> ipc::ControlResponse {
    ipc::ControlResponse::error(error, message)
}

pub fn send_toggle_command(language: Option<&str>) -> Result<ipc::ControlResponse, AppError> {
    let command = match language {
        Some(language) if !language.trim().is_empty() => format!("toggle lang={language}"),
        _ => "toggle".to_string(),
    };
    send_daemon_command(&command)
}

pub fn send_status_command() -> Result<ipc::ControlResponse, AppError> {
    send_daemon_command("status")
}

pub fn send_set_language_command(language: &str) -> Result<ipc::ControlResponse, AppError> {
    send_daemon_command(&format!("set-language lang={language}"))
}

pub fn send_stop_command() -> Result<ipc::ControlResponse, AppError> {
    send_daemon_command("stop")
}

fn send_daemon_command(command: &str) -> Result<ipc::ControlResponse, AppError> {
    let socket_path = daemon_socket_path()?;
    if !socket_path.exists() {
        return Err(AppError::runtime(format!(
            "daemon socket not found at {}. Start it with `sv daemon start`",
            socket_path.display()
        )));
    }
    let mut stream = UnixStream::connect(&socket_path).map_err(|err| {
        AppError::runtime(format!(
            "daemon socket unavailable at {}. Start it with `sv daemon start` ({err})",
            socket_path.display()
        ))
    })?;
    let payload = format!("{command}\n");
    stream
        .write_all(payload.as_bytes())
        .map_err(|err| AppError::runtime(format!("failed to send {command}: {err}")))?;
    stream
        .shutdown(Shutdown::Write)
        .map_err(|err| AppError::runtime(format!("failed to finalize command write: {err}")))?;

    let mut response_line = String::new();
    stream
        .read_to_string(&mut response_line)
        .map_err(|err| AppError::runtime(format!("failed to read daemon response: {err}")))?;

    if response_line.trim().is_empty() {
        return Err(AppError::runtime("daemon returned an empty response"));
    }

    let response = ipc::parse_control_response(&response_line)
        .map_err(|err| AppError::runtime(format!("invalid daemon response: {err}")))?;
    if !response.ok {
        let message = response
            .message
            .clone()
            .or(response.error.clone())
            .unwrap_or_else(|| "unknown daemon error".to_string());
        return Err(AppError::runtime(message));
    }

    Ok(response)
}

fn model_size_token(size: ModelSize) -> &'static str {
    match size {
        ModelSize::Auto => "auto",
        ModelSize::Tiny => "tiny",
        ModelSize::Base => "base",
        ModelSize::Small => "small",
        ModelSize::Medium => "medium",
        ModelSize::Large => "large",
    }
}

fn model_language_token(model_language: ModelLanguage) -> &'static str {
    match model_language {
        ModelLanguage::Auto => "auto",
        ModelLanguage::En => "en",
    }
}

struct CpalAudioBackend;

impl AudioBackend for CpalAudioBackend {
    fn list_input_devices(&self, host: &cpal::Host) -> Result<Vec<String>, audio::AudioError> {
        audio::list_input_devices(host)
    }

    fn start_capture(
        &self,
        host: &cpal::Host,
        device_name: Option<&str>,
        sample_rate: u32,
    ) -> Result<Box<dyn CaptureSource>, audio::AudioError> {
        let capture = audio::start_capture(host, device_name, sample_rate)?;
        Ok(Box::new(CpalCapture { inner: capture }))
    }
}

struct CpalCapture {
    inner: audio::Capture,
}

impl CaptureSource for CpalCapture {
    fn drain(&mut self, output: &mut Vec<f32>) {
        audio::drain_samples(&mut self.inner, output);
    }
}

struct WhisperFactory;

impl TranscriberFactory for WhisperFactory {
    fn load(
        &self,
        spec: &ModelSpec,
        allow_download: bool,
    ) -> Result<Box<dyn Transcriber>, AppError> {
        let prepared = model::prepare_model(None, spec, allow_download)?;
        let model_path = prepared.path;
        let context = WhisperContext::from_file(&model_path)
            .map_err(|err| AppError::runtime(err.to_string()))?;
        Ok(Box::new(WhisperTranscriber { context }))
    }
}

struct WhisperTranscriber {
    context: WhisperContext,
}

impl Transcriber for WhisperTranscriber {
    fn transcribe(&self, samples: &[f32], language: Option<&str>) -> Result<String, AppError> {
        self.context
            .transcribe(samples, language)
            .map_err(|err| AppError::runtime(err.to_string()))
    }
}

#[cfg(any(test, feature = "test-support"))]
pub mod test_support {
    use std::collections::VecDeque;
    use std::sync::mpsc;
    use std::sync::{Arc, Mutex};

    use super::{
        AudioBackend, CaptureSource, ControlEvent, ControlMessage, DaemonOutput, Transcriber,
        TranscriberFactory,
    };
    use crate::audio::{AudioError, AudioErrorKind};
    use crate::error::AppError;
    use crate::model::ModelSpec;

    #[derive(Default)]
    pub struct TestOutput {
        stdout: Vec<String>,
        stderr: Vec<String>,
    }

    impl TestOutput {
        pub fn stdout_lines(&self) -> &[String] {
            &self.stdout
        }

        pub fn stderr_lines(&self) -> &[String] {
            &self.stderr
        }
    }

    impl DaemonOutput for TestOutput {
        fn stdout(&mut self, message: &str) {
            self.stdout.push(message.to_string());
        }

        fn stderr(&mut self, message: &str) {
            self.stderr.push(message.to_string());
        }
    }

    pub struct TestAudioBackend {
        devices: Vec<String>,
        chunks: Arc<Mutex<VecDeque<Vec<f32>>>>,
        start_error: Arc<Mutex<Option<AudioError>>>,
    }

    impl TestAudioBackend {
        pub fn new(devices: Vec<String>, chunks: Vec<Vec<f32>>) -> Self {
            Self {
                devices,
                chunks: Arc::new(Mutex::new(chunks.into())),
                start_error: Arc::new(Mutex::new(None)),
            }
        }

        pub fn with_start_error(devices: Vec<String>, error: AudioError) -> Self {
            Self {
                devices,
                chunks: Arc::new(Mutex::new(VecDeque::new())),
                start_error: Arc::new(Mutex::new(Some(error))),
            }
        }
    }

    impl AudioBackend for TestAudioBackend {
        fn list_input_devices(&self, _host: &cpal::Host) -> Result<Vec<String>, AudioError> {
            if self.devices.is_empty() {
                return Err(AudioError {
                    kind: AudioErrorKind::DeviceUnavailable,
                    message: "no input devices available".to_string(),
                });
            }
            Ok(self.devices.clone())
        }

        fn start_capture(
            &self,
            _host: &cpal::Host,
            device_name: Option<&str>,
            _sample_rate: u32,
        ) -> Result<Box<dyn CaptureSource>, AudioError> {
            if let Some(err) = self.start_error.lock().expect("audio error lock").take() {
                return Err(err);
            }
            if let Some(device) = device_name {
                if !self
                    .devices
                    .iter()
                    .any(|name| name.eq_ignore_ascii_case(device))
                {
                    return Err(AudioError {
                        kind: AudioErrorKind::DeviceNotFound,
                        message: format!("input device not found: {device}"),
                    });
                }
            }
            Ok(Box::new(TestCapture {
                chunks: Arc::clone(&self.chunks),
            }))
        }
    }

    struct TestCapture {
        chunks: Arc<Mutex<VecDeque<Vec<f32>>>>,
    }

    impl CaptureSource for TestCapture {
        fn drain(&mut self, output: &mut Vec<f32>) {
            if let Some(chunk) = self.chunks.lock().expect("audio chunk lock").pop_front() {
                output.extend(chunk);
            }
        }
    }

    #[derive(Clone)]
    pub struct TestTranscriberFactory {
        responses: Arc<Mutex<VecDeque<Result<String, AppError>>>>,
        loaded_specs: Arc<Mutex<Vec<(ModelSpec, bool)>>>,
        transcribe_languages: Arc<Mutex<Vec<Option<String>>>>,
    }

    impl TestTranscriberFactory {
        pub fn new(responses: Vec<String>) -> Self {
            let responses = responses.into_iter().map(Ok).collect();
            Self {
                responses: Arc::new(Mutex::new(responses)),
                loaded_specs: Arc::new(Mutex::new(Vec::new())),
                transcribe_languages: Arc::new(Mutex::new(Vec::new())),
            }
        }

        pub fn with_results(responses: Vec<Result<String, AppError>>) -> Self {
            Self {
                responses: Arc::new(Mutex::new(responses.into())),
                loaded_specs: Arc::new(Mutex::new(Vec::new())),
                transcribe_languages: Arc::new(Mutex::new(Vec::new())),
            }
        }

        pub fn load_count(&self) -> usize {
            self.loaded_specs.lock().expect("loaded specs lock").len()
        }

        pub fn transcribed_languages(&self) -> Vec<Option<String>> {
            self.transcribe_languages
                .lock()
                .expect("transcribed languages lock")
                .clone()
        }
    }

    impl TranscriberFactory for TestTranscriberFactory {
        fn load(
            &self,
            spec: &ModelSpec,
            allow_download: bool,
        ) -> Result<Box<dyn Transcriber>, AppError> {
            self.loaded_specs
                .lock()
                .expect("loaded specs lock")
                .push((*spec, allow_download));
            Ok(Box::new(TestTranscriber {
                responses: Arc::clone(&self.responses),
                transcribe_languages: Arc::clone(&self.transcribe_languages),
            }))
        }
    }

    struct TestTranscriber {
        responses: Arc<Mutex<VecDeque<Result<String, AppError>>>>,
        transcribe_languages: Arc<Mutex<Vec<Option<String>>>>,
    }

    impl Transcriber for TestTranscriber {
        fn transcribe(&self, _samples: &[f32], language: Option<&str>) -> Result<String, AppError> {
            self.transcribe_languages
                .lock()
                .expect("transcribed languages lock")
                .push(language.map(|value| value.to_string()));
            let next = self
                .responses
                .lock()
                .expect("transcriber responses lock")
                .pop_front();
            match next {
                Some(result) => result,
                None => Ok(String::new()),
            }
        }
    }

    pub fn control_channel() -> (mpsc::Sender<ControlMessage>, mpsc::Receiver<ControlMessage>) {
        mpsc::channel()
    }

    pub fn control_message(event: ControlEvent) -> ControlMessage {
        ControlMessage {
            event,
            response: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;
    use std::io::{Read, Write};
    use std::path::Path;
    use std::process;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex, OnceLock};
    use std::thread;
    use std::time::Duration;

    use super::test_support::{
        control_channel, control_message, TestAudioBackend, TestOutput, TestTranscriberFactory,
    };

    static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    struct EnvGuard {
        key: &'static str,
        previous: Option<std::ffi::OsString>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: &Path) -> Self {
            let previous = env::var_os(key);
            env::set_var(key, value);
            Self { key, previous }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => env::set_var(self.key, value),
                None => env::remove_var(self.key),
            }
        }
    }

    fn lock_tests() -> std::sync::MutexGuard<'static, ()> {
        TEST_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("test lock poisoned")
    }

    fn temp_runtime_dir() -> std::path::PathBuf {
        let mut dir = env::temp_dir();
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        dir.push(format!("soundvibes-daemon-test-{}-{stamp}", process::id()));
        dir
    }

    fn read_event_line(stream: &mut UnixStream) -> Result<String, AppError> {
        let mut line = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            match stream.read(&mut byte) {
                Ok(0) => break,
                Ok(_) => {
                    line.push(byte[0]);
                    if byte[0] == b'\n' {
                        break;
                    }
                }
                Err(err)
                    if err.kind() == std::io::ErrorKind::WouldBlock
                        || err.kind() == std::io::ErrorKind::TimedOut =>
                {
                    return Err(AppError::runtime("timed out waiting for daemon event"));
                }
                Err(err) => {
                    return Err(AppError::runtime(format!(
                        "failed to read daemon event: {err}"
                    )));
                }
            }
        }

        if line.is_empty() {
            return Err(AppError::runtime("daemon event stream closed"));
        }

        String::from_utf8(line)
            .map_err(|err| AppError::runtime(format!("invalid utf-8 daemon event: {err}")))
    }

    #[test]
    fn daemon_loop_emits_transcript_to_output() -> Result<(), AppError> {
        let (sender, receiver) = control_channel();
        let control_sender = sender.clone();
        let shutdown = Arc::new(AtomicBool::new(false));
        let mut output = TestOutput::default();
        let deps = DaemonDeps {
            audio: Box::new(TestAudioBackend::new(
                vec!["Mic".to_string()],
                vec![vec![0.2; 160]],
            )),
            transcriber_factory: Box::new(TestTranscriberFactory::new(vec!["hello".to_string()])),
        };
        let config = DaemonConfig {
            download_model: false,
            language: "en".to_string(),
            model_pool_languages: vec!["en".to_string()],
            device: None,
            audio_host: AudioHost::Default,
            sample_rate: 16_000,
            format: OutputFormat::Plain,
            mode: OutputMode::Stdout,
            vad: VadMode::Off,
            vad_silence_ms: 800,
            vad_threshold: 0.015,
            vad_chunk_ms: 250,
            debug_audio: false,
            debug_vad: false,
            dump_audio: false,
        };

        let shutdown_trigger = Arc::clone(&shutdown);
        let control_thread = thread::spawn(move || {
            let _ = control_sender.send(control_message(ControlEvent::Toggle { language: None }));
            let _ = control_sender.send(control_message(ControlEvent::Toggle { language: None }));
            thread::sleep(Duration::from_millis(50));
            shutdown_trigger.store(true, Ordering::Relaxed);
        });

        let result = run_daemon_loop(&config, &deps, &mut output, receiver, &shutdown, None);
        control_thread.join().expect("control thread failed");
        result?;

        assert!(output
            .stdout_lines()
            .iter()
            .any(|line| line.contains("Transcript 1: hello")));
        Ok(())
    }

    #[test]
    fn socket_toggle_and_status_return_json_responses() -> Result<(), AppError> {
        let _lock = lock_tests();
        let runtime_dir = temp_runtime_dir();
        fs::create_dir_all(&runtime_dir)
            .map_err(|err| AppError::runtime(format!("failed to create runtime dir: {err}")))?;
        let _env_guard = EnvGuard::set("XDG_RUNTIME_DIR", &runtime_dir);

        let socket_path = daemon_socket_path()?;
        let (_socket_guard, receiver) = start_socket_listener(&socket_path)?;

        let deps = DaemonDeps {
            audio: Box::new(TestAudioBackend::new(
                vec!["Mic".to_string()],
                vec![vec![0.2; 160]],
            )),
            transcriber_factory: Box::new(TestTranscriberFactory::new(vec!["hello".to_string()])),
        };
        let config = DaemonConfig {
            download_model: false,
            language: "en".to_string(),
            model_pool_languages: vec!["en".to_string()],
            device: None,
            audio_host: AudioHost::Default,
            sample_rate: 16_000,
            format: OutputFormat::Plain,
            mode: OutputMode::Stdout,
            vad: VadMode::Off,
            vad_silence_ms: 800,
            vad_threshold: 0.015,
            vad_chunk_ms: 250,
            debug_audio: false,
            debug_vad: false,
            dump_audio: false,
        };
        let client_thread = thread::spawn(move || -> Result<(), AppError> {
            let toggle_response = send_toggle_command(Some("fr"))?;
            assert!(toggle_response.ok);
            assert_eq!(toggle_response.state.as_deref(), Some("recording"));
            assert_eq!(toggle_response.language.as_deref(), Some("fr"));

            let status_response = send_status_command()?;
            assert!(status_response.ok);
            assert_eq!(status_response.state.as_deref(), Some("recording"));
            assert_eq!(status_response.language.as_deref(), Some("fr"));

            let _ = send_toggle_command(None)?;
            let _ = send_stop_command()?;
            Ok(())
        });

        let shutdown = Arc::new(AtomicBool::new(false));
        let mut output = TestOutput::default();
        run_daemon_loop(
            &config,
            &deps,
            &mut output,
            receiver,
            shutdown.as_ref(),
            None,
        )?;

        client_thread
            .join()
            .map_err(|_| AppError::runtime("client thread panicked"))??;
        Ok(())
    }

    #[test]
    fn socket_invalid_command_returns_json_error() -> Result<(), AppError> {
        let _lock = lock_tests();
        let runtime_dir = temp_runtime_dir();
        fs::create_dir_all(&runtime_dir)
            .map_err(|err| AppError::runtime(format!("failed to create runtime dir: {err}")))?;
        let _env_guard = EnvGuard::set("XDG_RUNTIME_DIR", &runtime_dir);

        let socket_path = daemon_socket_path()?;
        let (_socket_guard, _receiver) = start_socket_listener(&socket_path)?;
        let mut stream = UnixStream::connect(&socket_path)
            .map_err(|err| AppError::runtime(format!("failed to connect to socket: {err}")))?;
        stream
            .write_all(b"bogus\n")
            .map_err(|err| AppError::runtime(format!("failed to write command: {err}")))?;
        stream
            .shutdown(Shutdown::Write)
            .map_err(|err| AppError::runtime(format!("failed to close write side: {err}")))?;

        let mut response_line = String::new();
        stream
            .read_to_string(&mut response_line)
            .map_err(|err| AppError::runtime(format!("failed to read response: {err}")))?;
        let response = ipc::parse_control_response(&response_line)
            .map_err(|err| AppError::runtime(format!("failed to parse response: {err}")))?;

        assert!(!response.ok);
        assert_eq!(response.error.as_deref(), Some("invalid_request"));
        Ok(())
    }

    #[test]
    fn event_fanout_broadcasts_identical_events_to_multiple_clients() -> Result<(), AppError> {
        let _lock = lock_tests();
        let runtime_dir = temp_runtime_dir();
        fs::create_dir_all(&runtime_dir)
            .map_err(|err| AppError::runtime(format!("failed to create runtime dir: {err}")))?;
        let _env_guard = EnvGuard::set("XDG_RUNTIME_DIR", &runtime_dir);

        let socket_path = daemon_events_socket_path()?;
        let (_socket_guard, event_sender) = start_events_socket_listener(&socket_path)?;

        let mut first = UnixStream::connect(&socket_path)
            .map_err(|err| AppError::runtime(format!("failed to connect first client: {err}")))?;
        let mut second = UnixStream::connect(&socket_path)
            .map_err(|err| AppError::runtime(format!("failed to connect second client: {err}")))?;
        first
            .set_read_timeout(Some(Duration::from_secs(1)))
            .map_err(|err| AppError::runtime(format!("failed to set first timeout: {err}")))?;
        second
            .set_read_timeout(Some(Duration::from_secs(1)))
            .map_err(|err| AppError::runtime(format!("failed to set second timeout: {err}")))?;

        thread::sleep(Duration::from_millis(40));

        let expected =
            ipc::DaemonEvent::new("2026-02-06T10:00:00Z", ipc::DaemonEventType::DaemonReady);
        event_sender
            .send(expected.clone())
            .map_err(|err| AppError::runtime(format!("failed to send event: {err}")))?;

        let first_line = read_event_line(&mut first)?;
        let second_line = read_event_line(&mut second)?;

        let first_event = ipc::from_json_line::<ipc::DaemonEvent>(&first_line)
            .map_err(|err| AppError::runtime(format!("failed to parse first event: {err}")))?;
        let second_event = ipc::from_json_line::<ipc::DaemonEvent>(&second_line)
            .map_err(|err| AppError::runtime(format!("failed to parse second event: {err}")))?;

        assert_eq!(first_event, expected);
        assert_eq!(second_event, expected);
        Ok(())
    }

    #[test]
    fn event_fanout_drops_disconnected_clients_and_keeps_healthy_clients() -> Result<(), AppError> {
        let _lock = lock_tests();
        let runtime_dir = temp_runtime_dir();
        fs::create_dir_all(&runtime_dir)
            .map_err(|err| AppError::runtime(format!("failed to create runtime dir: {err}")))?;
        let _env_guard = EnvGuard::set("XDG_RUNTIME_DIR", &runtime_dir);

        let socket_path = daemon_events_socket_path()?;
        let (_socket_guard, event_sender) = start_events_socket_listener(&socket_path)?;

        let disconnected = UnixStream::connect(&socket_path).map_err(|err| {
            AppError::runtime(format!("failed to connect disconnected client: {err}"))
        })?;
        let mut healthy = UnixStream::connect(&socket_path)
            .map_err(|err| AppError::runtime(format!("failed to connect healthy client: {err}")))?;
        healthy
            .set_read_timeout(Some(Duration::from_secs(1)))
            .map_err(|err| AppError::runtime(format!("failed to set timeout: {err}")))?;

        drop(disconnected);
        thread::sleep(Duration::from_millis(40));

        let first_expected = ipc::DaemonEvent::new(
            "2026-02-06T10:00:01Z",
            ipc::DaemonEventType::RecordingStarted {
                language: "fr".to_string(),
            },
        );
        let second_expected = ipc::DaemonEvent::new(
            "2026-02-06T10:00:02Z",
            ipc::DaemonEventType::RecordingStopped {
                language: "fr".to_string(),
            },
        );
        event_sender
            .send(first_expected.clone())
            .map_err(|err| AppError::runtime(format!("failed to send first event: {err}")))?;
        event_sender
            .send(second_expected.clone())
            .map_err(|err| AppError::runtime(format!("failed to send second event: {err}")))?;

        let first_line = read_event_line(&mut healthy)?;
        let second_line = read_event_line(&mut healthy)?;

        let first_event = ipc::from_json_line::<ipc::DaemonEvent>(&first_line)
            .map_err(|err| AppError::runtime(format!("failed to parse first event: {err}")))?;
        let second_event = ipc::from_json_line::<ipc::DaemonEvent>(&second_line)
            .map_err(|err| AppError::runtime(format!("failed to parse second event: {err}")))?;

        assert_eq!(first_event, first_expected);
        assert_eq!(second_event, second_expected);
        Ok(())
    }

    #[test]
    fn model_pool_preloads_active_and_configured_languages() -> Result<(), AppError> {
        let transcriber_factory = TestTranscriberFactory::new(Vec::new());
        let deps = DaemonDeps {
            audio: Box::new(TestAudioBackend::new(vec!["Mic".to_string()], Vec::new())),
            transcriber_factory: Box::new(transcriber_factory.clone()),
        };
        let config = DaemonConfig {
            download_model: false,
            language: "sv".to_string(),
            model_pool_languages: vec!["en".to_string(), "fr".to_string()],
            device: None,
            audio_host: AudioHost::Default,
            sample_rate: 16_000,
            format: OutputFormat::Plain,
            mode: OutputMode::Stdout,
            vad: VadMode::Off,
            vad_silence_ms: 800,
            vad_threshold: 0.015,
            vad_chunk_ms: 250,
            debug_audio: false,
            debug_vad: false,
            dump_audio: false,
        };

        let model_pool = ModelPool::preload(&config, &deps)?;

        assert!(model_pool.transcriber_for("en").is_some());
        assert!(model_pool.transcriber_for("fr").is_some());
        assert!(model_pool.transcriber_for("sv").is_some());
        assert_eq!(transcriber_factory.load_count(), 3);
        Ok(())
    }

    #[test]
    fn model_pool_switches_active_language_for_transcription() -> Result<(), AppError> {
        let (sender, receiver) = control_channel();
        let control_sender = sender.clone();
        let shutdown = Arc::new(AtomicBool::new(false));
        let mut output = TestOutput::default();
        let transcriber_factory = TestTranscriberFactory::new(vec!["bonjour".to_string()]);
        let deps = DaemonDeps {
            audio: Box::new(TestAudioBackend::new(
                vec!["Mic".to_string()],
                vec![vec![0.2; 160]],
            )),
            transcriber_factory: Box::new(transcriber_factory.clone()),
        };
        let config = DaemonConfig {
            download_model: false,
            language: "en".to_string(),
            model_pool_languages: vec!["en".to_string(), "fr".to_string()],
            device: None,
            audio_host: AudioHost::Default,
            sample_rate: 16_000,
            format: OutputFormat::Plain,
            mode: OutputMode::Stdout,
            vad: VadMode::Off,
            vad_silence_ms: 800,
            vad_threshold: 0.015,
            vad_chunk_ms: 250,
            debug_audio: false,
            debug_vad: false,
            dump_audio: false,
        };

        let shutdown_trigger = Arc::clone(&shutdown);
        let control_thread = thread::spawn(move || {
            let _ = control_sender.send(control_message(ControlEvent::SetLanguage {
                language: "fr".to_string(),
            }));
            let _ = control_sender.send(control_message(ControlEvent::Toggle { language: None }));
            let _ = control_sender.send(control_message(ControlEvent::Toggle { language: None }));
            thread::sleep(Duration::from_millis(50));
            shutdown_trigger.store(true, Ordering::Relaxed);
        });

        let result = run_daemon_loop(&config, &deps, &mut output, receiver, &shutdown, None);
        control_thread.join().expect("control thread failed");
        result?;

        assert_eq!(transcriber_factory.load_count(), 2);
        assert_eq!(
            transcriber_factory.transcribed_languages(),
            vec![Some("fr".to_string())]
        );
        Ok(())
    }

    #[test]
    fn model_pool_loads_new_language_on_set_language() -> Result<(), AppError> {
        let (sender, receiver) = control_channel();
        let control_sender = sender.clone();
        let shutdown = Arc::new(AtomicBool::new(false));
        let mut output = TestOutput::default();
        let transcriber_factory = TestTranscriberFactory::new(vec!["hej".to_string()]);
        let deps = DaemonDeps {
            audio: Box::new(TestAudioBackend::new(
                vec!["Mic".to_string()],
                vec![vec![0.2; 160]],
            )),
            transcriber_factory: Box::new(transcriber_factory.clone()),
        };
        let config = DaemonConfig {
            download_model: false,
            language: "en".to_string(),
            model_pool_languages: vec!["en".to_string()],
            device: None,
            audio_host: AudioHost::Default,
            sample_rate: 16_000,
            format: OutputFormat::Plain,
            mode: OutputMode::Stdout,
            vad: VadMode::Off,
            vad_silence_ms: 800,
            vad_threshold: 0.015,
            vad_chunk_ms: 250,
            debug_audio: false,
            debug_vad: false,
            dump_audio: false,
        };

        let shutdown_trigger = Arc::clone(&shutdown);
        let control_thread = thread::spawn(move || {
            let _ = control_sender.send(control_message(ControlEvent::SetLanguage {
                language: "sv".to_string(),
            }));
            let _ = control_sender.send(control_message(ControlEvent::Toggle { language: None }));
            let _ = control_sender.send(control_message(ControlEvent::Toggle { language: None }));
            thread::sleep(Duration::from_millis(50));
            shutdown_trigger.store(true, Ordering::Relaxed);
        });

        let result = run_daemon_loop(&config, &deps, &mut output, receiver, &shutdown, None);
        control_thread.join().expect("control thread failed");
        result?;

        assert_eq!(transcriber_factory.load_count(), 2);
        assert_eq!(
            transcriber_factory.transcribed_languages(),
            vec![Some("sv".to_string())]
        );
        Ok(())
    }

    #[test]
    fn parses_toggle_request_command() {
        let event = control_event_from_command("toggle lang=sv").expect("expected parse success");
        assert_eq!(
            event,
            ControlEvent::Toggle {
                language: Some("sv".to_string())
            }
        );
    }

    #[test]
    fn rejects_toggle_request_command_with_unknown_token() {
        let err = control_event_from_command("toggle foo=bar").expect_err("expected parse error");
        assert!(err.contains("unexpected token"));
    }
}
