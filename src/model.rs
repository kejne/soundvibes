use clap::ValueEnum;
use serde::Deserialize;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::error::AppError;

const DEFAULT_MODEL_BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

#[derive(Debug, Copy, Clone, ValueEnum, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelSize {
    Auto,
    Tiny,
    Base,
    Small,
    Medium,
    Large,
}

impl ModelSize {
    fn resolved(self) -> Self {
        match self {
            ModelSize::Auto => ModelSize::Small,
            other => other,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            ModelSize::Auto => "small",
            ModelSize::Tiny => "tiny",
            ModelSize::Base => "base",
            ModelSize::Small => "small",
            ModelSize::Medium => "medium",
            ModelSize::Large => "large",
        }
    }
}

#[derive(Debug, Copy, Clone, ValueEnum, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelLanguage {
    Auto,
    En,
}

#[derive(Debug, Copy, Clone)]
pub struct ModelSpec {
    pub size: ModelSize,
    pub language: ModelLanguage,
}

impl ModelSpec {
    pub fn new(size: ModelSize, language: ModelLanguage) -> Self {
        Self { size, language }
    }

    pub fn filename(&self) -> String {
        let size = self.size.resolved().as_str();
        match self.language {
            ModelLanguage::Auto => format!("ggml-{size}.bin"),
            ModelLanguage::En => format!("ggml-{size}.en.bin"),
        }
    }
}

#[derive(Debug)]
pub struct PreparedModel {
    pub path: PathBuf,
    pub downloaded: bool,
}

pub fn prepare_model(
    explicit_path: Option<&Path>,
    spec: &ModelSpec,
    allow_download: bool,
) -> Result<PreparedModel, AppError> {
    let path = resolve_model_path(explicit_path, spec);
    let downloaded = ensure_model_available(&path, spec, allow_download)?;
    Ok(PreparedModel { path, downloaded })
}

pub fn resolve_model_path(explicit_path: Option<&Path>, spec: &ModelSpec) -> PathBuf {
    explicit_path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| default_model_dir().join(spec.filename()))
}

pub fn default_model_dir() -> PathBuf {
    let data_home = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    data_home.join("soundvibes").join("models")
}

fn ensure_model_available(
    path: &Path,
    spec: &ModelSpec,
    allow_download: bool,
) -> Result<bool, AppError> {
    if path.exists() {
        validate_model_path(path)?;
        return Ok(false);
    }

    if !allow_download {
        return Err(AppError::config(format!(
            "model file not found at {} (set download_model = true to download)",
            path.display()
        )));
    }

    download_model(path, spec)?;
    validate_model_path(path)?;
    Ok(true)
}

fn validate_model_path(path: &Path) -> Result<(), AppError> {
    if !path.exists() {
        return Err(AppError::config(format!(
            "model file not found at {}",
            path.display()
        )));
    }
    if !path.is_file() {
        return Err(AppError::config(format!(
            "model path is not a file: {}",
            path.display()
        )));
    }
    Ok(())
}

fn download_model(path: &Path, spec: &ModelSpec) -> Result<(), AppError> {
    let filename = spec.filename();
    let base = env::var("SV_MODEL_BASE_URL").unwrap_or_else(|_| DEFAULT_MODEL_BASE_URL.to_string());
    let url = format!("{}/{}", base.trim_end_matches('/'), filename);

    println!("Downloading model {filename} from {url}...");

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            AppError::config(format!(
                "failed to create model directory {}: {err}",
                parent.display()
            ))
        })?;
    }

    let response = ureq::get(&url)
        .call()
        .map_err(|err| AppError::config(format!("failed to download model from {url}: {err}")))?;
    if response.status() != 200 {
        return Err(AppError::config(format!(
            "model download failed with status {} from {url}",
            response.status()
        )));
    }

    let temp_path = path.with_extension("bin.part");
    let mut reader = response.into_reader();
    let mut file = fs::File::create(&temp_path).map_err(|err| {
        AppError::config(format!(
            "failed to create temporary model file {}: {err}",
            temp_path.display()
        ))
    })?;
    io::copy(&mut reader, &mut file)
        .map_err(|err| AppError::config(format!("failed to write model file: {err}")))?;
    file.flush()
        .map_err(|err| AppError::config(format!("failed to flush model file: {err}")))?;
    fs::rename(&temp_path, path).map_err(|err| {
        AppError::config(format!(
            "failed to move model file into place {}: {err}",
            path.display()
        ))
    })?;
    Ok(())
}
