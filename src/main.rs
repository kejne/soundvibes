mod audio;

use chrono::Local;
use clap::parser::ValueSource;
use clap::{CommandFactory, FromArgMatches, Parser, ValueEnum};
use hound::WavSpec;
use serde::Deserialize;
use signal_hook::consts::signal::{SIGINT, SIGTERM};
use signal_hook::flag;
use std::env;
use std::error::Error;
use std::fmt;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use sv::whisper::WhisperContext;

#[derive(Parser, Debug)]
#[command(name = "sv", version, about = "Offline speech-to-text CLI")]
struct Cli {
    #[arg(long, value_name = "PATH")]
    model: Option<PathBuf>,

    #[arg(long, default_value = "en", value_name = "CODE")]
    language: String,

    #[arg(long, value_name = "NAME")]
    device: Option<String>,

    #[arg(long, value_name = "HOST")]
    audio_host: Option<AudioHost>,

    #[arg(long, default_value_t = 16_000, value_name = "HZ")]
    sample_rate: u32,

    #[arg(long, default_value = "plain", value_name = "MODE")]
    format: OutputFormat,

    #[arg(long, default_value = "on", value_name = "MODE")]
    vad: VadMode,

    #[arg(long, default_value_t = audio::DEFAULT_SILENCE_TIMEOUT_MS, value_name = "MS")]
    vad_silence_ms: u64,

    #[arg(long, default_value_t = audio::DEFAULT_VAD_THRESHOLD, value_name = "LEVEL")]
    vad_threshold: f32,

    #[arg(long, default_value_t = audio::DEFAULT_CHUNK_MS, value_name = "MS")]
    vad_chunk_ms: u64,

    #[arg(long, default_value_t = false)]
    debug_audio: bool,

    #[arg(long, default_value_t = false)]
    debug_vad: bool,

    #[arg(long, default_value_t = false)]
    list_devices: bool,

    #[arg(long, default_value_t = false)]
    dump_audio: bool,

    #[arg(long, default_value_t = false)]
    daemon: bool,
}

#[derive(Debug, Clone)]
struct Config {
    model_path: Option<PathBuf>,
    language: String,
    device: Option<String>,
    audio_host: AudioHost,
    sample_rate: u32,
    format: OutputFormat,
    vad: VadMode,
    vad_silence_ms: u64,
    vad_threshold: f32,
    vad_chunk_ms: u64,
    debug_audio: bool,
    debug_vad: bool,
    list_devices: bool,
    dump_audio: bool,
}

