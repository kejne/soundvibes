# PRD: Offline Voice-to-Text CLI (Linux)

## Problem
Linux users need a simple, offline push-to-talk voice-to-text tool that does not require installing heavy runtimes or managing complex dependencies.

## Goals
- Provide push-to-talk recording from the default microphone with transcription on key release.
- Work fully offline with a small model and fast post-recording transcription.
- Run as a background daemon that listens for control commands over a local socket.
- Use the same binary: `sv --daemon` runs the service, `sv` toggles capture via the socket.
- Ship as a single Rust CLI binary plus a bundled model file.

## Target Users
- Linux developers and power users who want local voice-to-text.
- Privacy-sensitive users who cannot use cloud APIs.

## Scope (MVP)
- CLI that captures audio from the default input device.
- Push-to-talk recording: hold a key to record, transcribe and print on release.
- Small offline model (whisper.cpp tiny/base with quantization).
- Configuration via `config.toml` in the XDG config directory.
- Works on Linux x86_64.
- Daemon mode that listens on a local socket and captures on toggle.
- Socket-controlled CLI toggle for start/stop using the same binary.
- Daemon mode can inject transcribed text at the cursor when requested.

## Non-Goals (MVP)
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
- In daemon mode, the capture key injects text into the focused app instead of stdout.

## Output Behavior
- One final transcript emitted on key release.
- JSONL mode emits objects with `type`, `text`, `timestamp`.

## Exit Codes
- `0`: success.
- `2`: invalid config or missing model.
- `3`: audio device error.

## Architecture (High Level)
- Audio capture: `cpal` for mic input at 16 kHz mono.
- Push-to-talk buffer: capture while toggled on, stop on toggle off.
- Optional VAD: trim trailing silence after release.
- Inference: whisper.cpp via Rust FFI bindings, using quantized small models.
- Output: final text output to stdout after transcription completes.
- Control plane: daemon listens on a local socket for toggle commands.
- Text injection: Wayland portal virtual keyboard or X11 XTest.

## Model Choice
- Engine: whisper.cpp (FFI) for best accuracy-to-size tradeoff.
- Initial model: tiny/base quantized (ggml).
- Model is bundled and loaded from a local path.

## Performance Assumptions
- Best-effort latency on CPU for a small model.
- Acceptable transcription time after key release.
- No hard latency SLA in MVP.

## Packaging & Distribution
- Single compiled Rust binary.
- Bundle model file alongside the binary.
- Provide a simple tarball release for Linux.

## Configuration
- Load config from XDG base directory if available.
- Default path: `${XDG_CONFIG_HOME:-~/.config}/soundvibes/config.toml`.
- Config file format: TOML.
- Config keys: `model`, `language`, `device`, `sample_rate`, `format`, `hotkey`, `vad`, `mode`.

## Validation Plan
- Manual test on Linux laptop with default microphone.
- Verify transcript appears shortly after toggling capture off.
- Confirm tool runs without network access.
- Validate daemon socket toggle from the CLI.
- Validate text injection into a focused editor.

## Risks & Mitigations
- CPU performance too slow: use smaller quantized model and VAD.
- Audio capture issues on some devices: provide device selection flag.
- Model size too large: allow user to swap model via CLI flag.
- Daemon not running: surface an actionable error from the CLI toggle.
- Text injection permissions vary by compositor: document portal prompts and limitations.
