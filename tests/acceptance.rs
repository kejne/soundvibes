// Automated acceptance tests for docs/acceptance-tests.md.
// Keep AT-xx mappings in sync with the documentation.
use std::env;
use std::error::Error;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{BufRead, BufReader};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use std::os::unix::fs::PermissionsExt;

#[cfg(feature = "test-support")]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(feature = "test-support")]
use serde_json::Value;

#[cfg(feature = "test-support")]
use std::os::unix::net::UnixStream;

#[cfg(feature = "test-support")]
use sv::daemon::test_support::{
    control_channel, control_message, TestAudioBackend, TestOutput, TestTranscriberFactory,
};
#[cfg(feature = "test-support")]
use sv::daemon::{DaemonConfig, DaemonDeps};
#[cfg(feature = "test-support")]
use sv::model::{ModelSize, ModelVariants};
#[cfg(feature = "test-support")]
use sv::types::{AudioHost, OutputFormat, OutputMode, VadMode};

#[test]
fn at01_daemon_starts_with_valid_model() -> Result<(), Box<dyn Error>> {
    if env::var("SV_HARDWARE_TESTS").ok().as_deref() != Some("1") {
        eprintln!("Skipping AT-01; set SV_HARDWARE_TESTS=1 to run.");
        return Ok(());
    }

    let model_path = model_path()?;
    if !model_path.exists() {
        eprintln!(
            "Skipping AT-01; model file not found at {}",
            model_path.display()
        );
        return Ok(());
    }

    let config_home = temp_dir("soundvibes-acceptance-config");
    let runtime_dir = temp_dir("soundvibes-acceptance-runtime");
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
    let (ready_tx, ready_rx) = mpsc::channel();
    let reader_thread = thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            if line.contains("Daemon listening on") {
                let _ = ready_tx.send(line);
                break;
            }
        }
    });

    wait_for_daemon_ready(&mut child, ready_rx)?;

    stop_daemon(&mut child)?;
    let _ = reader_thread.join();
    Ok(())
}

#[test]
fn at01a_missing_model_is_auto_downloaded() -> Result<(), Box<dyn Error>> {
    let data_home = temp_dir("soundvibes-acceptance-data");
    let _data_guard = EnvGuard::set("XDG_DATA_HOME", &data_home);
    let payload = b"soundvibes-test-model".to_vec();
    let (base_url, server_handle) = start_test_server(payload.clone())?;
    let _url_guard = EnvGuard::set("SV_MODEL_BASE_URL", &base_url);

    let spec =
        sv::model::ModelSpec::new(sv::model::ModelSize::Auto, sv::model::ModelLanguage::Auto);
    let prepared = sv::model::prepare_model(None, &spec, true)?;

    assert!(prepared.downloaded, "expected model download");
    assert!(prepared.path.exists(), "expected model file to exist");
    let stored = fs::read(&prepared.path)?;
    assert_eq!(stored, payload, "downloaded model bytes mismatch");
    let _ = server_handle.join();
    Ok(())
}

#[test]
fn at01b_language_selects_model_variant() -> Result<(), Box<dyn Error>> {
    let english = sv::model::model_language_for_transcription("en");
    let auto = sv::model::model_language_for_transcription("auto");
    let other = sv::model::model_language_for_transcription("es");

    assert_eq!(english, sv::model::ModelLanguage::En);
    assert_eq!(auto, sv::model::ModelLanguage::Auto);
    assert_eq!(other, sv::model::ModelLanguage::Auto);

    let english_spec = sv::model::ModelSpec::new(sv::model::ModelSize::Small, english);
    let auto_spec = sv::model::ModelSpec::new(sv::model::ModelSize::Small, auto);

    assert!(english_spec.filename().contains(".en."));
    assert!(!auto_spec.filename().contains(".en."));
    Ok(())
}

#[test]
fn at02_missing_model_returns_exit_code_2() -> Result<(), Box<dyn Error>> {
    let config_home = temp_dir("soundvibes-acceptance-config");
    let runtime_dir = temp_dir("soundvibes-acceptance-runtime");
    let data_home = temp_dir("soundvibes-acceptance-data");
    let _data_guard = EnvGuard::set("XDG_DATA_HOME", &data_home);
    write_config(&config_home, "download_model = false\n")?;

    let binary = env!("CARGO_BIN_EXE_sv");
    let output = Command::new(binary)
        .args(["daemon", "start"])
        .env("XDG_CONFIG_HOME", &config_home)
        .env("XDG_RUNTIME_DIR", &runtime_dir)
        .output()?;

    let status = output.status.code().unwrap_or(-1);
    assert_eq!(status, 2, "expected exit code 2, got {status}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("model file not found"),
        "expected missing model error, got: {stderr}"
    );
    Ok(())
}

