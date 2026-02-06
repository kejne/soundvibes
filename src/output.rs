use std::env;
use std::fmt;
use std::process::Command;

#[derive(Debug)]
pub struct OutputError {
    message: String,
}

impl OutputError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for OutputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

pub fn inject_text(text: &str) -> Result<(), OutputError> {
    let mut errors = Vec::new();
    if let Some(err) = try_wayland(text)? {
        errors.push(err);
    } else {
        return Ok(());
    }

    if let Some(err) = try_x11(text)? {
        errors.push(err);
    } else {
        return Ok(());
    }

    let mut message = format!(
        "no supported injection backends available ({})",
        errors.join("; ")
    );
    if missing_graphical_session(&errors) {
        message.push_str(
            "; session environment missing (DISPLAY/WAYLAND_DISPLAY). If running via systemd user service, start it after graphical session (WantedBy=graphical-session.target)",
        );
    }

    Err(OutputError::new(message))
}

fn missing_graphical_session(errors: &[String]) -> bool {
    errors
        .iter()
        .any(|err| err == "wayland session not detected")
        && errors.iter().any(|err| err == "x11 session not detected")
}

fn try_wayland(text: &str) -> Result<Option<String>, OutputError> {
    if !has_wayland_session() {
        return Ok(Some("wayland session not detected".to_string()));
    }

    match run_command(
        "wtype",
        &["--", text],
        "install wtype to enable Wayland text injection",
    ) {
        Ok(()) => Ok(None),
        Err(err) => Ok(Some(format!("wayland: {err}"))),
    }
}

fn try_x11(text: &str) -> Result<Option<String>, OutputError> {
    if !has_x11_session() {
        return Ok(Some("x11 session not detected".to_string()));
    }

    match run_command(
        "xdotool",
        &["type", "--clearmodifiers", "--delay", "0", "--", text],
        "install xdotool to enable X11 text injection",
    ) {
        Ok(()) => Ok(None),
        Err(err) => Ok(Some(format!("x11: {err}"))),
    }
}

fn has_wayland_session() -> bool {
    if let Ok(value) = env::var("XDG_SESSION_TYPE") {
        if value.eq_ignore_ascii_case("wayland") {
            return true;
        }
    }
    env::var_os("WAYLAND_DISPLAY").is_some()
}

fn has_x11_session() -> bool {
    if let Ok(value) = env::var("XDG_SESSION_TYPE") {
        if value.eq_ignore_ascii_case("x11") {
            return true;
        }
    }
    env::var_os("DISPLAY").is_some()
}

fn run_command(program: &str, args: &[&str], help: &str) -> Result<(), OutputError> {
    let status = Command::new(program).args(args).status().map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            OutputError::new(format!("{program} not found; {help}"))
        } else {
            OutputError::new(format!("failed to run {program}: {err}"))
        }
    })?;

    if status.success() {
        Ok(())
    } else {
        Err(OutputError::new(format!(
            "{program} exited with status {status}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    struct EnvGuard {
        key: &'static str,
        previous: Option<OsString>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = env::var_os(key);
            env::set_var(key, value);
            Self { key, previous }
        }

        fn remove(key: &'static str) -> Self {
            let previous = env::var_os(key);
            env::remove_var(key);
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

    #[test]
    fn detects_wayland_session_from_env() {
        let _guard = EnvGuard::set("XDG_SESSION_TYPE", "wayland");
        assert!(has_wayland_session());
    }

    #[test]
    fn detects_x11_session_from_env() {
        let _guard = EnvGuard::set("XDG_SESSION_TYPE", "x11");
        assert!(has_x11_session());
    }

    #[test]
    fn detects_wayland_session_from_display_fallback() {
        let _guard = EnvGuard::remove("XDG_SESSION_TYPE");
        let _wayland_guard = EnvGuard::set("WAYLAND_DISPLAY", "wayland-0");
        let _display_guard = EnvGuard::remove("DISPLAY");
        assert!(has_wayland_session());
    }

    #[test]
    fn detects_missing_graphical_session_errors() {
        let errors = vec![
            "wayland session not detected".to_string(),
            "x11 session not detected".to_string(),
        ];
        assert!(missing_graphical_session(&errors));
    }
}
