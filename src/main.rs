use clap::parser::ValueSource;
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use serde::Deserialize;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process;
use sv::audio;
use sv::daemon;
use sv::error::AppError;
use sv::model::ModelSize;
use sv::types::{AudioHost, OutputFormat, OutputMode, VadMode, VadSetting};

#[derive(Parser, Debug, Clone)]
#[command(name = "sv", version, about = "Offline speech-to-text CLI")]
struct Cli {
    #[arg(long, default_value = "small", value_name = "SIZE", global = true)]
    model_size: ModelSize,

    #[arg(long, default_value = "en", value_name = "CODE", global = true)]
    language: String,

    #[arg(long, value_name = "CODE", global = true)]
    toggle_language: Option<String>,

    #[arg(long, value_name = "NAME", global = true)]
    device: Option<String>,

    #[arg(long, value_name = "HOST", global = true)]
    audio_host: Option<AudioHost>,

    #[arg(long, default_value_t = 16_000, value_name = "HZ", global = true)]
    sample_rate: u32,

    #[arg(long, default_value = "plain", value_name = "MODE", global = true)]
    format: OutputFormat,

    #[arg(long, default_value = "inject", value_name = "MODE", global = true)]
    mode: OutputMode,

    #[arg(long, default_value = "on", value_name = "MODE", global = true)]
    vad: VadMode,

    #[arg(
        long,
        default_value_t = audio::DEFAULT_SILENCE_TIMEOUT_MS,
        value_name = "MS",
        global = true
    )]
    vad_silence_ms: u64,

    #[arg(
        long,
        default_value_t = audio::DEFAULT_VAD_THRESHOLD,
        value_name = "LEVEL",
        global = true
    )]
    vad_threshold: f32,

    #[arg(
        long,
        default_value_t = audio::DEFAULT_CHUNK_MS,
        value_name = "MS",
        global = true
    )]
    vad_chunk_ms: u64,

    #[arg(long, default_value_t = false, global = true)]
    debug_audio: bool,

    #[arg(long, default_value_t = false, global = true)]
    debug_vad: bool,

    #[arg(long, default_value_t = false, global = true)]
    list_devices: bool,

    #[arg(long, default_value_t = false, global = true)]
    dump_audio: bool,

    #[arg(long, default_value_t = true, action = clap::ArgAction::Set, global = true)]
    download_model: bool,

    #[command(subcommand)]
    command: Option<CliCommand>,
}