#[test]
fn at03_invalid_input_device_returns_exit_code_3() -> Result<(), Box<dyn Error>> {
    if env::var("SV_HARDWARE_TESTS").ok().as_deref() != Some("1") {
        eprintln!("Skipping AT-03; set SV_HARDWARE_TESTS=1 to run.");
        return Ok(());
    }

    let model_path = model_path()?;
    if !model_path.exists() {
        eprintln!(
            "Skipping AT-03; model file not found at {}",
            model_path.display()
        );
        return Ok(());
    }

    let config_home = temp_dir("soundvibes-acceptance-config");
    let runtime_dir = temp_dir("soundvibes-acceptance-runtime");
    write_config(
        &config_home,
        "language = \"en\"\ndownload_model = false\ndevice = \"nonexistent\"\n",
    )?;

    let binary = env!("CARGO_BIN_EXE_sv");
    let output = Command::new(binary)
        .args(["daemon", "start"])
        .env("XDG_CONFIG_HOME", &config_home)
        .env("XDG_RUNTIME_DIR", &runtime_dir)
        .output()?;

    let status = output.status.code().unwrap_or(-1);
    assert_eq!(status, 3, "expected exit code 3, got {status}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("input device not found"),
        "expected device error, got: {stderr}"
    );
    Ok(())
}

#[cfg(feature = "test-support")]
#[test]
fn at04_daemon_toggle_captures_and_transcribes() -> Result<(), Box<dyn Error>> {
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
        model_size: ModelSize::Small,
        model_variants: ModelVariants::Both,
        download_model: false,
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
        let _ = control_sender.send(control_message(sv::daemon::ControlEvent::Toggle {
            language: None,
        }));
        let _ = control_sender.send(control_message(sv::daemon::ControlEvent::Toggle {
            language: None,
        }));
        thread::sleep(Duration::from_millis(50));
        shutdown_trigger.store(true, Ordering::Relaxed);
    });

    sv::daemon::run_daemon_loop(
        &config,
        &deps,
        &mut output,
        receiver,
        shutdown.as_ref(),
        None,
    )?;
    control_thread.join().expect("control thread failed");

    assert!(output
        .stdout_lines()
        .iter()
        .any(|line| line.contains("Transcript 1: hello")));
    Ok(())
}

#[cfg(feature = "test-support")]
#[test]
fn at05_jsonl_output_formatting() -> Result<(), Box<dyn Error>> {
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
        model_size: ModelSize::Small,
        model_variants: ModelVariants::Both,
        download_model: false,
        language: "en".to_string(),
        device: None,
        audio_host: AudioHost::Default,
        sample_rate: 16_000,
        format: OutputFormat::Jsonl,
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
        let _ = control_sender.send(control_message(sv::daemon::ControlEvent::Toggle {
            language: None,
        }));
        let _ = control_sender.send(control_message(sv::daemon::ControlEvent::Toggle {
            language: None,
        }));
        thread::sleep(Duration::from_millis(50));
        shutdown_trigger.store(true, Ordering::Relaxed);
    });

    sv::daemon::run_daemon_loop(
        &config,
        &deps,
        &mut output,
        receiver,
        shutdown.as_ref(),
        None,
    )?;
    control_thread.join().expect("control thread failed");

    let json_line = output
        .stdout_lines()
        .iter()
        .find(|line| line.starts_with('{'))
        .ok_or("missing JSONL output")?;
    let parsed: Value = serde_json::from_str(json_line)?;
    assert_eq!(parsed["type"], "final");
    assert_eq!(parsed["text"], "hello");
    assert!(parsed["timestamp"].as_str().is_some());
    assert!(parsed["utterance"].as_u64().is_some());
    assert!(parsed["duration_ms"].as_u64().is_some());
    Ok(())
}

