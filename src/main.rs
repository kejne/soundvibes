use clap::parser::ValueSource;
use clap::{CommandFactory, FromArgMatches, Parser};
use serde::Deserialize;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process;
use sv::audio;
use sv::daemon;
use sv::error::AppError;
use sv::model::{ModelLanguage, ModelSize, ModelSpec};
use sv::types::{AudioHost, OutputFormat, OutputMode, VadMode, VadSetting};

#[derive(Parser, Debug)]
#[command(name = "sv", version, about = "Offline speech-to-text CLI")]
struct Cli {
    #[arg(long, value_name = "PATH")]
    model: Option<PathBuf>,

    #[arg(long, default_value = "small", value_name = "SIZE")]
    model_size: ModelSize,

    #[arg(long, default_value = "auto", value_name = "LANG")]
    model_language: ModelLanguage,

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

    #[arg(long, default_value = "inject", value_name = "MODE")]
    mode: OutputMode,

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

    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    download_model: bool,

    #[arg(long, default_value_t = false)]
    daemon: bool,
}

#[derive(Debug, Clone)]
struct Config {
    model_path: Option<PathBuf>,
    model_size: ModelSize,
    model_language: ModelLanguage,
    download_model: bool,
    language: String,
    device: Option<String>,
    audio_host: AudioHost,
    sample_rate: u32,
    format: OutputFormat,
    mode: OutputMode,
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

        let model_size = if matches.value_source("model_size") == Some(ValueSource::CommandLine) {
            cli.model_size
        } else {
            file.model_size.unwrap_or(cli.model_size)
        };

        let model_language =
            if matches.value_source("model_language") == Some(ValueSource::CommandLine) {
                cli.model_language
            } else {
                file.model_language.unwrap_or(cli.model_language)
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

        let mode = if matches.value_source("mode") == Some(ValueSource::CommandLine) {
            cli.mode
        } else {
            file.mode.unwrap_or(cli.mode)
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

        let download_model =
            if matches.value_source("download_model") == Some(ValueSource::CommandLine) {
                cli.download_model
            } else {
                file.download_model.unwrap_or(cli.download_model)
            };

        let file_model_path = file.model_path.or(file.model);
        let model_path = if matches.value_source("model") == Some(ValueSource::CommandLine) {
            cli.model
        } else {
            cli.model.or(file_model_path)
        };

        Self {
            model_path,
            model_size,
            model_language,
            download_model,
            language,
            device,
            audio_host,
            sample_rate,
            format,
            mode,
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

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FileConfig {
    model: Option<PathBuf>,
    model_path: Option<PathBuf>,
    model_size: Option<ModelSize>,
    model_language: Option<ModelLanguage>,
    download_model: Option<bool>,
    language: Option<String>,
    device: Option<String>,
    audio_host: Option<AudioHost>,
    sample_rate: Option<u32>,
    format: Option<OutputFormat>,
    mode: Option<OutputMode>,
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
        if let Err(err) = daemon::send_toggle_command() {
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

    let prepared_model = if config.list_devices {
        None
    } else {
        let spec = ModelSpec::new(config.model_size, config.model_language);
        match sv::model::prepare_model(config.model_path.as_deref(), &spec, config.download_model) {
            Ok(prepared) => Some(prepared),
            Err(err) => {
                eprintln!("error: {err}");
                process::exit(err.exit_code());
            }
        }
    };

    println!("SoundVibes sv {}", env!("CARGO_PKG_VERSION"));
    if let Some(prepared) = &prepared_model {
        if prepared.downloaded {
            println!("Model download complete.");
        }
        println!("Model: {}", prepared.path.display());
    }
    println!("Language: {}", config.language);
    println!("Sample rate: {} Hz", config.sample_rate);
    println!("Format: {:?}", config.format);
    println!("Mode: {:?}", config.mode);
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
        let model_path = prepared_model
            .as_ref()
            .map(|prepared| prepared.path.clone());
        let daemon_config = daemon::DaemonConfig {
            model_path,
            language: config.language.clone(),
            device: config.device.clone(),
            audio_host: config.audio_host,
            sample_rate: config.sample_rate,
            format: config.format,
            mode: config.mode,
            vad: config.vad,
            vad_silence_ms: config.vad_silence_ms,
            vad_threshold: config.vad_threshold,
            vad_chunk_ms: config.vad_chunk_ms,
            debug_audio: config.debug_audio,
            debug_vad: config.debug_vad,
            dump_audio: config.dump_audio,
        };
        let deps = daemon::DaemonDeps::default();
        let mut output = daemon::StdoutOutput;
        daemon::run_daemon(&daemon_config, &deps, &mut output)
    };

    if let Err(err) = result {
        eprintln!("error: {err}");
        process::exit(err.exit_code());
    }
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

fn run_list_devices(config: &Config) -> Result<(), AppError> {
    let host = daemon::select_audio_host(config.audio_host)?;
    audio::configure_alsa_logging(config.debug_audio);
    let devices = audio::list_input_devices(&host).map_err(|err| AppError::audio(err.message))?;
    println!("Input devices:");
    for name in devices {
        println!("  - {name}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::{Mutex, OnceLock};
    use std::time::Duration;

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
        let socket_path = daemon::daemon_socket_path()?;
        let (_socket_guard, control_events) = daemon::start_socket_listener(&socket_path)?;

        daemon::send_toggle_command()?;

        match control_events.recv_timeout(Duration::from_secs(1)) {
            Ok(daemon::ControlEvent::Toggle) => Ok(()),
            Ok(daemon::ControlEvent::Error(message)) => Err(AppError::runtime(message)),
            Err(_) => Err(AppError::runtime("toggle command not received")),
        }
    }

    #[test]
    fn toggle_command_errors_when_socket_missing() {
        let _lock = lock_tests();
        let runtime_dir = temp_runtime_dir();
        fs::create_dir_all(&runtime_dir).expect("failed to create test runtime dir");
        let _guard = EnvGuard::set("XDG_RUNTIME_DIR", &runtime_dir);

        let err = daemon::send_toggle_command().expect_err("expected socket error");
        assert!(err.to_string().contains("daemon socket not found"));
    }

    #[test]
    fn toggle_command_errors_when_socket_unavailable() {
        let _lock = lock_tests();
        let runtime_dir = temp_runtime_dir();
        fs::create_dir_all(&runtime_dir).expect("failed to create test runtime dir");
        let _guard = EnvGuard::set("XDG_RUNTIME_DIR", &runtime_dir);
        let socket_path = daemon::daemon_socket_path().expect("failed to compute socket path");
        if let Some(parent) = socket_path.parent() {
            fs::create_dir_all(parent).expect("failed to create socket dir");
        }
        fs::write(&socket_path, b"not-a-socket").expect("failed to create socket file");

        let err = daemon::send_toggle_command().expect_err("expected socket error");
        assert!(err.to_string().contains("daemon socket unavailable"));
        assert!(err.to_string().contains("sv --daemon"));
    }
}
