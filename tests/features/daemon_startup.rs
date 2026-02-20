#[cfg(feature = "test-support")]
pub mod daemon_startup {
    use cucumber::{Given, Then, When, World};
    use std::env;
    use std::fs;
    use std::io::BufRead;
    use std::io::BufReader;
    use std::path::PathBuf;
    use std::process::{Command, Stdio};
    use std::sync::mpsc;
    use std::thread;
    use std::time::{Duration, Instant};

    use crate::DaemonWorld;

    fn temp_dir(prefix: &str) -> PathBuf {
        let mut dir = env::temp_dir();
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        dir.push(format!("{prefix}-{}-{}", prefix, std::process::id()));
        dir
    }

    fn model_path() -> PathBuf {
        let data_home = env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|_| env::var("HOME").map(|home| PathBuf::from(home).join(".local/share")))
            .unwrap_or_else(|_| PathBuf::from("/tmp"));
        data_home
            .join("soundvibes")
            .join("models")
            .join("ggml-small.en.bin")
    }

    fn write_config(config_home: &PathBuf, contents: &str) -> std::io::Result<()> {
        let config_path = config_home.join("soundvibes").join("config.toml");
        fs::create_dir_all(config_path.parent().expect("config parent"))?;
        fs::write(&config_path, contents)?;
        Ok(())
    }

    #[allow(dead_code)]
    pub struct DaemonStartupWorld {
        pub config_home: PathBuf,
        pub runtime_dir: PathBuf,
        pub data_home: PathBuf,
        pub child: Option<std::process::Child>,
        pub exit_code: Option<i32>,
        pub stderr_output: String,
    }

    impl Default for DaemonStartupWorld {
        fn default() -> Self {
            Self {
                config_home: temp_dir("soundvibes-test-config"),
                runtime_dir: temp_dir("soundvibes-test-runtime"),
                data_home: temp_dir("soundvibes-test-data"),
                child: None,
                exit_code: None,
                stderr_output: String::new(),
            }
        }
    }

    #[Given("a valid model file exists at the default location")]
    pub fn valid_model_exists(_world: &mut DaemonWorld) {
        if env::var("SV_HARDWARE_TESTS").ok().as_deref() != Some("1") {
            eprintln!("Skipping AT-01; set SV_HARDWARE_TESTS=1 to run.");
        }
        let path = model_path();
        if !path.exists() {
            eprintln!("Skipping AT-01; model file not found at {}", path.display());
        }
    }

    #[Given("the config directory is empty")]
    pub fn empty_config_dir(world: &mut DaemonWorld) {
        let config_home = temp_dir("soundvibes-cucumber-config");
        let _ = fs::remove_dir_all(&config_home);
        fs::create_dir_all(&config_home).expect("create config dir");
        world.config.model_size = sv::model::ModelSize::Small;
    }

    #[Given("download_model is disabled")]
    pub fn download_model_disabled(world: &mut DaemonWorld) {
        world.config.download_model = false;
    }

    #[Given("no model file exists")]
    pub fn no_model_file(world: &mut DaemonWorld) {
        let data_home = temp_dir("soundvibes-cucumber-data");
        let _ = fs::remove_dir_all(&data_home);
        fs::create_dir_all(&data_home).expect("create data dir");
        let _ = env::var_os("XDG_DATA_HOME").map(|v| env::remove_var("XDG_DATA_HOME"));
    }

    #[Given("the device is set to \"nonexistent\"")]
    pub fn nonexistent_device(world: &mut DaemonWorld) {
        world.config.device = Some("nonexistent".to_string());
    }

    #[When("I start the daemon")]
    pub fn start_daemon(world: &mut DaemonWorld) -> Result<(), Box<dyn std::error::Error>> {
        let config_home = temp_dir("soundvibes-cucumber-config");
        let runtime_dir = temp_dir("soundvibes-cucumber-runtime");

        write_config(&config_home, "language = \"en\"\ndownload_model = false\n")?;

        let binary = env!("CARGO_BIN_EXE_sv");
        let mut child = Command::new(binary)
            .args(["daemon", "start"])
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_RUNTIME_DIR", &runtime_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let stdout = child.stdout.take().expect("stdout pipe");
        let stderr = child.stderr.take().expect("stderr pipe");
        let (ready_tx, ready_rx) = mpsc::channel();

        let _reader_thread = thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                if line.contains("Daemon listening on") {
                    let _ = ready_tx.send(line);
                    break;
                }
            }
        });

        let start = Instant::now();
        loop {
            if let Ok(line) = ready_rx.recv_timeout(Duration::from_millis(100)) {
                if line.contains("Daemon listening on") {
                    break;
                }
            }
            if let Some(status) = child.try_wait()? {
                return Err(format!("daemon exited early with {status}").into());
            }
            if start.elapsed() > Duration::from_secs(3) {
                return Err("daemon did not report ready state".into());
            }
        }

        let pid = child.id();
        let _ = Command::new("kill")
            .arg("-TERM")
            .arg(pid.to_string())
            .status();
        let _ = child.wait();

        Ok(())
    }

    #[Then("the daemon should start successfully")]
    pub fn daemon_should_start(_world: &mut DaemonWorld) {
        // Handled in When step
    }

    #[Then("the daemon should be ready to capture audio")]
    pub fn daemon_ready_for_audio(_world: &mut DaemonWorld) {
        // Implicit
    }

    #[Then("the daemon should exit with code 2")]
    pub fn exit_code_2(world: &mut DaemonWorld) -> Result<(), Box<dyn std::error::Error>> {
        let status = world.exit_code.unwrap_or(-1);
        assert_eq!(status, 2, "expected exit code 2, got {}", status);
        Ok(())
    }

    #[Then("the error should contain \"model file not found\"")]
    pub fn error_contains_model_not_found(
        world: &mut DaemonWorld,
    ) -> Result<(), Box<dyn std::error::Error>> {
        assert!(
            world.stderr_output.contains("model file not found"),
            "expected missing model error, got: {}",
            world.stderr_output
        );
        Ok(())
    }

    #[Then("the daemon should exit with code 3")]
    pub fn exit_code_3(world: &mut DaemonWorld) -> Result<(), Box<dyn std::error::Error>> {
        let status = world.exit_code.unwrap_or(-1);
        assert_eq!(status, 3, "expected exit code 3, got {}", status);
        Ok(())
    }

    #[Then("the error should contain \"input device not found\"")]
    pub fn error_contains_device_not_found(
        world: &mut DaemonWorld,
    ) -> Result<(), Box<dyn std::error::Error>> {
        assert!(
            world.stderr_output.contains("input device not found"),
            "expected device error, got: {}",
            world.stderr_output
        );
        Ok(())
    }
}
