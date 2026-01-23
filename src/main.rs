use clap::{Parser, ValueEnum};
use std::path::{Path, PathBuf};
use std::process;

#[derive(Parser, Debug)]
#[command(name = "sv", version, about = "Offline speech-to-text CLI")]
struct Cli {
    #[arg(long, value_name = "PATH")]
    model: PathBuf,

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
}

#[derive(Debug, Clone)]
struct Config {
    model_path: PathBuf,
    language: String,
    device: Option<String>,
    sample_rate: u32,
    format: OutputFormat,
    vad: VadMode,
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
        }
    }
}

#[derive(Debug, Copy, Clone, ValueEnum)]
enum OutputFormat {
    Plain,
    Jsonl,
}

#[derive(Debug, Copy, Clone, ValueEnum)]
enum VadMode {
    On,
    Off,
}

fn main() {
    let cli = Cli::parse();
    let config = Config::from_cli(cli);

    if let Err(message) = validate_model_path(&config.model_path) {
        eprintln!("error: {message}");
        process::exit(2);
    }

    println!("SoundVibes sv {}", env!("CARGO_PKG_VERSION"));
    println!("Model: {}", config.model_path.display());
    println!("Language: {}", config.language);
    println!("Sample rate: {} Hz", config.sample_rate);
    println!("Format: {:?}", config.format);
    println!("VAD: {:?}", config.vad);
    if let Some(device) = &config.device {
        println!("Device: {device}");
    }
}

fn validate_model_path(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("model file not found at {}", path.display()));
    }
    if !path.is_file() {
        return Err(format!("model path is not a file: {}", path.display()));
    }
    Ok(())
}