#[derive(Subcommand, Debug, Clone)]
enum CliCommand {
    Daemon {
        #[command(subcommand)]
        command: DaemonCommand,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
enum DaemonCommand {
    Start,
    Status,
    Stop,
    #[command(name = "set-language")]
    SetLanguage {
        #[arg(long = "lang", value_name = "CODE")]
        lang: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CliMode {
    Toggle,
    RunDaemon,
    StatusDaemon,
    StopDaemon,
    SetLanguage { language: String },
    ListDevices,
}

fn resolve_cli_mode(cli: &Cli) -> CliMode {
    match cli.command {
        Some(CliCommand::Daemon {
            command: DaemonCommand::Start,
        }) => CliMode::RunDaemon,
        Some(CliCommand::Daemon {
            command: DaemonCommand::Status,
        }) => CliMode::StatusDaemon,
        Some(CliCommand::Daemon {
            command: DaemonCommand::Stop,
        }) => CliMode::StopDaemon,
        Some(CliCommand::Daemon {
            command: DaemonCommand::SetLanguage { ref lang },
        }) => CliMode::SetLanguage {
            language: lang.clone(),
        },
        None => {
            if cli.list_devices {
                CliMode::ListDevices
            } else {
                CliMode::Toggle
            }
        }
    }
}

#[derive(Debug, Clone)]
struct Config {
    model_size: ModelSize,
    download_model: bool,
    language: String,
    toggle_language: Option<String>,
    model_pool_languages: Vec<String>,
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

        let toggle_language =
            if matches.value_source("toggle_language") == Some(ValueSource::CommandLine) {
                cli.toggle_language
            } else {
                None
            };

        let model_pool_languages = file
            .model_pool_languages
            .unwrap_or_else(|| vec![language.clone()]);
        let model_pool_languages = if model_pool_languages.is_empty() {
            vec![language.clone()]
        } else {
            model_pool_languages
        };

        let model_size = if matches.value_source("model_size") == Some(ValueSource::CommandLine) {
            cli.model_size
        } else {
            file.model_size.unwrap_or(cli.model_size)
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

        Self {
            model_size,
            download_model,
            language,
            toggle_language,
            model_pool_languages,
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
    model_size: Option<ModelSize>,
    download_model: Option<bool>,
    language: Option<String>,
    model_pool_languages: Option<Vec<String>>,
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
    let mode = resolve_cli_mode(&cli);

    match &mode {
        CliMode::StatusDaemon => {
            match daemon::send_status_command() {
                Ok(response) => {
                    println!(
                        "state={} language={}",
                        response.state.as_deref().unwrap_or("unknown"),
                        response.language.as_deref().unwrap_or("unknown")
                    );
                }
                Err(err) => {
                    eprintln!("error: {err}");
                    process::exit(err.exit_code());
                }
            }
            return;
        }
        CliMode::StopDaemon => {
            if let Err(err) = daemon::send_stop_command() {
                eprintln!("error: {err}");
                process::exit(err.exit_code());
            }
            return;
        }
        CliMode::SetLanguage { language } => {
            if let Err(err) = daemon::send_set_language_command(language) {
                eprintln!("error: {err}");
                process::exit(err.exit_code());
            }
            return;
        }
        CliMode::Toggle | CliMode::RunDaemon | CliMode::ListDevices => {}
    }

    let file_config = match load_config_file() {
        Ok(config) => config,
        Err(err) => {
            eprintln!("error: {err}");
            process::exit(err.exit_code());
        }
    };
    let mut config = Config::from_sources(cli, &matches, file_config);

    match &mode {
        CliMode::Toggle => {
            let language = config
                .toggle_language
                .as_deref()
                .unwrap_or(config.language.as_str());
            if let Err(err) = daemon::send_toggle_command(Some(language)) {
                eprintln!("error: {err}");
                process::exit(err.exit_code());
            }
            return;
        }
        CliMode::RunDaemon | CliMode::ListDevices => {}
        CliMode::StatusDaemon | CliMode::StopDaemon | CliMode::SetLanguage { .. } => unreachable!(),
    }

    if mode == CliMode::RunDaemon {
        config.list_devices = false;
    }

    println!("SoundVibes sv {}", env!("CARGO_PKG_VERSION"));
    println!("Model size: {:?}", config.model_size);
    println!("Language: {}", config.language);
    println!(
        "Model pool languages: {}",
        config.model_pool_languages.join(",")
    );
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
        let daemon_config = daemon::DaemonConfig {
            model_size: config.model_size,
            download_model: config.download_model,
            language: config.language.clone(),
            model_pool_languages: config.model_pool_languages.clone(),
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
    use std::thread;

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

    fn config_from_args_and_file(args: &[&str], file: FileConfig) -> Config {
        let matches = Cli::command().get_matches_from(args);
        let cli = Cli::from_arg_matches(&matches).expect("failed to parse cli args");
        Config::from_sources(cli, &matches, file)
    }

    #[test]
    fn toggle_command_reaches_daemon_socket() -> Result<(), AppError> {
        use std::io::{Read, Write};
        use std::os::unix::net::UnixListener;

        let _lock = lock_tests();
        let runtime_dir = temp_runtime_dir();
        fs::create_dir_all(&runtime_dir).map_err(|err| {
            AppError::runtime(format!("failed to create test runtime dir: {err}"))
        })?;
        let _guard = EnvGuard::set("XDG_RUNTIME_DIR", &runtime_dir);
        let socket_path = daemon::daemon_socket_path()?;
        if let Some(parent) = socket_path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                AppError::runtime(format!("failed to create socket parent: {err}"))
            })?;
        }

        let listener = UnixListener::bind(&socket_path)
            .map_err(|err| AppError::runtime(format!("failed to bind test socket: {err}")))?;
        let server_thread = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("expected client connection");
            let mut payload = String::new();
            stream
                .read_to_string(&mut payload)
                .expect("expected command payload");
            stream
                .write_all(b"{\"api_version\":\"1\",\"ok\":true,\"state\":\"recording\",\"language\":\"en\"}\n")
                .expect("expected response write");
            payload
        });

        let response = daemon::send_toggle_command(None)?;
        assert!(response.ok);
        assert_eq!(response.state.as_deref(), Some("recording"));

        let payload = server_thread
            .join()
            .map_err(|_| AppError::runtime("server thread panicked"))?;
        assert_eq!(payload.trim_end(), "toggle");
        Ok(())
    }

    #[test]
    fn toggle_command_errors_when_socket_missing() {
        let _lock = lock_tests();
        let runtime_dir = temp_runtime_dir();
        fs::create_dir_all(&runtime_dir).expect("failed to create test runtime dir");
        let _guard = EnvGuard::set("XDG_RUNTIME_DIR", &runtime_dir);

        let err = daemon::send_toggle_command(None).expect_err("expected socket error");
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

        let err = daemon::send_toggle_command(None).expect_err("expected socket error");
        assert!(err.to_string().contains("daemon socket unavailable"));
        assert!(err.to_string().contains("sv daemon start"));
    }

    #[test]
    fn defaults_to_toggle_when_no_subcommand() {
        let cli = Cli::try_parse_from(["sv"]).expect("failed to parse cli");
        assert_eq!(resolve_cli_mode(&cli), CliMode::Toggle);
    }

    #[test]
    fn model_pool_languages_default_to_active_language() {
        let config = config_from_args_and_file(
            &["sv"],
            FileConfig {
                language: Some("sv".to_string()),
                ..FileConfig::default()
            },
        );

        assert_eq!(config.model_pool_languages, vec!["sv".to_string()]);
    }

    #[test]
    fn model_size_defaults_to_small() {
        let config = config_from_args_and_file(&["sv"], FileConfig::default());
        assert_eq!(config.model_size, ModelSize::Small);
    }

    #[test]
    fn model_size_respects_config_and_cli_override() {
        let config = config_from_args_and_file(
            &["sv"],
            FileConfig {
                model_size: Some(ModelSize::Medium),
                ..FileConfig::default()
            },
        );
        assert_eq!(config.model_size, ModelSize::Medium);

        let cli_override = config_from_args_and_file(
            &["sv", "--model-size", "tiny"],
            FileConfig {
                model_size: Some(ModelSize::Large),
                ..FileConfig::default()
            },
        );
        assert_eq!(cli_override.model_size, ModelSize::Tiny);
    }

    #[test]
    fn model_pool_languages_respect_file_value() {
        let config = config_from_args_and_file(
            &["sv"],
            FileConfig {
                model_pool_languages: Some(vec!["en".to_string(), "fr".to_string()]),
                ..FileConfig::default()
            },
        );

        assert_eq!(
            config.model_pool_languages,
            vec!["en".to_string(), "fr".to_string()]
        );
    }

    #[test]
    fn empty_model_pool_languages_falls_back_to_active_language() {
        let config = config_from_args_and_file(
            &["sv", "--language", "fr"],
            FileConfig {
                model_pool_languages: Some(Vec::new()),
                ..FileConfig::default()
            },
        );

        assert_eq!(config.model_pool_languages, vec!["fr".to_string()]);
    }

    #[test]
    fn toggle_language_cli_override_is_captured() {
        let config =
            config_from_args_and_file(&["sv", "--toggle-language", "de"], FileConfig::default());

        assert_eq!(config.language, "en");
        assert_eq!(config.toggle_language.as_deref(), Some("de"));
    }

    #[test]
    fn parses_daemon_start_subcommand() {
        let cli = Cli::try_parse_from(["sv", "daemon", "start"]).expect("failed to parse cli");
        assert_eq!(resolve_cli_mode(&cli), CliMode::RunDaemon);
    }

    #[test]
    fn parses_daemon_stop_subcommand() {
        let cli = Cli::try_parse_from(["sv", "daemon", "stop"]).expect("failed to parse cli");
        assert_eq!(resolve_cli_mode(&cli), CliMode::StopDaemon);
    }

    #[test]
    fn parses_daemon_status_subcommand() {
        let cli = Cli::try_parse_from(["sv", "daemon", "status"]).expect("failed to parse cli");
        assert_eq!(resolve_cli_mode(&cli), CliMode::StatusDaemon);
    }

    #[test]
    fn parses_daemon_set_language_subcommand() {
        let cli = Cli::try_parse_from(["sv", "daemon", "set-language", "--lang", "fr"])
            .expect("failed to parse cli");
        assert_eq!(
            resolve_cli_mode(&cli),
            CliMode::SetLanguage {
                language: "fr".to_string(),
            }
        );
    }
}
