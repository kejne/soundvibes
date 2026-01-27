use chrono::{Local, Utc};
use signal_hook::consts::signal::{SIGINT, SIGTERM};
use signal_hook::flag;
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::audio;
use crate::error::AppError;
use crate::output;
use crate::types::{AudioHost, OutputFormat, OutputMode, VadMode};
use crate::whisper::WhisperContext;

#[derive(Debug, Clone)]
pub struct DaemonConfig {
    pub model_path: Option<PathBuf>,
    pub language: String,
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
    fn load(&self, model_path: Option<&Path>) -> Result<Box<dyn Transcriber>, AppError>;
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
    Toggle,
    Error(String),
}

pub fn run_daemon(
    config: &DaemonConfig,
    deps: &DaemonDeps,
    output: &mut dyn DaemonOutput,
) -> Result<(), AppError> {
    let socket_path = daemon_socket_path()?;
    let (_guard, control_events) = start_socket_listener(&socket_path)?;
    output.stdout(&format!("Daemon listening on {}", socket_path.display()));

    let shutdown = Arc::new(AtomicBool::new(false));
    for signal in [SIGINT, SIGTERM] {
        flag::register(signal, Arc::clone(&shutdown)).map_err(|err| {
            AppError::runtime(format!("failed to register signal handler: {err}"))
        })?;
    }

    run_daemon_loop(config, deps, output, control_events, &shutdown)
}

pub fn run_daemon_loop(
    config: &DaemonConfig,
    deps: &DaemonDeps,
    output: &mut dyn DaemonOutput,
    control_events: Receiver<ControlEvent>,
    shutdown: &AtomicBool,
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

    let transcriber = deps
        .transcriber_factory
        .load(config.model_path.as_deref())?;
    let vad = audio::VadConfig::new(
        config.vad == VadMode::On,
        config.vad_silence_ms,
        config.vad_threshold,
        config.vad_chunk_ms,
        config.debug_vad,
    );

    let mut recording = false;
    let mut buffer = Vec::new();
    let mut utterance_index = 0u64;
    let mut capture: Option<Box<dyn CaptureSource>> = None;

    loop {
        if shutdown.load(Ordering::Relaxed) {
            if recording {
                stop_recording(
                    &*transcriber,
                    config,
                    &vad,
                    &mut capture,
                    &mut buffer,
                    &mut utterance_index,
                    output,
                )?;
            }
            output.stdout("Daemon shutting down.");
            break;
        }
        match control_events.recv_timeout(Duration::from_millis(20)) {
            Ok(ControlEvent::Toggle) => {
                if recording {
                    recording = false;
                    stop_recording(
                        &*transcriber,
                        config,
                        &vad,
                        &mut capture,
                        &mut buffer,
                        &mut utterance_index,
                        output,
                    )?;
                } else {
                    let new_capture = deps
                        .audio
                        .start_capture(&host, config.device.as_deref(), config.sample_rate)
                        .map_err(|err| match err.kind {
                            audio::AudioErrorKind::DeviceNotFound if config.device.is_some() => {
                                AppError::audio(err.message)
                            }
                            _ => AppError::audio(err.message),
                        })?;
                    recording = true;
                    buffer.clear();
                    capture = Some(new_capture);
                    output.stdout("Toggle on. Recording...");
                }
            }
            Ok(ControlEvent::Error(message)) => return Err(AppError::runtime(message)),
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
    vad: &audio::VadConfig,
    capture: &mut Option<Box<dyn CaptureSource>>,
    buffer: &mut Vec<f32>,
    utterance_index: &mut u64,
    output: &mut dyn DaemonOutput,
) -> Result<(), AppError> {
    let mut active = capture
        .take()
        .ok_or_else(|| AppError::runtime("capture stream missing"))?;
    active.drain(buffer);
    finalize_recording(transcriber, config, vad, buffer, utterance_index, output)?;
    Ok(())
}

fn finalize_recording(
    transcriber: &dyn Transcriber,
    config: &DaemonConfig,
    vad: &audio::VadConfig,
    buffer: &[f32],
    utterance_index: &mut u64,
    output: &mut dyn DaemonOutput,
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
        .transcribe(&trimmed, Some(&config.language))
        .map_err(|err| AppError::runtime(err.to_string()))?;
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
    output.stdout("Ready for next utterance.");
    Ok(())
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

pub fn start_socket_listener(
    socket_path: &Path,
) -> Result<(SocketGuard, Receiver<ControlEvent>), AppError> {
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
            return Err(AppError::runtime(
                "daemon already running; use `sv` to toggle capture",
            ));
        }
        fs::remove_file(socket_path).map_err(|err| {
            AppError::runtime(format!(
                "failed to remove stale daemon socket {}: {err}",
                socket_path.display()
            ))
        })?;
    }

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
                        eprintln!("socket read error: {err}");
                        continue;
                    }
                    let command = buffer.trim();
                    if command.is_empty() || command == "toggle" {
                        let _ = sender.send(ControlEvent::Toggle);
                    } else {
                        eprintln!("unsupported daemon command: {command}");
                    }
                }
                Err(err) => {
                    let _ =
                        sender.send(ControlEvent::Error(format!("socket listener error: {err}")));
                    break;
                }
            }
        }
    });

    Ok((guard, receiver))
}

