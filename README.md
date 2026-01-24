# SoundVibes (sv)

Offline voice-to-text CLI for Linux.

## Overview
`sv` captures audio from your microphone using push-to-talk and runs offline speech-to-text with a small whisper.cpp model. It aims for minimal runtime dependencies and ships as a single binary plus a local model file.

## Requirements
- Linux x86_64
- Microphone input device

## Model Setup
Download a whisper.cpp ggml model to the XDG data directory.

Example (base English model):

```bash
data_dir="${XDG_DATA_HOME:-$HOME/.local/share}/soundvibes/models"
mkdir -p "$data_dir"
curl -L -o "$data_dir/ggml-base.en.bin" https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin
```

Or use the mise task:

```bash
mise run download-model
```

Pick a size via `SIZE` (English models only):

```bash
SIZE=small mise run download-model
```

Available sizes: `tiny`, `base`, `small`, `medium`, `large`.

## Configuration
Create a config file at `${XDG_CONFIG_HOME:-~/.config}/soundvibes/config.toml`.

```toml
model = "/home/you/.local/share/soundvibes/models/ggml-base.en.bin"
language = "auto"
device = "default"
sample_rate = 16000
format = "plain"
hotkey = "ctrl+`"
vad = false
```

If `model` is omitted, `sv` defaults to `${XDG_DATA_HOME:-~/.local/share}/soundvibes/models/ggml-base.en.bin`.

### Hotkey syntax
- Use a `+`-separated combo with optional modifiers (`ctrl`, `alt`, `shift`, `super`) and a single key.
- Modifiers are case-insensitive and can be combined (example: `ctrl+shift+space`).
- For the super modifier, use `super`, `meta`, `win`, or `cmd`.
- Supported keys include letters (`a`-`z`), digits (`0`-`9`), `space`, `tab`, `enter`, `esc`, function keys (`f1`-`f12`), and the backtick key.
- Special characters must be quoted in TOML, so wrap combos like `ctrl+`` in double quotes.
- For the backtick key, use `` ` `` in the combo (example: `hotkey = "ctrl+`"`).
- Literal symbol keys are matched by character, so use the exact printable character in the combo.

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
