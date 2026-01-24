# SoundVibes (sv)

Offline voice-to-text CLI for Linux.

## Overview
`sv` captures audio from your microphone using push-to-talk and runs offline speech-to-text with a small whisper.cpp model. It aims for minimal runtime dependencies and ships as a single binary plus a local model file.

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

## Configuration
Create a config file at `${XDG_CONFIG_HOME:-~/.config}/soundvibes/config.toml`.

```toml
model = "/home/you/soundvibes/models/ggml-tiny.en.bin"
language = "auto"
device = "default"
sample_rate = 16000
format = "plain"
hotkey = "ctrl+`"
vad = false
```

## Usage
```bash
sv
```

## Output Formats
- `plain` (default): prints the final transcript after key release.
- `jsonl`: emits JSON lines with `type`, `text`, `timestamp`.

## Documentation
- PRD: `docs/mvp/prd-stt-cli.md`
- Technical design: `docs/mvp/technical-design-stt-cli.md`
- Acceptance tests: `docs/mvp/acceptance-tests.md`

## Validation
These steps align with `docs/mvp/acceptance-tests.md`.

1. Ensure the model is downloaded (see Model Setup).
2. Create a valid config file (see Configuration).
3. Start the CLI:
   ```bash
   sv
   ```
4. Verify the missing-model behavior:
   ```bash
   sv
   ```
   Update `model` in the config to the missing path first.
5. Run the remaining acceptance checks (device errors, JSONL output, offline mode)
   as listed in `docs/mvp/acceptance-tests.md`.