pub fn send_toggle_command() -> Result<(), AppError> {
    let socket_path = daemon_socket_path()?;
    if !socket_path.exists() {
        return Err(AppError::runtime(format!(
            "daemon socket not found at {}. Start it with `sv --daemon`",
            socket_path.display()
        )));
    }
    let mut stream = UnixStream::connect(&socket_path).map_err(|err| {
        AppError::runtime(format!(
            "daemon socket unavailable at {}. Start it with `sv --daemon` ({err})",
            socket_path.display()
        ))
    })?;
    stream
        .write_all(b"toggle\n")
        .map_err(|err| AppError::runtime(format!("failed to send toggle: {err}")))?;
    Ok(())
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
    fn load(&self, model_path: Option<&Path>) -> Result<Box<dyn Transcriber>, AppError> {
        let model_path = model_path.ok_or_else(|| AppError::config("model path is required"))?;
        let context = WhisperContext::from_file(model_path)
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
    use std::path::Path;
    use std::sync::mpsc;
    use std::sync::{Arc, Mutex};

    use super::{
        AudioBackend, CaptureSource, ControlEvent, DaemonOutput, Transcriber, TranscriberFactory,
    };
    use crate::audio::{AudioError, AudioErrorKind};
    use crate::error::AppError;

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
    }

    impl TestTranscriberFactory {
        pub fn new(responses: Vec<String>) -> Self {
            let responses = responses.into_iter().map(Ok).collect();
            Self {
                responses: Arc::new(Mutex::new(responses)),
            }
        }

        pub fn with_results(responses: Vec<Result<String, AppError>>) -> Self {
            Self {
                responses: Arc::new(Mutex::new(responses.into())),
            }
        }
    }

    impl TranscriberFactory for TestTranscriberFactory {
        fn load(&self, _model_path: Option<&Path>) -> Result<Box<dyn Transcriber>, AppError> {
            Ok(Box::new(TestTranscriber {
                responses: Arc::clone(&self.responses),
            }))
        }
    }

    struct TestTranscriber {
        responses: Arc<Mutex<VecDeque<Result<String, AppError>>>>,
    }

    impl Transcriber for TestTranscriber {
        fn transcribe(
            &self,
            _samples: &[f32],
            _language: Option<&str>,
        ) -> Result<String, AppError> {
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

    pub fn control_channel() -> (mpsc::Sender<ControlEvent>, mpsc::Receiver<ControlEvent>) {
        mpsc::channel()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    use super::test_support::{
        control_channel, TestAudioBackend, TestOutput, TestTranscriberFactory,
    };

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
            model_path: None,
            language: "en".to_string(),
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
            let _ = control_sender.send(ControlEvent::Toggle);
            let _ = control_sender.send(ControlEvent::Toggle);
            thread::sleep(Duration::from_millis(50));
            shutdown_trigger.store(true, Ordering::Relaxed);
        });

        let result = run_daemon_loop(&config, &deps, &mut output, receiver, &shutdown);
        control_thread.join().expect("control thread failed");
        result?;

        assert!(output
            .stdout_lines()
            .iter()
            .any(|line| line.contains("Transcript 1: hello")));
        Ok(())
    }
}
