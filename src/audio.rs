use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::{HeapConsumer, HeapProducer, HeapRb};
use std::time::Duration;

pub struct Capture {
    _stream: cpal::Stream,
    consumer: HeapConsumer<f32>,
}

pub const DEFAULT_CHUNK_MS: u64 = 250;
pub const DEFAULT_VAD_THRESHOLD: f32 = 0.015;
pub const DEFAULT_SILENCE_TIMEOUT_MS: u64 = 800;

pub struct VadConfig {
    pub enabled: bool,
    pub energy_threshold: f32,
    pub silence_timeout: Duration,
    pub chunk_size: Duration,
    pub debug: bool,
}

#[derive(Debug, Copy, Clone)]
pub struct SegmentInfo {
    pub index: u64,
    pub duration_ms: u64,
}

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

pub fn list_input_devices(host: &cpal::Host) -> Result<Vec<String>, String> {
    let devices = host
        .input_devices()
        .map_err(|err| format!("failed to list input devices: {err}"))?;

    let mut names = Vec::new();
    for device in devices {
        let name = device
            .name()
            .map_err(|err| format!("failed to read device name: {err}"))?;
        names.push(name);
    }

    if names.is_empty() {
        return Err("no input devices available".to_string());
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
) -> Result<Capture, String> {
    let device = select_input_device(host, device_name)?;
    let device_label = device
        .name()
        .map_err(|err| format!("failed to read input device name: {err}"))?;
    println!("Selected input device: {device_label}");

    let (stream_config, sample_format) = select_stream_config(&device, sample_rate)?;

    let ring = HeapRb::<f32>::new(sample_rate as usize * 5);
    let (producer, consumer) = ring.split();

    let stream = match sample_format {
        cpal::SampleFormat::F32 => build_input_stream::<f32>(&device, &stream_config, producer)?,
        cpal::SampleFormat::F64 => build_input_stream::<f64>(&device, &stream_config, producer)?,
        cpal::SampleFormat::I8 => build_input_stream::<i8>(&device, &stream_config, producer)?,
        cpal::SampleFormat::I16 => build_input_stream::<i16>(&device, &stream_config, producer)?,
        cpal::SampleFormat::I32 => build_input_stream::<i32>(&device, &stream_config, producer)?,
        cpal::SampleFormat::U8 => build_input_stream::<u8>(&device, &stream_config, producer)?,
        cpal::SampleFormat::U16 => build_input_stream::<u16>(&device, &stream_config, producer)?,
        cpal::SampleFormat::U32 => build_input_stream::<u32>(&device, &stream_config, producer)?,
        format => {
            return Err(format!("unsupported sample format: {format:?}"));
        }
    };

    stream
        .play()
        .map_err(|err| format!("failed to start input stream: {err}"))?;

    Ok(Capture {
        _stream: stream,
        consumer,
    })
}

pub fn stream_segments<F>(
    mut capture: Capture,
    sample_rate: u32,
    vad: VadConfig,
    mut on_segment: F,
) -> Result<(), String>
where
    F: FnMut(&[f32], SegmentInfo) -> Result<(), String>,
{
    let chunk_samples = duration_to_samples(sample_rate, vad.chunk_size).max(1);
    let mut chunk = Vec::with_capacity(chunk_samples);
    let mut utterance_samples = Vec::new();
    let mut idle = 0;
    let mut in_speech = false;
    let mut silence_ms = 0u64;
    let mut utterance_index = 0u64;

    loop {
        chunk.clear();
        while chunk.len() < chunk_samples {
            if let Some(sample) = capture.consumer.pop() {
                chunk.push(sample);
            } else {
                break;
            }
        }

        if chunk.is_empty() {
            idle = (idle + 1).min(20);
            let delay = if idle > 5 { 200 } else { 80 };
            std::thread::sleep(Duration::from_millis(delay));
            continue;
        }

        idle = 0;
        let rms = rms_energy(&chunk);
        if vad.debug {
            println!(
                "VAD rms {rms:.4} threshold {threshold:.4} in_speech {in_speech}",
                threshold = vad.energy_threshold,
            );
        }

        if !vad.enabled {
            utterance_index += 1;
            let duration_ms = samples_to_ms(chunk.len(), sample_rate);
            on_segment(
                &chunk,
                SegmentInfo {
                    index: utterance_index,
                    duration_ms,
                },
            )?;
            std::thread::sleep(Duration::from_millis(40));
            continue;
        }

        if rms >= vad.energy_threshold {
            if !in_speech {
                in_speech = true;
                silence_ms = 0;
                utterance_samples.clear();
                utterance_index += 1;
                println!("Speech started (utterance {utterance_index})");
            }
            utterance_samples.extend_from_slice(&chunk);
        } else if in_speech {
            silence_ms += samples_to_ms(chunk.len(), sample_rate);
            if silence_ms >= vad.silence_timeout.as_millis() as u64 {
                let duration_ms = samples_to_ms(utterance_samples.len(), sample_rate);
                println!("Speech ended (utterance {utterance_index}, {duration_ms} ms)");
                on_segment(
                    &utterance_samples,
                    SegmentInfo {
                        index: utterance_index,
                        duration_ms,
                    },
                )?;
                in_speech = false;
                silence_ms = 0;
                utterance_samples.clear();
            }
        }

        std::thread::sleep(Duration::from_millis(20));
    }
}

fn rms_energy(samples: &[f32]) -> f32 {
    let sum_squares = samples.iter().map(|sample| sample * sample).sum::<f32>();
    (sum_squares / samples.len() as f32).sqrt()
}

fn duration_to_samples(sample_rate: u32, duration: Duration) -> usize {
    let seconds = duration.as_secs_f32();
    (sample_rate as f32 * seconds).round() as usize
}

fn samples_to_ms(samples: usize, sample_rate: u32) -> u64 {
    if sample_rate == 0 {
        return 0;
    }
    ((samples as f32 / sample_rate as f32) * 1000.0).round() as u64
}

fn select_input_device(host: &cpal::Host, name: Option<&str>) -> Result<cpal::Device, String> {
    if let Some(target) = name {
        let target_lower = target.to_lowercase();
        let devices = host
            .input_devices()
            .map_err(|err| format!("failed to list input devices: {err}"))?;

        for device in devices {
            let device_name = device
                .name()
                .map_err(|err| format!("failed to read device name: {err}"))?;
            if device_name.to_lowercase() == target_lower {
                return Ok(device);
            }
        }

        return Err(format!("input device not found: {target}"));
    }

    host.default_input_device()
        .ok_or_else(|| "no default input device available".to_string())
}

fn select_stream_config(
    device: &cpal::Device,
    sample_rate: u32,
) -> Result<(cpal::StreamConfig, cpal::SampleFormat), String> {
    let configs = device
        .supported_input_configs()
        .map_err(|err| format!("failed to query input configs: {err}"))?;

    let mut best: Option<(cpal::StreamConfig, cpal::SampleFormat)> = None;
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

        let candidate = (stream_config, config.sample_format());
        if channels == 1 {
            return Ok(candidate);
        }

        if best.is_none() {
            best = Some(candidate);
        }
    }

    best.ok_or_else(|| format!("no input config supports {sample_rate} Hz"))
}

fn build_input_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut producer: HeapProducer<f32>,
) -> Result<cpal::Stream, String>
where
    T: cpal::Sample + cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    let channels = config.channels as usize;
    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                if channels == 1 {
                    for sample in data {
                        let _ = producer.push(sample.to_sample::<f32>());
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
                        let _ = producer.push(mono);
                    }
                }
            },
            move |err| {
                eprintln!("audio input error: {err}");
            },
            None,
        )
        .map_err(|err| format!("failed to build input stream: {err}"))
}
