# Technical Design: Soundvibes Offline Voice-to-Text CLI

## Overview
This document describes the technical design for the `sv` CLI that performs offline, start/stop voice-to-text on Linux using whisper.cpp with a small quantized model.

## Goals
- Single binary plus automatic model download to a local path.
- Start/stop capture with transcription after capture stops.
- Best-effort latency on CPU.
- Support daemon mode with socket-based control and text injection at the cursor.
- Automatically use NVIDIA/AMD GPUs for inference when available, falling back to CPU.

## Architecture
- CLI entrypoint loads configuration.
- Command listener controls capture start/stop.
- Audio capture pipeline reads microphone input via `cpal` while recording is toggled on.
- A buffer aggregates audio frames for post-recording inference.
- Optional VAD trims trailing silence after release.
- whisper.cpp runs inference on the captured buffer.
- Output stream prints a final transcript.
- Optional daemon service runs continuously and injects text into the focused app.

## Components

### Config
- Load settings from `${XDG_CONFIG_HOME:-~/.config}/soundvibes/config.toml`.
- CLI flags complement configuration and override file values when present.
- Defaults are applied if keys are missing.
- Configuration struct shared across pipeline components.
- Add `mode` to select `stdout` (default) or `inject` for daemon output.
- Config supports `model_language` and `model_size` selection with a default of the small general model.
- Allow overriding the model install path (`model_path`) while keeping a default data directory.

### Audio Capture
- Use `cpal` to select input device and stream 16 kHz mono.
- Convert samples to `f32` normalized range [-1.0, 1.0].
- Capture samples while recording is toggled on.

### Buffering
- Store samples for the duration of the recording window.
- Optional chunking to avoid excessive memory for long holds.

### VAD (Voice Activity Detection)
- Optional VAD to trim trailing silence after release.
- Simple energy-based threshold to start; upgradeable later.

### Command Control
- Run `sv --daemon` to start the background service.
- Run `sv` to send a toggle command to the daemon over a Unix socket.
- Store the socket in `${XDG_RUNTIME_DIR}/soundvibes/sv.sock`.
- Provide actionable errors when the daemon socket is unavailable.

### Text Injection
- Use a backend abstraction for output delivery.
- Wayland: use portal virtual keyboard or input capture APIs.
- X11: use XTest to synthesize keypresses into the focused window.
- If injection is unavailable, fallback to stdout with a warning.

### Daemon Mode
- Long-running process that listens for toggle commands on a Unix socket.
- On toggle on, start capture; on toggle off, complete transcription.
- On capture completion, either print or inject text based on `mode`.
- Systemd user unit or foreground mode used to manage lifecycle.

### Inference Engine
- whisper.cpp bound via Rust FFI.
- Ensure the configured ggml model is downloaded before loading at startup.
- Run inference on captured audio and return a final transcript.
- Use a small quantized model for CPU speed.
- Attempt GPU acceleration automatically; fall back to CPU when no supported GPU backend is detected.

### Model Download
- On `sv`/`sv --daemon` startup, check for the configured model in the default data directory.
- Download the ggml model if missing, based on `model_language` and `model_size` config (defaults to small + general).
- If `model_path` is provided, download or resolve the model there instead of the default location.

### GPU Backend Selection
- Build whisper.cpp with GPU backends enabled (Vulkan for AMD/NVIDIA, CUDA for NVIDIA when available).
- Always enable GPU usage in runtime params; rely on whisper.cpp backend detection to select the first supported device.
- Do not expose GPU selection to the user; if no GPU backend is available, inference continues on CPU.

### Output Formatting
- `plain`: print final transcript after transcription completes.
- `jsonl`: emit a JSON line with `type`, `text`, `timestamp`.

## Configuration
- Format: TOML.
- Example fields: `model`, `model_path`, `model_size`, `model_language`, `download_model`, `language`, `device`, `sample_rate`, `format`, `vad`, `mode`.

## Data Flow
1. CLI loads config and model.
2. CLI toggle command starts audio capture in the daemon.
3. Audio capture stores samples until toggle off.
4. Optional VAD trims trailing silence.
5. Inference runs on captured audio, returns final text.
6. Output formatter prints or injects final result.

## Error Handling
- Missing model: exit code 2 with message.
- No input device: exit code 3 with message.
- Stream errors: log and exit gracefully.
- Daemon socket missing: emit actionable guidance for starting `sv --daemon`.

## Validation
- Manual mic test with `sv` using a valid config file.
- Validate final transcript after capture stops.
- Confirm offline operation by disconnecting network.
- Validate socket toggle commands against the daemon.
- Validate injection into a focused editor.
- Validate GPU usage on NVIDIA/AMD systems by checking whisper.cpp startup logs, and verify CPU fallback on systems without a supported GPU backend.

## Open Questions
- Best default model (tiny vs base) for CPU speed.
- VAD threshold calibration on typical laptop microphones.
- Best supported portal for text injection across compositors.
