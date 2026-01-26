use std::error::Error;
use std::fmt;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum AppErrorKind {
    Config,
    Audio,
    Runtime,
}

#[derive(Debug)]
pub struct AppError {
    kind: AppErrorKind,
    message: String,
}

impl AppError {
    pub fn config(message: impl Into<String>) -> Self {
        Self {
            kind: AppErrorKind::Config,
            message: message.into(),
        }
    }

    pub fn audio(message: impl Into<String>) -> Self {
        Self {
            kind: AppErrorKind::Audio,
            message: message.into(),
        }
    }

    pub fn runtime(message: impl Into<String>) -> Self {
        Self {
            kind: AppErrorKind::Runtime,
            message: message.into(),
        }
    }

    pub fn exit_code(&self) -> i32 {
        match self.kind {
            AppErrorKind::Config => 2,
            AppErrorKind::Audio => 3,
            AppErrorKind::Runtime => 1,
        }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Error for AppError {}
