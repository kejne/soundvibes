# PRD: Soundvibes Offline Voice-to-Text CLI (Linux)

## Problem
Linux users need a simple, offline start/stop voice-to-text tool that does not require installing heavy runtimes or managing complex dependencies.

This PRD is a living document and should be updated as product requirements and behavior evolve.

## Goals
- Provide start/stop recording from the default microphone with transcription after capture stops.
- Work fully offline with a small model and fast post-recording transcription.
- Run as a background daemon that listens for control commands over a local socket.
- Use the same binary: `sv --daemon` runs the service, `sv` toggles capture via the socket.
- Ship as a single Rust CLI binary plus an automatically downloaded model file.
- Automatically accelerate inference on NVIDIA/AMD GPUs when available, otherwise fall back to CPU.

## Target Users
- Linux developers and power users who want local voice-to-text.
- Privacy-sensitive users who cannot use cloud APIs.

## Scope
- CLI that captures audio from the default input device.
- Start/stop recording: capture audio while toggled on, transcribe and print when stopped.
- Small offline model (whisper.cpp small with quantization by default).
- Automatic GPU backend selection for NVIDIA/AMD devices with CPU fallback.
- Configuration via `config.toml` in the XDG config directory.
- Works on Linux x86_64.
- Daemon mode that listens on a local socket and captures on toggle.
- Socket-controlled CLI toggle for start/stop using the same binary.
- Daemon mode can inject transcribed text at the cursor when requested.

## Non-Goals
- GUI or tray integration.
- Speaker diarization.
- Automatic punctuation or formatting.
- Cloud sync or remote APIs.
- Cross-platform support outside Linux.

## User Experience
- Command: `sv --daemon` to start the background service.
- Command: `sv` to toggle capture state via the daemon socket.
- Configure model and options in the config file, then run the daemon.
- When capture is toggled on, start recording; when toggled off, transcribe and output.
- Errors are returned with actionable messages (missing model, no mic, unsupported device).
- In daemon mode, the capture toggle injects text into the focused app instead of stdout.

## Output Behavior
- One final transcript emitted after capture stops.
- JSONL mode emits objects with `type`, `text`, `timestamp`.

## Exit Codes
- `0`: success.
- `2`: invalid config or missing model.
- `3`: audio device error.

## Architecture (High Level)
- Audio capture: `cpal` for mic input at 16 kHz mono.
- Push-to-talk buffer: capture while toggled on, stop on toggle off.
- Optional VAD: trim trailing silence after release.
- Inference: whisper.cpp via Rust FFI bindings, using quantized small models with GPU acceleration when available.
- Output: final text output to stdout after transcription completes.
- Control plane: daemon listens on a local socket for toggle commands.
- Text injection: Wayland portal virtual keyboard or X11 XTest.

## Model Choice
- Engine: whisper.cpp (FFI) for best accuracy-to-size tradeoff.
- Default model: small general (ggml).
- Model is downloaded on demand to a default location and loaded locally.

## Performance Assumptions
- Best-effort latency on CPU for a small model.
- Acceptable transcription time after capture stops.
- No hard latency SLA in the initial release.
- GPU acceleration is opportunistic and should not require user configuration.

## Packaging & Distribution
- Single compiled Rust binary.
- Download model file to a default data directory on first use.
- Provide a simple tarball release for Linux.

## Configuration
- Load config from XDG base directory if available.
- Default path: `${XDG_CONFIG_HOME:-~/.config}/soundvibes/config.toml`.
- Config file format: TOML.
- Config keys: `model`, `model_path`, `model_size`, `model_language`, `download_model`, `language`, `device`, `sample_rate`, `format`, `vad`, `mode`.

## Validation Plan
- Manual test on Linux laptop with default microphone.
- Verify transcript appears shortly after toggling capture off.
- Confirm tool runs without network access.
- Validate daemon socket toggle from the CLI.
- Validate text injection into a focused editor.

## Risks & Mitigations
- CPU performance too slow: use smaller quantized model and VAD.
- Missing GPU runtime: fall back to CPU and document GPU prerequisites.
- Audio capture issues on some devices: provide device selection flag.
- Model size too large: allow user to swap model via CLI flag.
- Daemon not running: surface an actionable error from the CLI toggle.
- Text injection permissions vary by compositor: document portal prompts and limitations.
