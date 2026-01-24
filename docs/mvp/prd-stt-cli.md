# PRD: Offline Voice-to-Text CLI (Linux)

## Problem
Linux users need a simple, offline push-to-talk voice-to-text tool that does not require installing heavy runtimes or managing complex dependencies.

## Goals
- Provide push-to-talk recording from the default microphone with transcription on key release.
- Work fully offline with a small model and fast post-recording transcription.
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

## Non-Goals (MVP)
- GUI or tray integration.
- Speaker diarization.
- Automatic punctuation or formatting.
- Cloud sync or remote APIs.

## User Experience
- Command: `sv`
- Configure model and options in the config file, then run the CLI.
- Hold the capture key to record; release to transcribe and print the final text.
- Errors are returned with actionable messages (missing model, no mic, unsupported device).

## Output Behavior
- One final transcript emitted on key release.
- JSONL mode emits objects with `type`, `text`, `timestamp`.

## Exit Codes
- `0`: success.
- `2`: invalid config or missing model.
- `3`: audio device error.

## Architecture (High Level)
- Audio capture: `cpal` for mic input at 16 kHz mono.
- Push-to-talk buffer: capture while key is held, stop on release.
- Optional VAD: trim trailing silence after release.
- Inference: whisper.cpp via Rust FFI bindings, using quantized small models.
- Output: final text output to stdout after transcription completes.

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
- Config keys: `model`, `language`, `device`, `sample_rate`, `format`, `hotkey`, `vad`.

## Validation Plan
- Manual test on Linux laptop with default microphone.
- Verify transcript appears shortly after key release.
- Confirm tool runs without network access.

## Risks & Mitigations
- CPU performance too slow: use smaller quantized model and VAD.
- Audio capture issues on some devices: provide device selection flag.
- Model size too large: allow user to swap model via CLI flag.
