mod audio;

use clap::{Parser, ValueEnum};
use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process;
use sv::whisper::WhisperContext;

#[derive(Parser, Debug)]
#[command(name = "sv", version, about = "Offline speech-to-text CLI")]
struct Cli {
    #[arg(long, value_name = "PATH", required_unless_present = "list_devices")]
    model: Option<PathBuf>,

    #[arg(long, default_value = "en", value_name = "CODE")]
    language: String,

    #[arg(long, value_name = "NAME")]
    device: Option<String>,

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
}

#[derive(Debug, Clone)]
struct Config {
    model_path: Option<PathBuf>,
    language: String,
    device: Option<String>,
    sample_rate: u32,
    format: OutputFormat,
    vad: VadMode,
    vad_silence_ms: u64,
    vad_threshold: f32,
    vad_chunk_ms: u64,
    debug_audio: bool,
    debug_vad: bool,
    list_devices: bool,
}

impl Config {
    fn from_cli(cli: Cli) -> Self {
        Self {
            model_path: cli.model,
            language: cli.language,
            device: cli.device,
            sample_rate: cli.sample_rate,
            format: cli.format,
            vad: cli.vad,
            vad_silence_ms: cli.vad_silence_ms,
            vad_threshold: cli.vad_threshold,
            vad_chunk_ms: cli.vad_chunk_ms,
            debug_audio: cli.debug_audio,
            debug_vad: cli.debug_vad,
            list_devices: cli.list_devices,
        }
    }
}

#[derive(Debug, Copy, Clone, ValueEnum)]
enum OutputFormat {
    Plain,
    Jsonl,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, ValueEnum)]
enum VadMode {
    On,
    Off,
}

fn main() {
    let cli = Cli::parse();
    let config = Config::from_cli(cli);

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
    if let Some(device) = &config.device {
        println!("Device: {device}");
    }

    if let Err(err) = run_capture(&config) {
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

fn run_capture(config: &Config) -> Result<(), AppError> {
    let host = cpal::default_host();
    audio::configure_alsa_logging(config.debug_audio);
    let devices = audio::list_input_devices(&host).map_err(|err| AppError::audio(err.message))?;
    println!("Input devices:");
    for name in devices {
        println!("  - {name}");
    }

    if config.list_devices {
        return Ok(());
    }

    let model_path = config
        .model_path
        .as_ref()
        .ok_or_else(|| AppError::config("model path is required"))?;
    let context =
        WhisperContext::from_file(model_path).map_err(|err| AppError::runtime(err.to_string()))?;
    let capture = audio::start_capture(&host, config.device.as_deref(), config.sample_rate)
        .map_err(|err| match err.kind {
            audio::AudioErrorKind::DeviceNotFound if config.device.is_some() => {
                AppError::config(err.message)
            }
            _ => AppError::audio(err.message),
        })?;
    let vad = audio::VadConfig::new(
        config.vad == VadMode::On,
        config.vad_silence_ms,
        config.vad_threshold,
        config.vad_chunk_ms,
        config.debug_vad,
    );
    println!("Capturing audio stream. Press Ctrl+C to stop.");
    audio::stream_segments(capture, config.sample_rate, vad, |samples, info| {
        let transcript = context
            .transcribe(samples, Some(&config.language))
            .map_err(|err| AppError::runtime(err.to_string()))?;
        emit_transcript(config.format, &transcript, info)
    })
    .map_err(|err| AppError::runtime(err))?;
    Ok(())
}

fn emit_transcript(
    format: OutputFormat,
    text: &str,
    info: audio::SegmentInfo,
) -> Result<(), AppError> {
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
