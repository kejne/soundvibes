use std::env;
use std::fmt;
use std::process::Command;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SessionType {
    Wayland,
    X11,
    Unknown,
}

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

pub fn detect_session_type() -> SessionType {
    if let Ok(value) = env::var("XDG_SESSION_TYPE") {
        return match value.to_lowercase().as_str() {
            "wayland" => SessionType::Wayland,
            "x11" => SessionType::X11,
            _ => SessionType::Unknown,
        };
    }

    if env::var_os("WAYLAND_DISPLAY").is_some() {
        return SessionType::Wayland;
    }

    if env::var_os("DISPLAY").is_some() {
        return SessionType::X11;
    }

    SessionType::Unknown
}

pub fn inject_text(text: &str) -> Result<(), OutputError> {
    match detect_session_type() {
        SessionType::Wayland => inject_wayland(text),
        SessionType::X11 => inject_x11(text),
        SessionType::Unknown => Err(OutputError::new(
            "unable to detect session type; set XDG_SESSION_TYPE to wayland or x11",
        )),
    }
}

fn inject_wayland(text: &str) -> Result<(), OutputError> {
    run_command(
        "wtype",
        &["--", text],
        "install wtype to enable Wayland text injection",
    )
}

fn inject_x11(text: &str) -> Result<(), OutputError> {
    run_command(
        "xdotool",
        &["type", "--clearmodifiers", "--delay", "0", "--", text],
        "install xdotool to enable X11 text injection",
    )
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
    fn detect_session_type_prefers_xdg_session_type() {
        let _guard = EnvGuard::set("XDG_SESSION_TYPE", "wayland");
        assert_eq!(detect_session_type(), SessionType::Wayland);
    }

    #[test]
    fn detect_session_type_uses_display_fallbacks() {
        let _guard = EnvGuard::remove("XDG_SESSION_TYPE");
        let _wayland_guard = EnvGuard::set("WAYLAND_DISPLAY", "wayland-0");
        let _display_guard = EnvGuard::remove("DISPLAY");
        assert_eq!(detect_session_type(), SessionType::Wayland);
    }
}
