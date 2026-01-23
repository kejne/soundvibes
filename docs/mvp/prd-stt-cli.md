# PRD: Offline Voice-to-Text CLI (Linux)

## Problem
Linux users need a simple, offline, real-time voice-to-text tool that does not require installing heavy runtimes or managing complex dependencies.

## Goals
- Provide real-time transcription from the default microphone.
- Work fully offline with a small model and best-effort latency.
- Ship as a single Rust CLI binary plus a bundled model file.

## Target Users
- Linux developers and power users who want local voice-to-text.
- Privacy-sensitive users who cannot use cloud APIs.

## Scope (MVP)
- CLI that captures audio from the default input device.
- Streaming, incremental transcription to stdout.
- Small offline model (whisper.cpp tiny/base with quantization).
- Configurable flags for model path, language, and input device.
- Works on Linux x86_64.

## Non-Goals (MVP)
- GUI or tray integration.
- Speaker diarization.
- Automatic punctuation or formatting.
- Cloud sync or remote APIs.

## User Experience
- Command: `stt --model ./models/ggml-tiny.en.bin`
- Streaming output prints partial results as they change and final results on end-of-utterance.
- Errors are returned with actionable messages (missing model, no mic, unsupported device).

## CLI Spec (Draft)
- Binary name: `sv` (SoundVibes)
- Required flags:
  - `--model <path>`: path to a local whisper.cpp ggml model file.
- Optional flags:
  - `--language <code>`: language hint (default: `auto`).
  - `--device <name>`: input device name or index (default: system default).
  - `--sample-rate <hz>`: capture rate (default: 16000).
  - `--format <mode>`: output format (`plain`, `jsonl`).
  - `--vad <on|off>`: enable voice activity detection (default: on).
- Output behavior:
  - Partial transcripts update frequently on stdout.
  - Final transcripts emit on end-of-utterance boundary.
  - JSONL mode emits objects with `type`, `text`, `timestamp`.
- Exit codes:
  - `0`: success.
  - `2`: invalid flags or missing model.
  - `3`: audio device error.

## Architecture (High Level)
- Audio capture: `cpal` for mic input at 16 kHz mono.
- Audio chunking: 200-500 ms frames in a ring buffer.
- Optional VAD: to reduce wasted inference and mark end-of-utterance.
- Inference: whisper.cpp via Rust FFI bindings, using quantized small models.
- Output: incremental text output to stdout with finalization on VAD stop.

## Model Choice
- Engine: whisper.cpp (FFI) for best accuracy-to-size tradeoff.
- Initial model: tiny/base quantized (ggml).
- Model is bundled and loaded from a local path.

## Performance Assumptions
- Best-effort latency on CPU for a small model.
- Acceptable real-time streaming with partial results within a short interval.
- No hard latency SLA in MVP.

## Packaging & Distribution
- Single compiled Rust binary.
- Bundle model file alongside the binary.
- Provide a simple tarball release for Linux.

## Validation Plan
- Manual test on Linux laptop with default microphone.
- Verify incremental output appears as speech is captured.
- Confirm tool runs without network access.

## Risks & Mitigations
- CPU performance too slow: use smaller quantized model and VAD.
- Audio capture issues on some devices: provide device selection flag.
- Model size too large: allow user to swap model via CLI flag.
