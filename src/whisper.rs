use std::ffi::{CStr, CString, NulError};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::ptr::NonNull;
use std::thread;

#[allow(
    dead_code,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals
)]
mod bindings {
    include!(concat!(env!("OUT_DIR"), "/whisper_bindings.rs"));
}

use bindings::*;

#[derive(Debug)]
pub enum WhisperError {
    InvalidPath(NulError),
    InitFailed,
    TranscriptionFailed(i32),
}

impl std::fmt::Display for WhisperError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WhisperError::InvalidPath(error) => write!(f, "invalid model path: {error}"),
            WhisperError::InitFailed => write!(f, "failed to initialize whisper context"),
            WhisperError::TranscriptionFailed(code) => {
                write!(f, "whisper transcription failed with code {code}")
            }
        }
    }
}

impl std::error::Error for WhisperError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            WhisperError::InvalidPath(error) => Some(error),
            _ => None,
        }
    }
}

pub struct WhisperContext {
    ctx: NonNull<whisper_context>,
}

impl WhisperContext {
    pub fn from_file(path: &Path) -> Result<Self, WhisperError> {
        let path_c =
            CString::new(path.as_os_str().as_bytes()).map_err(WhisperError::InvalidPath)?;
        let mut params = unsafe { whisper_context_default_params() };
        params.use_gpu = true;
        params.flash_attn = false;
        params.gpu_device = 0;

        let ctx = unsafe { whisper_init_from_file_with_params(path_c.as_ptr(), params) };
        let ctx = NonNull::new(ctx).ok_or(WhisperError::InitFailed)?;
        Ok(Self { ctx })
    }

    pub fn transcribe(
        &self,
        samples: &[f32],
        language: Option<&str>,
    ) -> Result<String, WhisperError> {
        let mut params = unsafe {
            whisper_full_default_params(whisper_sampling_strategy_WHISPER_SAMPLING_GREEDY)
        };
        params.print_progress = false;
        params.print_realtime = false;
        params.print_timestamps = false;
        params.no_timestamps = true;
        params.single_segment = true;
        params.translate = false;
        let available_threads = thread::available_parallelism()
            .map(|count| count.get())
            .unwrap_or(1);
        let n_threads = (available_threads / 2).max(1) as i32;
        params.n_threads = n_threads;

        let detect_language = language.is_none();
        let language_cstring;
        let language_ptr = if let Some(language) = language {
            language_cstring = CString::new(language).map_err(WhisperError::InvalidPath)?;
            language_cstring.as_ptr()
        } else {
            std::ptr::null()
        };
        params.language = language_ptr;
        params.detect_language = detect_language;

        let result = unsafe {
            whisper_full(
                self.ctx.as_ptr(),
                params,
                samples.as_ptr(),
                samples.len() as i32,
            )
        };
        if result != 0 {
            return Err(WhisperError::TranscriptionFailed(result));
        }

        let segments = unsafe { whisper_full_n_segments(self.ctx.as_ptr()) };
        let mut output = String::new();
        for i in 0..segments {
            let text_ptr = unsafe { whisper_full_get_segment_text(self.ctx.as_ptr(), i) };
            if !text_ptr.is_null() {
                let text = unsafe { CStr::from_ptr(text_ptr) }.to_string_lossy();
                output.push_str(text.trim());
                output.push(' ');
            }
        }
        Ok(output.trim().to_string())
    }
}

impl Drop for WhisperContext {
    fn drop(&mut self) {
        unsafe { whisper_free(self.ctx.as_ptr()) };
    }
}
