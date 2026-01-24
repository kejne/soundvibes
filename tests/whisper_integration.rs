use std::error::Error;
use std::path::{Path, PathBuf};

use sv::whisper::WhisperContext;

#[test]
fn transcribes_sample_audio() -> Result<(), Box<dyn Error>> {
    let model_path = std::env::var("SV_MODEL_PATH").unwrap_or_else(|_| {
        let data_home = std::env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|_| std::env::var("HOME").map(|home| PathBuf::from(home).join(".local/share")))
            .unwrap_or_else(|_| PathBuf::from("/tmp"));
        data_home
            .join("soundvibes")
            .join("models")
            .join("ggml-base.en.bin")
            .to_string_lossy()
            .to_string()
    });
    let model_path = Path::new(&model_path);
    if !model_path.exists() {
        eprintln!(
            "Skipping test; model file not found at {}",
            model_path.display()
        );
        return Ok(());
    }

    let sample_path = Path::new("vendor/whisper.cpp/samples/jfk.wav");
    if !sample_path.exists() {
        eprintln!(
            "Skipping test; sample wav missing at {}",
            sample_path.display()
        );
        return Ok(());
    }

    let samples = load_wav_samples(sample_path)?;
    let context = WhisperContext::from_file(model_path)?;
    let transcript = context.transcribe(&samples, Some("en"))?;
    let normalized = transcript.to_lowercase();
    let expected = "ask not what your country can do for you";
    assert!(
        normalized.contains(expected),
        "expected transcript to include '{expected}', got '{transcript}'"
    );
    Ok(())
}

fn load_wav_samples(path: &Path) -> Result<Vec<f32>, Box<dyn Error>> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    let channels = spec.channels as usize;
    if channels == 0 {
        return Ok(Vec::new());
    }

    let mut output = Vec::new();
    match spec.sample_format {
        hound::SampleFormat::Float => {
            let mut frame = Vec::with_capacity(channels);
            for sample in reader.samples::<f32>() {
                frame.push(sample?);
                if frame.len() == channels {
                    let sum: f32 = frame.iter().sum();
                    output.push(sum / channels as f32);
                    frame.clear();
                }
            }
        }
        hound::SampleFormat::Int => {
            let max = (1_i64 << (spec.bits_per_sample - 1)) as f32;
            let mut frame = Vec::with_capacity(channels);
            for sample in reader.samples::<i32>() {
                frame.push(sample? as f32 / max);
                if frame.len() == channels {
                    let sum: f32 = frame.iter().sum();
                    output.push(sum / channels as f32);
                    frame.clear();
                }
            }
        }
    }

    Ok(output)
}
