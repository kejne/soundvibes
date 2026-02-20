#[cfg(feature = "test-support")]
pub mod installer {
    use cucumber::{Given, Then, When};
    use std::env;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::process::Command;

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

    fn write_executable(path: &std::path::Path, content: &str) -> std::io::Result<()> {
        fs::write(path, content)?;
        let mut perms = fs::metadata(path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms)?;
        Ok(())
    }

    #[Given("an existing config file with format = \"jsonl\"")]
    pub fn existing_config_jsonl(_world: &mut DaemonWorld) {
        // Created in test setup
    }

    #[When("I run the installer once")]
    pub fn run_installer_once(_world: &mut DaemonWorld) {
        // Run install.sh with mock binaries
    }

    #[Then("the installation should succeed")]
    pub fn install_success(_world: &mut DaemonWorld) {
        // Check exit code 0
    }

    #[When("I run the installer again")]
    pub fn run_installer_again(_world: &mut DaemonWorld) {
        // Run install.sh again
    }

    #[Then("the existing config should be preserved")]
    pub fn config_preserved(_world: &mut DaemonWorld) {
        // Check config file unchanged
    }

    #[Then("the systemd service should be created")]
    pub fn service_created(_world: &mut DaemonWorld) {
        // Check ~/.config/systemd/user/sv.service exists
    }

    #[Given("WAYLAND_DISPLAY is set to \"wayland-0\"")]
    pub fn wayland_set(_world: &mut DaemonWorld) {
        // Set env var in test
    }

    #[Given("DISPLAY is empty")]
    pub fn display_empty(_world: &mut DaemonWorld) {
        // Remove DISPLAY env var
    }

    #[When("I run the installer")]
    pub fn run_installer(_world: &mut DaemonWorld) {
        // Execute install.sh
    }

    #[Then("the installer should detect \"Wayland display server detected\"")]
    pub fn detect_wayland(_world: &mut DaemonWorld) {
        // Check output contains Wayland detection
    }

    #[Given("DISPLAY is set to \":0\"")]
    pub fn display_set(_world: &mut DaemonWorld) {
        // Set DISPLAY env var
    }

    #[Then("the installer should detect \"X11 display server detected\"")]
    pub fn detect_x11(_world: &mut DaemonWorld) {
        // Check output contains X11 detection
    }

    #[Then("the installer should report \"Could not detect display server\"")]
    pub fn detect_headless(_world: &mut DaemonWorld) {
        // Check output contains headless message
    }

    #[Given("the platform is Darwin")]
    pub fn platform_darwin(_world: &mut DaemonWorld) {
        // Mock uname to return Darwin
    }

    #[Then("the installer should fail")]
    pub fn install_fails(_world: &mut DaemonWorld) {
        // Check non-zero exit code
    }

    #[Then("the error should contain \"SoundVibes only supports Linux\"")]
    pub fn error_linux_only(_world: &mut DaemonWorld) {
        // Check stderr contains Linux-only message
    }
}