#[test]
fn at06_offline_operation() -> Result<(), Box<dyn Error>> {
    if env::var("SV_HARDWARE_TESTS").ok().as_deref() != Some("1")
        || env::var("SV_OFFLINE_TESTS").ok().as_deref() != Some("1")
    {
        eprintln!("Skipping AT-06; set SV_HARDWARE_TESTS=1 and SV_OFFLINE_TESTS=1 to run.");
        return Ok(());
    }

    let model_path = model_path()?;
    if !model_path.exists() {
        eprintln!(
            "Skipping AT-06; model file not found at {}",
            model_path.display()
        );
        return Ok(());
    }

    let config_home = temp_dir("soundvibes-acceptance-config");
    let runtime_dir = temp_dir("soundvibes-acceptance-runtime");
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
    let (ready_tx, ready_rx) = mpsc::channel();
    let reader_thread = thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            if line.contains("Daemon listening on") {
                let _ = ready_tx.send(line);
                break;
            }
        }
    });

    wait_for_daemon_ready(&mut child, ready_rx)?;
    stop_daemon(&mut child)?;
    let _ = reader_thread.join();
    Ok(())
}

#[test]
fn at07_gpu_auto_select() -> Result<(), Box<dyn Error>> {
    if env::var("SV_HARDWARE_TESTS").ok().as_deref() != Some("1")
        || env::var("SV_GPU_TESTS").ok().as_deref() != Some("1")
    {
        eprintln!("Skipping AT-07 GPU check; set SV_HARDWARE_TESTS=1 and SV_GPU_TESTS=1 to run.");
        return Ok(());
    }

    let model_path = model_path()?;
    if !model_path.exists() {
        eprintln!(
            "Skipping AT-07 GPU check; model file not found at {}",
            model_path.display()
        );
        return Ok(());
    }

    let config_home = temp_dir("soundvibes-acceptance-config");
    let runtime_dir = temp_dir("soundvibes-acceptance-runtime");
    write_config(&config_home, "language = \"en\"\ndownload_model = false\n")?;

    let stderr_lines = run_daemon_for_logs(&config_home, &runtime_dir)?;
    let stderr_joined = stderr_lines.join("\n");
    assert!(
        stderr_joined.contains("whisper: GPU backend selected"),
        "expected GPU backend selection, got: {stderr_joined}"
    );
    Ok(())
}

#[test]
fn at07_cpu_fallback() -> Result<(), Box<dyn Error>> {
    if env::var("SV_HARDWARE_TESTS").ok().as_deref() != Some("1")
        || env::var("SV_CPU_TESTS").ok().as_deref() != Some("1")
    {
        eprintln!("Skipping AT-07 CPU check; set SV_HARDWARE_TESTS=1 and SV_CPU_TESTS=1 to run.");
        return Ok(());
    }

    let model_path = model_path()?;
    if !model_path.exists() {
        eprintln!(
            "Skipping AT-07 CPU check; model file not found at {}",
            model_path.display()
        );
        return Ok(());
    }

    let config_home = temp_dir("soundvibes-acceptance-config");
    let runtime_dir = temp_dir("soundvibes-acceptance-runtime");
    write_config(&config_home, "language = \"en\"\ndownload_model = false\n")?;

    let stderr_lines = run_daemon_for_logs(&config_home, &runtime_dir)?;
    let stderr_joined = stderr_lines.join("\n");
    assert!(
        stderr_joined.contains("using CPU"),
        "expected CPU fallback message, got: {stderr_joined}"
    );
    Ok(())
}

#[test]
fn at10_marketing_site_builds_and_smoke_test() -> Result<(), Box<dyn Error>> {
    if env::var("SV_WEB_TESTS").ok().as_deref() != Some("1") {
        eprintln!("Skipping AT-10; set SV_WEB_TESTS=1 to run.");
        return Ok(());
    }

    if !command_available("npm") {
        eprintln!("Skipping AT-10; npm not available in PATH.");
        return Ok(());
    }

    let web_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("web");
    if !web_root.exists() {
        eprintln!("Skipping AT-10; web/ directory missing.");
        return Ok(());
    }

    let install_status = Command::new("npm")
        .arg("install")
        .arg("--no-audit")
        .arg("--no-fund")
        .current_dir(&web_root)
        .status()?;
    if !install_status.success() {
        return Err("AT-10 failed: npm install did not exit cleanly".into());
    }

    let build_status = Command::new("npm")
        .args(["run", "build"])
        .current_dir(&web_root)
        .status()?;
    if !build_status.success() {
        return Err("AT-10 failed: npm run build did not exit cleanly".into());
    }

    let smoke_status = Command::new("npm")
        .args(["run", "test:ui"])
        .current_dir(&web_root)
        .status()?;
    if !smoke_status.success() {
        return Err("AT-10 failed: npm run test:ui did not exit cleanly".into());
    }

    Ok(())
}