impl Config {
    fn from_sources(cli: Cli, matches: &clap::ArgMatches, file: FileConfig) -> Self {
        let language = if matches.value_source("language") == Some(ValueSource::CommandLine) {
            cli.language
        } else {
            file.language.unwrap_or(cli.language)
        };

        let device = if matches.value_source("device") == Some(ValueSource::CommandLine) {
            cli.device
        } else {
            cli.device.or(file.device)
        };

        let audio_host = if matches.value_source("audio_host") == Some(ValueSource::CommandLine) {
            cli.audio_host
                .unwrap_or_else(AudioHost::default_for_platform)
        } else {
            file.audio_host
                .or(cli.audio_host)
                .unwrap_or_else(AudioHost::default_for_platform)
        };

        let sample_rate = if matches.value_source("sample_rate") == Some(ValueSource::CommandLine) {
            cli.sample_rate
        } else {
            file.sample_rate.unwrap_or(cli.sample_rate)
        };

        let format = if matches.value_source("format") == Some(ValueSource::CommandLine) {
            cli.format
        } else {
            file.format.unwrap_or(cli.format)
        };

        let vad = if matches.value_source("vad") == Some(ValueSource::CommandLine) {
            cli.vad
        } else {
            file.vad.map(VadSetting::into_mode).unwrap_or(cli.vad)
        };

        let vad_silence_ms =
            if matches.value_source("vad_silence_ms") == Some(ValueSource::CommandLine) {
                cli.vad_silence_ms
            } else {
                file.vad_silence_ms.unwrap_or(cli.vad_silence_ms)
            };

        let vad_threshold =
            if matches.value_source("vad_threshold") == Some(ValueSource::CommandLine) {
                cli.vad_threshold
            } else {
                file.vad_threshold.unwrap_or(cli.vad_threshold)
            };

        let vad_chunk_ms = if matches.value_source("vad_chunk_ms") == Some(ValueSource::CommandLine)
        {
            cli.vad_chunk_ms
        } else {
            file.vad_chunk_ms.unwrap_or(cli.vad_chunk_ms)
        };

        let debug_audio = if matches.value_source("debug_audio") == Some(ValueSource::CommandLine) {
            cli.debug_audio
        } else {
            file.debug_audio.unwrap_or(cli.debug_audio)
        };

        let debug_vad = if matches.value_source("debug_vad") == Some(ValueSource::CommandLine) {
            cli.debug_vad
        } else {
            file.debug_vad.unwrap_or(cli.debug_vad)
        };

        let list_devices = if matches.value_source("list_devices") == Some(ValueSource::CommandLine)
        {
            cli.list_devices
        } else {
            file.list_devices.unwrap_or(cli.list_devices)
        };

        let dump_audio = if matches.value_source("dump_audio") == Some(ValueSource::CommandLine) {
            cli.dump_audio
        } else {
            file.dump_audio.unwrap_or(cli.dump_audio)
        };

        let model_path = if matches.value_source("model") == Some(ValueSource::CommandLine) {
            cli.model
        } else {
            file.model.or(cli.model)
        }
        .or_else(|| Some(default_model_path()));

        Self {
            model_path,
            language,
            device,
            audio_host,
            sample_rate,
            format,
            vad,
            vad_silence_ms,
            vad_threshold,
            vad_chunk_ms,
            debug_audio,
            debug_vad,
            list_devices,
            dump_audio,
        }
    }
}

#[derive(Debug, Copy, Clone, ValueEnum, Deserialize)]
#[serde(rename_all = "lowercase")]
enum OutputFormat {
    Plain,
    Jsonl,
}

#[derive(Debug, Copy, Clone, ValueEnum, Deserialize)]
#[serde(rename_all = "lowercase")]
enum AudioHost {
    Default,
    Alsa,
}

