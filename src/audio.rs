use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::{HeapConsumer, HeapProducer, HeapRb};
use std::error::Error;
use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub struct Capture {
    _stream: cpal::Stream,
    consumer: HeapConsumer<f32>,
    overflow: Arc<Mutex<Vec<f32>>>,
    overflow_count: Arc<AtomicUsize>,
}

pub const DEFAULT_CHUNK_MS: u64 = 250;
pub const DEFAULT_VAD_THRESHOLD: f32 = 0.015;
pub const DEFAULT_SILENCE_TIMEOUT_MS: u64 = 800;

pub struct VadConfig {
    pub enabled: bool,
    pub energy_threshold: f32,
    pub silence_timeout: Duration,
    pub chunk_size: Duration,
    #[allow(dead_code)]
    pub debug: bool,
}

#[derive(Debug, Copy, Clone)]
pub struct SegmentInfo {
    pub index: u64,
    pub duration_ms: u64,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum AudioErrorKind {
    DeviceNotFound,
    DeviceUnavailable,
    DeviceQuery,
    StreamConfig,
    StreamBuild,
    StreamStart,
}

#[derive(Debug)]
pub struct AudioError {
    pub kind: AudioErrorKind,
    pub message: String,
}

impl AudioError {
    fn new(kind: AudioErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Error for AudioError {}

impl VadConfig {
    pub fn new(
        enabled: bool,
        silence_timeout_ms: u64,
        energy_threshold: f32,
        chunk_ms: u64,
        debug: bool,
    ) -> Self {
        Self {
            enabled,
            energy_threshold,
            silence_timeout: Duration::from_millis(silence_timeout_ms),
            chunk_size: Duration::from_millis(chunk_ms),
            debug,
        }
    }
}

pub fn list_input_devices(host: &cpal::Host) -> Result<Vec<String>, AudioError> {
    let devices = host.input_devices().map_err(|err| {
        AudioError::new(
            AudioErrorKind::DeviceQuery,
            format!("failed to list input devices: {err}"),
        )
    })?;

    let mut names = Vec::new();
    for device in devices {
        let name = device.name().map_err(|err| {
            AudioError::new(
                AudioErrorKind::DeviceQuery,
                format!("failed to read device name: {err}"),
            )
        })?;
        names.push(name);
    }

    if names.is_empty() {
        return Err(AudioError::new(
            AudioErrorKind::DeviceUnavailable,
            "no input devices available",
        ));
    }

    Ok(names)
}

pub fn configure_alsa_logging(debug_audio: bool) {
    #[cfg(target_os = "linux")]
    {
        let handler = if debug_audio {
            None
        } else {
            Some(
                silence_alsa_error
                    as unsafe extern "C" fn(
                        *const std::os::raw::c_char,
                        std::os::raw::c_int,
                        *const std::os::raw::c_char,
                        std::os::raw::c_int,
                        *const std::os::raw::c_char,
                        *mut alsa_sys::__va_list_tag,
                    ),
            )
        };

        unsafe {
            alsa_sys::snd_lib_error_set_local(handler);
        }
    }
}

#[cfg(target_os = "linux")]
unsafe extern "C" fn silence_alsa_error(
    _file: *const std::os::raw::c_char,
    _line: std::os::raw::c_int,
    _func: *const std::os::raw::c_char,
    _err: std::os::raw::c_int,
    _fmt: *const std::os::raw::c_char,
    _arg: *mut alsa_sys::__va_list_tag,
) {
}

pub fn start_capture(
    host: &cpal::Host,
    device_name: Option<&str>,
    sample_rate: u32,
) -> Result<Capture, AudioError> {
    let device = select_input_device(host, device_name)?;
    let device_label = device.name().map_err(|err| {
        AudioError::new(
            AudioErrorKind::DeviceQuery,
            format!("failed to read input device name: {err}"),
        )
    })?;
    println!("Selected input device: {device_label}");

    let (stream_config, sample_format) = select_stream_config(&device, sample_rate)?;
    println!(
        "Input stream: {:?}, {} ch, {} Hz",
        sample_format, stream_config.channels, stream_config.sample_rate.0
    );

    let ring = HeapRb::<f32>::new(sample_rate as usize * 30);
    let (producer, consumer) = ring.split();
    let overflow = Arc::new(Mutex::new(Vec::new()));
    let overflow_count = Arc::new(AtomicUsize::new(0));

    let stream = match sample_format {
        cpal::SampleFormat::F32 => build_input_stream::<f32>(
            &device,
            &stream_config,
            producer,
            &overflow,
            &overflow_count,
        )?,
        cpal::SampleFormat::F64 => build_input_stream::<f64>(
            &device,
            &stream_config,
            producer,
            &overflow,
            &overflow_count,
        )?,
        cpal::SampleFormat::I8 => build_input_stream::<i8>(
            &device,
            &stream_config,
            producer,
            &overflow,
            &overflow_count,
        )?,
        cpal::SampleFormat::I16 => build_input_stream::<i16>(
            &device,
            &stream_config,
            producer,
            &overflow,
            &overflow_count,
        )?,
        cpal::SampleFormat::I32 => build_input_stream::<i32>(
            &device,
            &stream_config,
            producer,
            &overflow,
            &overflow_count,
        )?,
        cpal::SampleFormat::U8 => build_input_stream::<u8>(
            &device,
            &stream_config,
            producer,
            &overflow,
            &overflow_count,
        )?,
        cpal::SampleFormat::U16 => build_input_stream::<u16>(
            &device,
            &stream_config,
            producer,
            &overflow,
            &overflow_count,
        )?,
        cpal::SampleFormat::U32 => build_input_stream::<u32>(
            &device,
            &stream_config,
            producer,
            &overflow,
            &overflow_count,
        )?,
        format => {
            return Err(AudioError::new(
                AudioErrorKind::StreamConfig,
                format!("unsupported sample format: {format:?}"),
            ));
        }
    };

    stream.play().map_err(|err| {
        AudioError::new(
            AudioErrorKind::StreamStart,
            format!("failed to start input stream: {err}"),
        )
    })?;

    Ok(Capture {
        _stream: stream,
        consumer,
        overflow,
        overflow_count,
    })
}

pub fn drain_samples(capture: &mut Capture, output: &mut Vec<f32>) {
    while let Some(sample) = capture.consumer.pop() {
        output.push(sample);
    }
    let mut overflow = capture
        .overflow
        .lock()
        .expect("overflow buffer lock poisoned");
    if !overflow.is_empty() {
        let drained = overflow.len();
        output.extend(overflow.drain(..));
        capture.overflow_count.store(0, Ordering::Release);
        eprintln!("audio overflow drained: {drained} samples");
    }
}

pub fn trim_trailing_silence(samples: &[f32], sample_rate: u32, vad: &VadConfig) -> Vec<f32> {
    if samples.is_empty() || !vad.enabled {
        return samples.to_vec();
    }

    let chunk_samples = duration_to_samples(sample_rate, vad.chunk_size).max(1);
    let max_silence_ms = vad.silence_timeout.as_millis() as u64;
    let mut silence_ms = 0u64;
    let mut end = samples.len();

    while end > 0 {
        let start = end.saturating_sub(chunk_samples);
        let chunk = &samples[start..end];
        let rms = rms_energy(chunk);
        if rms >= vad.energy_threshold {
            break;
        }
        silence_ms += samples_to_ms(chunk.len(), sample_rate);
        end = start;
        if silence_ms >= max_silence_ms {
            break;
        }
    }

    samples[..end].to_vec()
}

fn rms_energy(samples: &[f32]) -> f32 {
    let sum_squares = samples.iter().map(|sample| sample * sample).sum::<f32>();
    (sum_squares / samples.len() as f32).sqrt()
}

fn duration_to_samples(sample_rate: u32, duration: Duration) -> usize {
    let seconds = duration.as_secs_f32();
    (sample_rate as f32 * seconds).round() as usize
}

pub fn samples_to_ms(samples: usize, sample_rate: u32) -> u64 {
    if sample_rate == 0 {
        return 0;
    }
    ((samples as f32 / sample_rate as f32) * 1000.0).round() as u64
}

fn select_input_device(host: &cpal::Host, name: Option<&str>) -> Result<cpal::Device, AudioError> {
    if let Some(target) = name {
        let target_lower = target.to_lowercase();
        let devices = host.input_devices().map_err(|err| {
            AudioError::new(
                AudioErrorKind::DeviceQuery,
                format!("failed to list input devices: {err}"),
            )
        })?;

        for device in devices {
            let device_name = device.name().map_err(|err| {
                AudioError::new(
                    AudioErrorKind::DeviceQuery,
                    format!("failed to read device name: {err}"),
                )
            })?;
            if device_name.to_lowercase() == target_lower {
                return Ok(device);
            }
        }

        return Err(AudioError::new(
            AudioErrorKind::DeviceNotFound,
            format!("input device not found: {target}"),
        ));
    }

    host.default_input_device().ok_or_else(|| {
        AudioError::new(
            AudioErrorKind::DeviceUnavailable,
            "no default input device available",
        )
    })
}

fn select_stream_config(
    device: &cpal::Device,
    sample_rate: u32,
) -> Result<(cpal::StreamConfig, cpal::SampleFormat), AudioError> {
    let configs = device.supported_input_configs().map_err(|err| {
        AudioError::new(
            AudioErrorKind::StreamConfig,
            format!("failed to query input configs: {err}"),
        )
    })?;

    let mut best: Option<(cpal::StreamConfig, cpal::SampleFormat)> = None;
    let mut best_rank = 0u8;
    for config in configs {
        let min_rate = config.min_sample_rate().0;
        let max_rate = config.max_sample_rate().0;
        if sample_rate < min_rate || sample_rate > max_rate {
            continue;
        }

        let channels = config.channels();
        let stream_config = cpal::StreamConfig {
            channels,
            sample_rate: cpal::SampleRate(sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        let format = config.sample_format();
        let candidate = (stream_config, format);
        let rank = sample_format_rank(format);
        if channels == 1 {
            if best.is_none() || rank > best_rank {
                best = Some(candidate);
                best_rank = rank;
            }
        } else if best.is_none() || rank > best_rank {
            best = Some(candidate);
            best_rank = rank;
        }
    }

    best.ok_or_else(|| {
        AudioError::new(
            AudioErrorKind::StreamConfig,
            format!("no input config supports {sample_rate} Hz"),
        )
    })
}

fn sample_format_rank(format: cpal::SampleFormat) -> u8 {
    match format {
        cpal::SampleFormat::F32 => 6,
        cpal::SampleFormat::F64 => 5,
        cpal::SampleFormat::I32 => 4,
        cpal::SampleFormat::I16 => 3,
        cpal::SampleFormat::U16 => 2,
        cpal::SampleFormat::I8 => 1,
        cpal::SampleFormat::U8 => 0,
        _ => 0,
    }
}

fn build_input_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut producer: HeapProducer<f32>,
    overflow: &Arc<Mutex<Vec<f32>>>,
    overflow_count: &Arc<AtomicUsize>,
) -> Result<cpal::Stream, AudioError>
where
    T: cpal::Sample + cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    let channels = config.channels as usize;
    let overflow = Arc::clone(overflow);
    let overflow_count = Arc::clone(overflow_count);
    let mut overflow_scratch = Vec::new();
    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                overflow_scratch.clear();
                let mut force_overflow = overflow_count.load(Ordering::Acquire) > 0;
                if channels == 1 {
                    for sample in data {
                        let value = sample.to_sample::<f32>();
                        if force_overflow || producer.push(value).is_err() {
                            overflow_scratch.push(value);
                            force_overflow = true;
                        }
                    }
                } else {
                    for frame in data.chunks(channels) {
                        if frame.is_empty() {
                            continue;
                        }
                        let mut sum = 0.0f32;
                        for sample in frame {
                            sum += sample.to_sample::<f32>();
                        }
                        let mono = sum / frame.len() as f32;
                        if force_overflow || producer.push(mono).is_err() {
                            overflow_scratch.push(mono);
                            force_overflow = true;
                        }
                    }
                }

                if !overflow_scratch.is_empty() {
                    let overflow_len = overflow_scratch.len();
                    let was_empty = overflow_count.load(Ordering::Acquire) == 0;
                    let mut overflow = overflow.lock().expect("overflow buffer lock poisoned");
                    overflow.extend(overflow_scratch.drain(..));
                    overflow_count.fetch_add(overflow_len, Ordering::Release);
                    if was_empty {
                        eprintln!("audio overflow detected: queued {overflow_len} samples");
                    }
                }
            },
            move |err| {
                eprintln!("audio input error: {err}");
            },
            None,
        )
        .map_err(|err| {
            AudioError::new(
                AudioErrorKind::StreamBuild,
                format!("failed to build input stream: {err}"),
            )
        })
}