#[test]
fn at11_installer_is_idempotent_and_preserves_config() -> Result<(), Box<dyn Error>> {
    let sandbox = temp_dir("soundvibes-install-test");
    let home = sandbox.join("home");
    let xdg_config_home = sandbox.join("xdg-config");
    let bin_dir = home.join(".local").join("bin");
    let mock_bin = sandbox.join("mock-bin");
    fs::create_dir_all(&bin_dir)?;
    fs::create_dir_all(&mock_bin)?;

    let config_dir = xdg_config_home.join("soundvibes");
    fs::create_dir_all(&config_dir)?;
    let config_path = config_dir.join("config.toml");
    let original_config = "# keep-me\nformat = \"jsonl\"\n";
    fs::write(&config_path, original_config)?;

    write_install_mocks(&mock_bin)?;

    let install_script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("install.sh");
    let path = format!(
        "{}:{}:{}",
        mock_bin.display(),
        bin_dir.display(),
        env::var("PATH")?
    );

    for run in 1..=2 {
        let output = run_install_script(
            &install_script,
            &home,
            &xdg_config_home,
            &bin_dir,
            &path,
            &["--yes", "--no-deps"],
            &[("WAYLAND_DISPLAY", ""), ("DISPLAY", "")],
        )?;
        assert_install_success(&output, &format!("idempotency run {run}"));
    }

    let updated_config = fs::read_to_string(&config_path)?;
    assert_eq!(
        updated_config, original_config,
        "installer should keep existing configuration"
    );

    let service_path = home.join(".config/systemd/user/sv.service");
    let service_contents = fs::read_to_string(&service_path)?;
    assert!(
        service_contents.contains("After=graphical-session.target"),
        "service should start after graphical session"
    );
    assert!(
        service_contents.contains("WantedBy=graphical-session.target"),
        "service should be tied to graphical session"
    );

    Ok(())
}

#[test]
fn at11a_installer_handles_display_environment_scenarios() -> Result<(), Box<dyn Error>> {
    let scenarios = [
        (
            "wayland-kde",
            vec![("WAYLAND_DISPLAY", "wayland-0"), ("DISPLAY", "")],
            "Wayland display server detected",
        ),
        (
            "x11-i3",
            vec![("WAYLAND_DISPLAY", ""), ("DISPLAY", ":0")],
            "X11 display server detected",
        ),
        (
            "headless",
            vec![("WAYLAND_DISPLAY", ""), ("DISPLAY", "")],
            "Could not detect display server",
        ),
    ];

    for (name, env_vars, expected_output) in scenarios {
        let sandbox = temp_dir(&format!("soundvibes-install-{name}"));
        let home = sandbox.join("home");
        let xdg_config_home = sandbox.join("xdg-config");
        let bin_dir = home.join(".local").join("bin");
        let mock_bin = sandbox.join("mock-bin");
        fs::create_dir_all(&bin_dir)?;
        fs::create_dir_all(&mock_bin)?;
        write_install_mocks(&mock_bin)?;

        let install_script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("install.sh");
        let path = format!(
            "{}:{}:{}",
            mock_bin.display(),
            bin_dir.display(),
            env::var("PATH")?
        );

        let output = run_install_script(
            &install_script,
            &home,
            &xdg_config_home,
            &bin_dir,
            &path,
            &["--yes", "--no-deps", "--no-service"],
            &env_vars,
        )?;
        assert_install_success(&output, name);

        let combined_output = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            combined_output.contains(expected_output),
            "scenario {name} did not report expected display detection: {expected_output}"
        );
    }

    Ok(())
}

#[test]
fn at11b_installer_rejects_unsupported_platform() -> Result<(), Box<dyn Error>> {
    let sandbox = temp_dir("soundvibes-install-unsupported-platform");
    let home = sandbox.join("home");
    let xdg_config_home = sandbox.join("xdg-config");
    let bin_dir = home.join(".local").join("bin");
    let mock_bin = sandbox.join("mock-bin");
    fs::create_dir_all(&bin_dir)?;
    fs::create_dir_all(&mock_bin)?;
    write_install_mocks(&mock_bin)?;
    write_executable(&mock_bin.join("uname"), "#!/bin/sh\nprintf 'Darwin\n'\n")?;

    let install_script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("install.sh");
    let path = format!(
        "{}:{}:{}",
        mock_bin.display(),
        bin_dir.display(),
        env::var("PATH")?
    );

    let output = run_install_script(
        &install_script,
        &home,
        &xdg_config_home,
        &bin_dir,
        &path,
        &["--yes", "--no-deps", "--no-service"],
        &[("WAYLAND_DISPLAY", ""), ("DISPLAY", "")],
    )?;

    assert!(
        !output.status.success(),
        "installer should fail on unsupported platform"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("SoundVibes only supports Linux"),
        "expected Linux platform error, got: {stderr}"
    );

    Ok(())
}

