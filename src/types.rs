use clap::ValueEnum;
use serde::Deserialize;

#[derive(Debug, Copy, Clone, ValueEnum, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    Plain,
    Jsonl,
}

#[derive(Debug, Copy, Clone, ValueEnum, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputMode {
    Stdout,
    Inject,
}

#[derive(Debug, Copy, Clone, ValueEnum, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioHost {
    Default,
    Alsa,
}

impl AudioHost {
    pub fn default_for_platform() -> Self {
        #[cfg(target_os = "linux")]
        {
            AudioHost::Alsa
        }
        #[cfg(not(target_os = "linux"))]
        {
            AudioHost::Default
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, ValueEnum, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VadMode {
    On,
    Off,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum VadSetting {
    Bool(bool),
    Mode(VadMode),
}

impl VadSetting {
    pub fn into_mode(self) -> VadMode {
        match self {
            VadSetting::Bool(true) => VadMode::On,
            VadSetting::Bool(false) => VadMode::Off,
            VadSetting::Mode(mode) => mode,
        }
    }
}
