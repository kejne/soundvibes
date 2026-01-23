# SoundVibes (sv)

Offline voice-to-text CLI for Linux.

## Overview
`sv` captures audio from your microphone and streams offline speech-to-text using a small whisper.cpp model. It aims for minimal runtime dependencies and ships as a single binary plus a local model file.

## Requirements
- Linux x86_64
- Microphone input device

## Model Setup
Download a small whisper.cpp ggml model and place it in `./models`.

Example (tiny English model):

```bash
mkdir -p models
curl -L -o models/ggml-tiny.en.bin https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin
```

## Usage
```bash
sv --model ./models/ggml-tiny.en.bin
```

## Output Formats
- `plain` (default): prints partial updates and final transcripts.
- `jsonl`: emits JSON lines with `type`, `text`, `timestamp`.

## Documentation
- PRD: `docs/mvp/prd-stt-cli.md`
- Technical design: `docs/mvp/technical-design-stt-cli.md`
- Acceptance tests: `docs/mvp/acceptance-tests.md`