#[test]
fn at12_plain_toggle_uses_configured_default_language() -> Result<(), Box<dyn Error>> {
    let config_home = temp_dir("soundvibes-acceptance-config");
    let runtime_dir = temp_dir("soundvibes-acceptance-runtime");
    write_config(&config_home, "language = \"sv\"\n")?;

    let socket_dir = runtime_dir.join("soundvibes");
    fs::create_dir_all(&socket_dir)?;
    let socket_path = socket_dir.join("sv.sock");
    let listener = UnixListener::bind(&socket_path)?;
    listener.set_nonblocking(true)?;

    let binary = env!("CARGO_BIN_EXE_sv");
    let child = Command::new(binary)
        .env("XDG_CONFIG_HOME", &config_home)
        .env("XDG_RUNTIME_DIR", &runtime_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;

    let start = Instant::now();
    let payload = loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let mut payload = String::new();
                stream.read_to_string(&mut payload)?;
                stream.write_all(
                    b"{\"api_version\":\"1\",\"ok\":true,\"state\":\"recording\",\"language\":\"sv\"}\n",
                )?;
                break payload;
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                if start.elapsed() > Duration::from_secs(3) {
                    return Err("timed out waiting for toggle command".into());
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err(err) => return Err(Box::new(err)),
        }
    };

    let output = child.wait_with_output()?;
    assert!(
        output.status.success(),
        "expected toggle command to exit successfully, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(payload.trim_end(), "toggle lang=sv");
    Ok(())
}

#[cfg(feature = "test-support")]
#[test]
fn at12_control_socket_toggle_with_language_and_status_response() -> Result<(), Box<dyn Error>> {
    let runtime_dir = temp_dir("soundvibes-acceptance-runtime");
    fs::create_dir_all(&runtime_dir)?;
    let _runtime_guard = EnvGuard::set("XDG_RUNTIME_DIR", &runtime_dir);

    let socket_path = sv::daemon::daemon_socket_path()?;
    let (_socket_guard, receiver) = sv::daemon::start_socket_listener(&socket_path)?;

    let deps = DaemonDeps {
        audio: Box::new(TestAudioBackend::new(
            vec!["Mic".to_string()],
            vec![vec![0.2; 160]],
        )),
        transcriber_factory: Box::new(TestTranscriberFactory::new(vec!["hello".to_string()])),
    };
    let config = DaemonConfig {
        model_size: ModelSize::Small,
        model_variants: ModelVariants::Both,
        download_model: false,
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

    let client_thread = thread::spawn(move || -> Result<(), sv::error::AppError> {
        let toggle_response = sv::daemon::send_toggle_command(Some("fr"))?;
        assert!(toggle_response.ok);
        assert_eq!(toggle_response.state.as_deref(), Some("recording"));
        assert_eq!(toggle_response.language.as_deref(), Some("fr"));

        let status_response = sv::daemon::send_status_command()?;
        assert!(status_response.ok);
        assert_eq!(status_response.state.as_deref(), Some("recording"));
        assert_eq!(status_response.language.as_deref(), Some("fr"));

        let _ = sv::daemon::send_toggle_command(None)?;
        let _ = sv::daemon::send_stop_command()?;
        Ok(())
    });

    let shutdown = Arc::new(AtomicBool::new(false));
    let mut output = TestOutput::default();
    sv::daemon::run_daemon_loop(
        &config,
        &deps,
        &mut output,
        receiver,
        shutdown.as_ref(),
        None,
    )?;

    client_thread
        .join()
        .map_err(|_| "client thread panicked")??;
    Ok(())
}

#[cfg(feature = "test-support")]
#[test]
fn at13_events_socket_fans_out_to_multiple_clients() -> Result<(), Box<dyn Error>> {
    let runtime_dir = temp_dir("soundvibes-acceptance-runtime");
    fs::create_dir_all(&runtime_dir)?;
    let _runtime_guard = EnvGuard::set("XDG_RUNTIME_DIR", &runtime_dir);

    let control_socket_path = sv::daemon::daemon_socket_path()?;
    let (_control_guard, receiver) = sv::daemon::start_socket_listener(&control_socket_path)?;
    let events_socket_path = sv::daemon::daemon_events_socket_path()?;
    let (_events_guard, event_sender) =
        sv::daemon::start_events_socket_listener(&events_socket_path)?;

    let mut first_subscriber = UnixStream::connect(&events_socket_path)?;
    let mut second_subscriber = UnixStream::connect(&events_socket_path)?;
    first_subscriber.set_read_timeout(Some(Duration::from_secs(2)))?;
    second_subscriber.set_read_timeout(Some(Duration::from_secs(2)))?;
    thread::sleep(Duration::from_millis(40));

    let deps = DaemonDeps {
        audio: Box::new(TestAudioBackend::new(
            vec!["Mic".to_string()],
            vec![vec![0.2; 160]],
        )),
        transcriber_factory: Box::new(TestTranscriberFactory::new(vec!["hello".to_string()])),
    };
    let config = DaemonConfig {
        model_size: ModelSize::Small,
        model_variants: ModelVariants::Both,
        download_model: false,
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

    let client_thread = thread::spawn(move || -> Result<(), sv::error::AppError> {
        let _ = sv::daemon::send_toggle_command(Some("fr"))?;
        let _ = sv::daemon::send_toggle_command(None)?;
        let _ = sv::daemon::send_stop_command()?;
        Ok(())
    });

    let shutdown = Arc::new(AtomicBool::new(false));
    let mut output = TestOutput::default();
    sv::daemon::run_daemon_loop(
        &config,
        &deps,
        &mut output,
        receiver,
        shutdown.as_ref(),
        Some(&event_sender),
    )?;

    client_thread
        .join()
        .map_err(|_| "client thread panicked")??;

    let first_events = read_daemon_events(&mut first_subscriber, 6)?;
    let second_events = read_daemon_events(&mut second_subscriber, 6)?;
    assert_eq!(
        first_events, second_events,
        "subscribers should see identical events"
    );

    assert!(matches!(
        first_events[0].event,
        sv::ipc::DaemonEventType::DaemonReady
    ));
    assert!(matches!(
        first_events[1].event,
        sv::ipc::DaemonEventType::ModelLoaded { .. }
    ));
    assert!(matches!(
        first_events[2].event,
        sv::ipc::DaemonEventType::ModelLoaded { .. }
    ));
    assert!(matches!(
        first_events[3].event,
        sv::ipc::DaemonEventType::RecordingStarted { .. }
    ));
    assert!(matches!(
        first_events[4].event,
        sv::ipc::DaemonEventType::TranscriptFinal { .. }
    ));
    assert!(matches!(
        first_events[5].event,
        sv::ipc::DaemonEventType::RecordingStopped { .. }
    ));

    Ok(())
}

#[cfg(feature = "test-support")]
#[test]
fn at14_set_language_switches_active_language_and_transcript_language() -> Result<(), Box<dyn Error>>
{
    let runtime_dir = temp_dir("soundvibes-acceptance-runtime");
    fs::create_dir_all(&runtime_dir)?;
    let _runtime_guard = EnvGuard::set("XDG_RUNTIME_DIR", &runtime_dir);

    let control_socket_path = sv::daemon::daemon_socket_path()?;
    let (_control_guard, receiver) = sv::daemon::start_socket_listener(&control_socket_path)?;
    let events_socket_path = sv::daemon::daemon_events_socket_path()?;
    let (_events_guard, event_sender) =
        sv::daemon::start_events_socket_listener(&events_socket_path)?;

    let mut subscriber = UnixStream::connect(&events_socket_path)?;
    subscriber.set_read_timeout(Some(Duration::from_secs(2)))?;
    thread::sleep(Duration::from_millis(40));

    let deps = DaemonDeps {
        audio: Box::new(TestAudioBackend::new(
            vec!["Mic".to_string()],
            vec![vec![0.2; 160]],
        )),
        transcriber_factory: Box::new(TestTranscriberFactory::new(vec!["hej".to_string()])),
    };
    let config = DaemonConfig {
        model_size: ModelSize::Small,
        model_variants: ModelVariants::Both,
        download_model: false,
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

    let client_thread = thread::spawn(move || -> Result<(), sv::error::AppError> {
        let set_language = sv::daemon::send_set_language_command("sv")?;
        assert!(set_language.ok);
        assert_eq!(set_language.language.as_deref(), Some("sv"));
        assert_eq!(set_language.state.as_deref(), Some("idle"));

        let status = sv::daemon::send_status_command()?;
        assert!(status.ok);
        assert_eq!(status.language.as_deref(), Some("sv"));
        assert_eq!(status.state.as_deref(), Some("idle"));

        let _ = sv::daemon::send_toggle_command(None)?;
        let _ = sv::daemon::send_toggle_command(None)?;
        let _ = sv::daemon::send_stop_command()?;
        Ok(())
    });

    let shutdown = Arc::new(AtomicBool::new(false));
    let mut output = TestOutput::default();
    sv::daemon::run_daemon_loop(
        &config,
        &deps,
        &mut output,
        receiver,
        shutdown.as_ref(),
        Some(&event_sender),
    )?;

    client_thread
        .join()
        .map_err(|_| "client thread panicked")??;

    let events = read_daemon_events(&mut subscriber, 6)?;
    assert!(events.iter().any(|event| {
        matches!(
            event.event,
            sv::ipc::DaemonEventType::ModelLoaded { ref language, .. } if language == "sv"
        )
    }));
    assert!(events.iter().any(|event| {
        matches!(
            event.event,
            sv::ipc::DaemonEventType::TranscriptFinal { ref language, .. } if language == "sv"
        )
    }));

    Ok(())
}

#[cfg(feature = "test-support")]
fn read_daemon_events(
    stream: &mut UnixStream,
    count: usize,
) -> Result<Vec<sv::ipc::DaemonEvent>, Box<dyn Error>> {
    let mut events = Vec::with_capacity(count);
    for _ in 0..count {
        let line = read_event_line(stream)?;
        let event = sv::ipc::from_json_line(&line)?;
        events.push(event);
    }
    Ok(events)
}

#[cfg(feature = "test-support")]
fn read_event_line(stream: &mut UnixStream) -> Result<String, Box<dyn Error>> {
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
                return Err("timed out while reading daemon event".into());
            }
            Err(err) => return Err(Box::new(err)),
        }
    }

    if line.is_empty() {
        return Err("daemon events stream closed before receiving expected event".into());
    }

    String::from_utf8(line).map_err(|err| err.into())
}

fn command_available(command: &str) -> bool {
    Command::new(command)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn write_executable(path: &std::path::Path, content: &str) -> Result<(), Box<dyn Error>> {
    fs::write(path, content)?;
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms)?;
    Ok(())
}

fn write_install_mocks(mock_bin: &std::path::Path) -> Result<(), Box<dyn Error>> {
    write_executable(
        &mock_bin.join("curl"),
        r#"#!/bin/sh
last=""
for arg in "$@"; do
  last="$arg"
done

if [ "$last" = "https://api.github.com/repos/kejne/soundvibes/releases/latest" ]; then
  printf '%s' '{"browser_download_url": "https://example.com/sv-linux-x86_64.tar.gz"}'
  exit 0
fi

out=""
while [ $# -gt 0 ]; do
  if [ "$1" = "-o" ]; then
    out="$2"
    shift 2
  else
    shift
  fi
done

if [ -n "$out" ]; then
  : > "$out"
fi
"#,
    )?;

    write_executable(
        &mock_bin.join("tar"),
        r#"#!/bin/sh
dest="."
while [ $# -gt 0 ]; do
  if [ "$1" = "-C" ]; then
    dest="$2"
    shift 2
  else
    shift
  fi
done

cat > "$dest/sv-linux-x86_64" <<'EOF'
#!/bin/sh
exit 0
EOF
chmod +x "$dest/sv-linux-x86_64"
"#,
    )?;

    write_executable(&mock_bin.join("systemctl"), "#!/bin/sh\nexit 0\n")?;
    write_executable(&mock_bin.join("wtype"), "#!/bin/sh\nexit 0\n")?;
    write_executable(&mock_bin.join("xdotool"), "#!/bin/sh\nexit 0\n")?;
    Ok(())
}

fn run_install_script(
    install_script: &std::path::Path,
    home: &std::path::Path,
    xdg_config_home: &std::path::Path,
    bin_dir: &std::path::Path,
    path: &str,
    args: &[&str],
    env_vars: &[(&str, &str)],
) -> Result<std::process::Output, Box<dyn Error>> {
    let mut command = Command::new("sh");
    command.arg(install_script);
    command.args(args);
    command.arg(format!("--bin-dir={}", bin_dir.display()));
    command
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", xdg_config_home)
        .env("PATH", path)
        .env("CI", "1");

    for (key, value) in env_vars {
        command.env(key, value);
    }

    let output = command.output()?;
    Ok(output)
}

fn assert_install_success(output: &std::process::Output, context: &str) {
    assert!(
        output.status.success(),
        "{context}: install script failed with {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn model_path() -> Result<PathBuf, Box<dyn Error>> {
    let data_home = env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|_| env::var("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .unwrap_or_else(|_| PathBuf::from("/tmp"));
    Ok(data_home
        .join("soundvibes")
        .join("models")
        .join("ggml-small.en.bin"))
}

fn temp_dir(prefix: &str) -> PathBuf {
    let mut dir = env::temp_dir();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    dir.push(format!("{prefix}-{}-{stamp}", std::process::id()));
    dir
}

struct EnvGuard {
    key: &'static str,
    previous: Option<OsString>,
}

impl EnvGuard {
    fn set(key: &'static str, value: impl AsRef<OsStr>) -> Self {
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

fn start_test_server(payload: Vec<u8>) -> Result<(String, thread::JoinHandle<()>), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    let handle = thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buffer = [0u8; 1024];
            let _ = stream.read(&mut buffer);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                payload.len()
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.write_all(&payload);
        }
    });
    Ok((format!("http://{addr}"), handle))
}

fn write_config(config_home: &std::path::Path, contents: &str) -> Result<(), Box<dyn Error>> {
    let config_path = config_home.join("soundvibes").join("config.toml");
    fs::create_dir_all(config_path.parent().expect("config parent"))?;
    fs::write(&config_path, contents)?;
    Ok(())
}

fn wait_for_daemon_ready(
    child: &mut std::process::Child,
    ready_rx: mpsc::Receiver<String>,
) -> Result<(), Box<dyn Error>> {
    let start = Instant::now();
    loop {
        if let Ok(line) = ready_rx.recv_timeout(Duration::from_millis(100)) {
            if line.contains("Daemon listening on") {
                return Ok(());
            }
        }
        if let Some(status) = child.try_wait()? {
            return Err(format!("daemon exited early with {status}").into());
        }
        if start.elapsed() > Duration::from_secs(3) {
            return Err("daemon did not report ready state".into());
        }
    }
}

fn stop_daemon(child: &mut std::process::Child) -> Result<(), Box<dyn Error>> {
    let pid = child.id();
    let status = Command::new("kill")
        .arg("-TERM")
        .arg(pid.to_string())
        .status()?;
    if !status.success() {
        return Err("failed to send SIGTERM to daemon".into());
    }

    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            if !status.success() {
                return Err(format!("daemon exited with {status}").into());
            }
            return Ok(());
        }
        if start.elapsed() > Duration::from_secs(3) {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }

    let _ = Command::new("kill")
        .arg("-KILL")
        .arg(pid.to_string())
        .status();
    let _ = child.wait();
    Err("daemon did not terminate after SIGTERM".into())
}

fn run_daemon_for_logs(
    config_home: &PathBuf,
    runtime_dir: &PathBuf,
) -> Result<Vec<String>, Box<dyn Error>> {
    let binary = env!("CARGO_BIN_EXE_sv");
    let mut child = Command::new(binary)
        .args(["daemon", "start"])
        .env("XDG_CONFIG_HOME", config_home)
        .env("XDG_RUNTIME_DIR", runtime_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child.stdout.take().expect("stdout pipe");
    let stderr = child.stderr.take().expect("stderr pipe");
    let (ready_tx, ready_rx) = mpsc::channel();
    let stderr_lines = Arc::new(Mutex::new(Vec::new()));
    let stderr_capture = Arc::clone(&stderr_lines);

    let stdout_thread = thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            if line.contains("Daemon listening on") {
                let _ = ready_tx.send(line);
                break;
            }
        }
    });

    let stderr_thread = thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            stderr_capture.lock().expect("stderr lock").push(line);
        }
    });

    wait_for_daemon_ready(&mut child, ready_rx)?;
    stop_daemon(&mut child)?;
    let _ = stdout_thread.join();
    let _ = stderr_thread.join();

    let collected = stderr_lines.lock().expect("stderr lock").clone();
    Ok(collected)
}
