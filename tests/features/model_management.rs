#[cfg(feature = "test-support")]
pub mod model_management {
    use cucumber::{Given, Then, When};
    use std::env;
    use std::fs;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::thread;

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

    #[Given("no model file exists")]
    pub fn no_model_file(world: &mut DaemonWorld) {
        let data_home = temp_dir("soundvibes-model-test-data");
        let _ = fs::remove_dir_all(&data_home);
        fs::create_dir_all(&data_home).expect("create data dir");
        world.config.download_model = false;
    }

    #[Given("the model base URL is available")]
    pub fn model_base_url_available(world: &mut DaemonWorld) {
        // This would start a mock server - simplified for now
    }

    #[When("I request a model download")]
    pub fn request_model_download(
        world: &mut DaemonWorld,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let data_home = temp_dir("soundvibes-model-download");
        fs::create_dir_all(&data_home).expect("create data dir");

        let payload = b"soundvibes-test-model".to_vec();

        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        let base_url = format!("http://{}", addr);

        let _server_thread = thread::spawn(move || {
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

        let spec =
            sv::model::ModelSpec::new(sv::model::ModelSize::Auto, sv::model::ModelLanguage::Auto);
        let prepared = sv::model::prepare_model(Some(&base_url), &spec, true)?;

        assert!(prepared.downloaded, "expected model download");
        assert!(prepared.path.exists(), "expected model file to exist");

        Ok(())
    }

    #[Then("the model should be downloaded automatically")]
    pub fn model_downloaded(_world: &mut DaemonWorld) {
        // Handled in When step
    }

    #[Then("the downloaded model should be stored in the models directory")]
    pub fn model_stored_in_models_dir(_world: &mut DaemonWorld) {
        // Handled in When step
    }

    #[Given("language is set to \"en\"")]
    pub fn language_en(world: &mut DaemonWorld) {
        world.config.language = "en".to_string();
    }

    #[When("I request an English model")]
    pub fn request_english_model(world: &mut DaemonWorld) {
        let language = sv::model::model_language_for_transcription("en");
        assert_eq!(language, sv::model::ModelLanguage::En);
    }

    #[Then("the model filename should contain \".en.\"")]
    pub fn filename_contains_en(_world: &mut DaemonWorld) {
        let spec =
            sv::model::ModelSpec::new(sv::model::ModelSize::Small, sv::model::ModelLanguage::En);
        assert!(spec.filename().contains(".en."));
    }

    #[Given("language is set to \"auto\"")]
    pub fn language_auto(world: &mut DaemonWorld) {
        world.config.language = "auto".to_string();
    }

    #[When("I request an auto-detected model")]
    pub fn request_auto_model(world: &mut DaemonWorld) {
        let language = sv::model::model_language_for_transcription("auto");
        assert_eq!(language, sv::model::ModelLanguage::Auto);
    }

    #[Then("the model filename should not contain \".en.\"")]
    pub fn filename_not_contains_en(_world: &mut DaemonWorld) {
        let spec =
            sv::model::ModelSpec::new(sv::model::ModelSize::Small, sv::model::ModelLanguage::Auto);
        assert!(!spec.filename().contains(".en."));
    }
}
