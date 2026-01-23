# Technical Design: Offline STT CLI (sv)

## Overview
This document describes the technical design for the `sv` CLI that performs offline, real-time speech-to-text on Linux using whisper.cpp with a small quantized model.

## Goals
- Single binary plus local model file.
- Real-time streaming output with partial and final transcripts.
- Best-effort latency on CPU.

## Architecture
- CLI entrypoint parses flags and config.
- Audio capture pipeline reads microphone input via `cpal`.
- A ring buffer aggregates audio frames into short chunks.
- VAD determines end-of-utterance boundaries.
- whisper.cpp runs inference on chunks and returns text updates.
- Output stream prints partial and final transcripts.

## Components

### CLI and Config
- Argument parser (e.g., `clap`) for:
  - `--model <path>`
  - `--language <code>`
  - `--device <name>`
  - `--sample-rate <hz>`
  - `--format <mode>`
  - `--vad <on|off>`
- Configuration struct shared across pipeline components.

### Audio Capture
- Use `cpal` to select input device and stream 16 kHz mono.
- Convert samples to `f32` normalized range [-1.0, 1.0].
- Push samples into a lock-free ring buffer.

### Chunking and Buffering
- Chunk size: 200-500 ms of audio.
- Sliding window for partial inference.
- Maintain a short history buffer for context.

### VAD (Voice Activity Detection)
- Optional VAD to avoid inference on silence.
- Simple energy-based threshold to start; upgradeable later.
- On silence timeout, finalize current transcript segment.

### Inference Engine
- whisper.cpp bound via Rust FFI.
- Load ggml model at startup.
- Run inference on each chunk and return partial transcript.
- Use a small quantized model for CPU speed.

### Output Formatting
- `plain`: print partial updates inline; print final on utterance end.
- `jsonl`: emit JSON lines with `type`, `text`, `timestamp`.

## Data Flow
1. CLI parses flags and loads model.
2. Audio stream starts and pushes samples to ring buffer.
3. Chunker pulls samples and optionally applies VAD.
4. Inference runs on chunk, returns partial text.
5. Output formatter prints partial or final results.

## Error Handling
- Missing model: exit code 2 with message.
- No input device: exit code 3 with message.
- Stream errors: log and exit gracefully.

## Validation
- Manual mic test with `sv --model ./models/ggml-tiny.en.bin`.
- Validate partial updates and final segmentation.
- Confirm offline operation by disconnecting network.

## Open Questions
- Best default model (tiny vs base) for CPU speed.
- VAD threshold calibration on typical laptop microphones.
