#[cfg(feature = "test-support")]
pub mod web {
    use cucumber::{Given, Then, When};
    use std::env;
    use std::path::PathBuf;
    use std::process::Command;
    use std::process::Stdio;

    use crate::DaemonWorld;

    fn command_available(command: &str) -> bool {
        Command::new(command)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    #[Given("npm is available")]
    pub fn npm_available(_world: &mut DaemonWorld) {
        if !command_available("npm") {
            eprintln!("Skipping AT-10; npm not available in PATH.");
        }
    }

    #[Given("the web directory exists")]
    pub fn web_dir_exists(_world: &mut DaemonWorld) {
        let web_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("web");
        if !web_root.exists() {
            eprintln!("Skipping AT-10; web/ directory missing.");
        }
    }

    #[When("I run npm install")]
    pub fn run_npm_install(_world: &mut DaemonWorld) -> Result<(), Box<dyn std::error::Error>> {
        let web_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("web");

        let status = Command::new("npm")
            .arg("install")
            .arg("--no-audit")
            .arg("--no-fund")
            .current_dir(&web_root)
            .status()?;

        if !status.success() {
            return Err("npm install did not exit cleanly".into());
        }

        Ok(())
    }

    #[Then("the installation should succeed")]
    pub fn install_succeeds(_world: &mut DaemonWorld) {
        // Handled in When step
    }

    #[When("I run npm run build")]
    pub fn run_npm_build(_world: &mut DaemonWorld) -> Result<(), Box<dyn std::error::Error>> {
        let web_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("web");

        let status = Command::new("npm")
            .args(["run", "build"])
            .current_dir(&web_root)
            .status()?;

        if !status.success() {
            return Err("npm run build did not exit cleanly".into());
        }

        Ok(())
    }

    #[Then("the build should succeed")]
    pub fn build_succeeds(_world: &mut DaemonWorld) {
        // Handled in When step
    }

    #[When("I run npm run test:ui")]
    pub fn run_npm_test_ui(_world: &mut DaemonWorld) -> Result<(), Box<dyn std::error::Error>> {
        let web_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("web");

        let status = Command::new("npm")
            .args(["run", "test:ui"])
            .current_dir(&web_root)
            .status()?;

        if !status.success() {
            return Err("npm run test:ui did not exit cleanly".into());
        }

        Ok(())
    }

    #[Then("the smoke tests should pass")]
    pub fn smoke_tests_pass(_world: &mut DaemonWorld) {
        // Handled in When step
    }
}
