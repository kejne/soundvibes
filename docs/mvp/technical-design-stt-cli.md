# Technical Design: Offline STT CLI (sv)

## Overview
This document describes the technical design for the `sv` CLI that performs offline, push-to-talk speech-to-text on Linux using whisper.cpp with a small quantized model.

## Goals
- Single binary plus local model file.
- Push-to-talk capture with transcription on key release.
- Best-effort latency on CPU.
- Support daemon mode with socket-based control and text injection at the cursor.

## Architecture
- CLI entrypoint loads configuration.
- Command listener controls capture start/stop.
- Audio capture pipeline reads microphone input via `cpal` while key is held.
- A buffer aggregates audio frames for post-recording inference.
- Optional VAD trims trailing silence after release.
- whisper.cpp runs inference on the captured buffer.
- Output stream prints a final transcript.
- Optional daemon service runs continuously and injects text into the focused app.

## Components

### Config
- Load settings from `${XDG_CONFIG_HOME:-~/.config}/soundvibes/config.toml`.
- No CLI flags in MVP; configuration is file-only.
- Defaults are applied if keys are missing.
- Configuration struct shared across pipeline components.
- Add `mode` to select `stdout` (default) or `inject` for daemon output.

### Audio Capture
- Use `cpal` to select input device and stream 16 kHz mono.
- Convert samples to `f32` normalized range [-1.0, 1.0].
- Capture samples while the hotkey is held.

### Buffering
- Store samples for the duration of the key hold.
- Optional chunking to avoid excessive memory for long holds.

### VAD (Voice Activity Detection)
- Optional VAD to trim trailing silence after release.
- Simple energy-based threshold to start; upgradeable later.

### Hotkey Capture
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
- Load ggml model at startup.
- Run inference on captured audio and return a final transcript.
- Use a small quantized model for CPU speed.

### Output Formatting
- `plain`: print final transcript after transcription completes.
- `jsonl`: emit a JSON line with `type`, `text`, `timestamp`.

## Configuration
- Format: TOML.
- Example fields: `model`, `language`, `device`, `sample_rate`, `format`, `hotkey`, `vad`, `mode`.

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
- Validate final transcript after key release.
- Confirm offline operation by disconnecting network.
- Validate socket toggle commands against the daemon.
- Validate injection into a focused editor.

## Open Questions
- Best default model (tiny vs base) for CPU speed.
- VAD threshold calibration on typical laptop microphones.
- Best supported portal for text injection across compositors.
