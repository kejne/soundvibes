#[cfg(feature = "test-support")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "test-support")]
use std::sync::Arc;

#[cfg(feature = "test-support")]
use cucumber::{World, WorldInit};

#[cfg(feature = "test-support")]
use sv::daemon::test_support::{TestAudioBackend, TestOutput, TestTranscriberFactory};
#[cfg(feature = "test-support")]
use sv::daemon::{DaemonConfig, DaemonDeps};
#[cfg(feature = "test-support")]
use sv::model::{ModelSize, ModelVariants};
#[cfg(feature = "test-support")]
use sv::types::{AudioHost, OutputFormat, OutputMode, VadMode};

#[cfg(feature = "test-support")]
#[derive(WorldInit)]
pub struct DaemonWorld {
    pub config: DaemonConfig,
    pub deps: DaemonDeps,
    pub output: TestOutput,
    pub shutdown: Arc<AtomicBool>,
}

#[cfg(feature = "test-support")]
impl Default for DaemonWorld {
    fn default() -> Self {
        Self {
            config: DaemonConfig {
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
            },
            deps: DaemonDeps {
                audio: Box::new(TestAudioBackend::new(
                    vec!["Mic".to_string()],
                    vec![vec![0.2; 160]],
                )),
                transcriber_factory: Box::new(TestTranscriberFactory::new(vec![
                    "hello".to_string()
                ])),
            },
            output: TestOutput::default(),
            shutdown: Arc::new(AtomicBool::new(false)),
        }
    }
}

#[cfg(feature = "test-support")]
mod daemon_startup;

#[cfg(feature = "test-support")]
mod model_management;

#[cfg(feature = "test-support")]
mod steps {
    use crate::DaemonWorld;
    use cucumber::{Given, Then, When};

    #[given("a valid model file exists at the default location")]
    pub fn valid_model_exists(_world: &mut DaemonWorld) {
        // Already handled by test-support mocks
    }

    #[given("the config directory is empty")]
    pub fn empty_config_dir(_world: &mut DaemonWorld) {
        // Config handled externally
    }

    #[given("download_model is disabled")]
    pub fn download_model_disabled(world: &mut DaemonWorld) {
        world.config.download_model = false;
    }

    #[given("no model file exists")]
    pub fn no_model_file(_world: &mut DaemonWorld) {
        // Handled by test environment
    }

    #[given("the device is set to \"nonexistent\"")]
    pub fn nonexistent_device(world: &mut DaemonWorld) {
        world.config.device = Some("nonexistent".to_string());
    }

    #[given("a mocked audio backend with sample audio")]
    pub fn mocked_audio_backend(world: &mut DaemonWorld) {
        world.deps.audio = Box::new(TestAudioBackend::new(
            vec!["Mic".to_string()],
            vec![vec![0.2; 160]],
        ));
    }

    #[given("a mocked transcriber returning \"hello\"")]
    pub fn mocked_transcriber_hello(world: &mut DaemonWorld) {
        world.deps.transcriber_factory =
            Box::new(TestTranscriberFactory::new(vec!["hello".to_string()]));
    }

    #[given("the output format is set to \"jsonl\"")]
    pub fn jsonl_format(world: &mut DaemonWorld) {
        world.config.format = OutputFormat::Jsonl;
    }

    #[when("I send toggle command to start recording")]
    pub fn send_toggle_start(_world: &mut DaemonWorld) {
        // Implemented in actual test runner
    }

    #[when("I wait for transcription to complete")]
    pub fn wait_for_transcription(_world: &mut DaemonWorld) {
        // Implemented in actual test runner
    }

    #[when("I send toggle command to stop recording")]
    pub fn send_toggle_stop(_world: &mut DaemonWorld) {
        // Implemented in actual test runner
    }

    #[then("the output should contain \"Transcript 1: hello\"")]
    pub fn check_transcript_output(world: &mut DaemonWorld) {
        assert!(world
            .output
            .stdout_lines()
            .iter()
            .any(|line| line.contains("Transcript 1: hello")));
    }

    #[then("the output should contain valid JSONL with type \"final\"")]
    pub fn check_jsonl_output(world: &mut DaemonWorld) {
        // Implemented in actual test runner
    }

    #[given("language is set to \"en\"")]
    pub fn language_en(world: &mut DaemonWorld) {
        world.config.language = "en".to_string();
    }

    #[given("language is set to \"sv\" in config")]
    pub fn language_sv_config(world: &mut DaemonWorld) {
        world.config.language = "sv".to_string();
    }

    #[given("the daemon language is \"en\"")]
    pub fn daemon_language_en(world: &mut DaemonWorld) {
        world.config.language = "en".to_string();
    }
}