impl AudioHost {
    fn default_for_platform() -> Self {
        #[cfg(target_os = "linux")]
        {
            return AudioHost::Alsa;
        }
        #[cfg(not(target_os = "linux"))]
        {
            AudioHost::Default
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, ValueEnum, Deserialize)]
#[serde(rename_all = "lowercase")]
enum VadMode {
    On,
    Off,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum VadSetting {
    Bool(bool),
    Mode(VadMode),
}

impl VadSetting {
    fn into_mode(self) -> VadMode {
        match self {
            VadSetting::Bool(true) => VadMode::On,
            VadSetting::Bool(false) => VadMode::Off,
            VadSetting::Mode(mode) => mode,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FileConfig {
    model: Option<PathBuf>,
    language: Option<String>,
    device: Option<String>,
    audio_host: Option<AudioHost>,
    sample_rate: Option<u32>,
    format: Option<OutputFormat>,
    vad: Option<VadSetting>,
    vad_silence_ms: Option<u64>,
    vad_threshold: Option<f32>,
    vad_chunk_ms: Option<u64>,
    debug_audio: Option<bool>,
    debug_vad: Option<bool>,
    list_devices: Option<bool>,
    dump_audio: Option<bool>,
}

fn main() {
    let matches = Cli::command().get_matches();
    let cli = Cli::from_arg_matches(&matches).expect("Failed to parse CLI arguments");
    if !cli.daemon && !cli.list_devices {
        if let Err(err) = send_toggle_command() {
            eprintln!("error: {err}");
            process::exit(err.exit_code());
        }
        return;
    }
    let file_config = match load_config_file() {
        Ok(config) => config,
        Err(err) => {
            eprintln!("error: {err}");
            process::exit(err.exit_code());
        }
    };
    let config = Config::from_sources(cli, &matches, file_config);

    if !config.list_devices {
        if let Some(model_path) = &config.model_path {
            if let Err(err) = validate_model_path(model_path) {
                eprintln!("error: {err}");
                process::exit(err.exit_code());
            }
        }
    }

    println!("SoundVibes sv {}", env!("CARGO_PKG_VERSION"));
    if let Some(model_path) = &config.model_path {
        println!("Model: {}", model_path.display());
    }
    println!("Language: {}", config.language);
    println!("Sample rate: {} Hz", config.sample_rate);
    println!("Format: {:?}", config.format);
    println!("VAD: {:?}", config.vad);
    println!("VAD silence timeout: {} ms", config.vad_silence_ms);
    println!("VAD threshold: {:.4}", config.vad_threshold);
    println!("VAD chunk: {} ms", config.vad_chunk_ms);
    println!("Dump audio: {}", config.dump_audio);
    println!("Audio host: {:?}", config.audio_host);
    if let Some(device) = &config.device {
        println!("Device: {device}");
    }

    let result = if config.list_devices {
        run_list_devices(&config)
    } else {
        run_daemon(&config)
    };

    if let Err(err) = result {
        eprintln!("error: {err}");
        process::exit(err.exit_code());
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum AppErrorKind {
    Config,
    Audio,
    Runtime,
}

#[derive(Debug)]
struct AppError {
    kind: AppErrorKind,
    message: String,
}

impl AppError {
    fn config(message: impl Into<String>) -> Self {
        Self {
            kind: AppErrorKind::Config,
            message: message.into(),
        }
    }

    fn audio(message: impl Into<String>) -> Self {
        Self {
            kind: AppErrorKind::Audio,
            message: message.into(),
        }
    }

    fn runtime(message: impl Into<String>) -> Self {
        Self {
            kind: AppErrorKind::Runtime,
            message: message.into(),
        }
    }

    fn exit_code(&self) -> i32 {
        match self.kind {
            AppErrorKind::Config => 2,
            AppErrorKind::Audio => 3,
            AppErrorKind::Runtime => 1,
        }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Error for AppError {}

#[derive(Debug)]
enum ControlEvent {
    Toggle,
    Error(String),
}
fn load_config_file() -> Result<FileConfig, AppError> {
    let path = match config_path() {
        Some(path) => path,
        None => return Ok(FileConfig::default()),
    };

    if !path.exists() {
        return Ok(FileConfig::default());
    }

    let contents = fs::read_to_string(&path).map_err(|err| {
        AppError::config(format!(
            "failed to read config file {}: {err}",
            path.display()
        ))
    })?;
    toml::from_str(&contents).map_err(|err| {
        AppError::config(format!(
            "failed to parse config file {}: {err}",
            path.display()
        ))
    })
}

fn config_path() -> Option<PathBuf> {
    let config_home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(config_home.join("soundvibes").join("config.toml"))
}

fn default_model_path() -> PathBuf {
    let data_home = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    data_home
        .join("soundvibes")
        .join("models")
        .join("ggml-base.en.bin")
}

fn select_audio_host(audio_host: AudioHost) -> Result<cpal::Host, AppError> {
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

fn run_list_devices(config: &Config) -> Result<(), AppError> {
    let host = select_audio_host(config.audio_host)?;
    audio::configure_alsa_logging(config.debug_audio);
    let devices = audio::list_input_devices(&host).map_err(|err| AppError::audio(err.message))?;
    println!("Input devices:");
    for name in devices {
        println!("  - {name}");
    }
    Ok(())
}

fn run_daemon(config: &Config) -> Result<(), AppError> {
    let socket_path = daemon_socket_path()?;
    let (_guard, control_events) = start_socket_listener(&socket_path)?;
    println!("Daemon listening on {}", socket_path.display());

    let shutdown = Arc::new(AtomicBool::new(false));
    for signal in [SIGINT, SIGTERM] {
        flag::register(signal, Arc::clone(&shutdown)).map_err(|err| {
            AppError::runtime(format!("failed to register signal handler: {err}"))
        })?;
    }

    let host = select_audio_host(config.audio_host)?;
    audio::configure_alsa_logging(config.debug_audio);
    let devices = audio::list_input_devices(&host).map_err(|err| AppError::audio(err.message))?;
    println!("Input devices:");
    for name in devices {
        println!("  - {name}");
    }

    let model_path = config
        .model_path
        .as_ref()
        .ok_or_else(|| AppError::config("model path is required"))?;
    let context =
        WhisperContext::from_file(model_path).map_err(|err| AppError::runtime(err.to_string()))?;
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
    let mut capture: Option<audio::Capture> = None;

    loop {
        if shutdown.load(Ordering::Relaxed) {
            if recording {
                stop_recording(
                    &context,
                    config,
                    &vad,
                    &mut capture,
                    &mut buffer,
                    &mut utterance_index,
                )?;
            }
            println!("Daemon shutting down.");
            break;
        }
        match control_events.recv_timeout(Duration::from_millis(20)) {
            Ok(ControlEvent::Toggle) => {
                if recording {
                    recording = false;
                    stop_recording(
                        &context,
                        config,
                        &vad,
                        &mut capture,
                        &mut buffer,
                        &mut utterance_index,
                    )?;
                } else {
                    let new_capture =
                        audio::start_capture(&host, config.device.as_deref(), config.sample_rate)
                            .map_err(|err| match err.kind {
                            audio::AudioErrorKind::DeviceNotFound if config.device.is_some() => {
                                AppError::config(err.message)
                            }
                            _ => AppError::audio(err.message),
                        })?;
                    recording = true;
                    buffer.clear();
                    capture = Some(new_capture);
                    println!("Toggle on. Recording...");
                }
            }
            Ok(ControlEvent::Error(message)) => return Err(AppError::runtime(message)),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                return Err(AppError::runtime("socket listener disconnected"));
            }
        }

        if recording {
            if let Some(active) = capture.as_mut() {
                audio::drain_samples(active, &mut buffer);
            }
        }
    }
    Ok(())
}

fn stop_recording(
    context: &WhisperContext,
    config: &Config,
    vad: &audio::VadConfig,
    capture: &mut Option<audio::Capture>,
    buffer: &mut Vec<f32>,
    utterance_index: &mut u64,
) -> Result<(), AppError> {
    let mut active = capture
        .take()
        .ok_or_else(|| AppError::runtime("capture stream missing"))?;
    audio::drain_samples(&mut active, buffer);
    finalize_recording(context, config, vad, buffer, utterance_index)?;
    Ok(())
}

fn finalize_recording(
    context: &WhisperContext,
    config: &Config,
    vad: &audio::VadConfig,
    buffer: &[f32],
    utterance_index: &mut u64,
) -> Result<(), AppError> {
    let trimmed = audio::trim_trailing_silence(buffer, config.sample_rate, vad);
    if trimmed.is_empty() {
        return Ok(());
    }
    *utterance_index += 1;
    let duration_ms = audio::samples_to_ms(trimmed.len(), config.sample_rate);
    if config.dump_audio {
        dump_audio_samples(&trimmed, config.sample_rate)?;
    }
    let transcript = context
        .transcribe(&trimmed, Some(&config.language))
        .map_err(|err| AppError::runtime(err.to_string()))?;
    emit_transcript(
        config.format,
        &transcript,
        audio::SegmentInfo {
            index: *utterance_index,
            duration_ms,
        },
    )
    .map_err(AppError::runtime)?;
    println!("Ready for next utterance.");
    Ok(())
}

struct SocketGuard {
    path: PathBuf,
}

impl Drop for SocketGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn daemon_socket_path() -> Result<PathBuf, AppError> {
    let runtime_dir = env::var_os("XDG_RUNTIME_DIR").ok_or_else(|| {
        AppError::runtime(
            "XDG_RUNTIME_DIR is not set; set it to a writable runtime dir (e.g. /run/user/$(id -u))",
        )
    })?;
    Ok(PathBuf::from(runtime_dir)
        .join("soundvibes")
        .join("sv.sock"))
}

fn start_socket_listener(
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

fn send_toggle_command() -> Result<(), AppError> {
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

fn emit_transcript(
    format: OutputFormat,
    text: &str,
    info: audio::SegmentInfo,
) -> Result<(), String> {
    match format {
        OutputFormat::Plain => {
            println!("Transcript {}: {}", info.index, text);
        }
        OutputFormat::Jsonl => {
            let escaped = json_escape(text);
            println!(
                "{{\"type\":\"final\",\"utterance\":{},\"duration_ms\":{},\"text\":\"{}\"}}",
                info.index, info.duration_ms, escaped
            );
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

fn dump_audio_samples(samples: &[f32], sample_rate: u32) -> Result<PathBuf, AppError> {
    let output_dir = env::current_dir()
        .map_err(|err| AppError::runtime(format!("failed to read current dir: {err}")))?;
    let timestamp = Local::now().format("%Y%m%d_%H%M%S").to_string();
    let filename = format!("sv_{timestamp}.wav");
    let path = output_dir.join(filename);
    let spec = WavSpec {
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
    println!("Saved audio: {}", path.display());
    Ok(path)
}

fn validate_model_path(path: &Path) -> Result<(), AppError> {
    if !path.exists() {
        return Err(AppError::config(format!(
            "model file not found at {}",
            path.display()
        )));
    }
    if !path.is_file() {
        return Err(AppError::config(format!(
            "model path is not a file: {}",
            path.display()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

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

    fn temp_runtime_dir() -> PathBuf {
        let mut dir = env::temp_dir();
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        dir.push(format!("soundvibes-test-{}-{stamp}", process::id()));
        dir
    }

    #[test]
    fn toggle_command_reaches_daemon_socket() -> Result<(), AppError> {
        let _lock = lock_tests();
        let runtime_dir = temp_runtime_dir();
        fs::create_dir_all(&runtime_dir).map_err(|err| {
            AppError::runtime(format!("failed to create test runtime dir: {err}"))
        })?;
        let _guard = EnvGuard::set("XDG_RUNTIME_DIR", &runtime_dir);
        let socket_path = daemon_socket_path()?;
        let (_socket_guard, control_events) = start_socket_listener(&socket_path)?;

        send_toggle_command()?;

        match control_events.recv_timeout(Duration::from_secs(1)) {
            Ok(ControlEvent::Toggle) => Ok(()),
            Ok(ControlEvent::Error(message)) => Err(AppError::runtime(message)),
            Err(_) => Err(AppError::runtime("toggle command not received")),
        }
    }

    #[test]
    fn toggle_command_errors_when_socket_missing() {
        let _lock = lock_tests();
        let runtime_dir = temp_runtime_dir();
        fs::create_dir_all(&runtime_dir).expect("failed to create test runtime dir");
        let _guard = EnvGuard::set("XDG_RUNTIME_DIR", &runtime_dir);

        let err = send_toggle_command().expect_err("expected socket error");
        assert!(err.to_string().contains("daemon socket not found"));
    }

    #[test]
    fn toggle_command_errors_when_socket_unavailable() {
        let _lock = lock_tests();
        let runtime_dir = temp_runtime_dir();
        fs::create_dir_all(&runtime_dir).expect("failed to create test runtime dir");
        let _guard = EnvGuard::set("XDG_RUNTIME_DIR", &runtime_dir);
        let socket_path = daemon_socket_path().expect("failed to compute socket path");
        if let Some(parent) = socket_path.parent() {
            fs::create_dir_all(parent).expect("failed to create socket dir");
        }
        fs::write(&socket_path, b"not-a-socket").expect("failed to create socket file");

        let err = send_toggle_command().expect_err("expected socket error");
        assert!(err.to_string().contains("daemon socket unavailable"));
        assert!(err.to_string().contains("sv --daemon"));
    }
}
