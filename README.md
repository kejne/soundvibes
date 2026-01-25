# SoundVibes (sv)

Offline voice-to-text CLI for Linux.

## Overview
`sv` captures audio from your microphone using start/stop toggles and runs offline speech-to-text with a small whisper.cpp model. It aims for minimal runtime dependencies and ships as a single binary plus a local model file.

## Requirements
- Linux x86_64
- Microphone input device

Vulkan GPU acceleration is enabled by default. Install the Vulkan loader + headers for your distro,
or build CPU-only with `cargo build --no-default-features`.

- Arch Linux:
  - `sudo pacman -Syu vulkan-headers vulkan-icd-loader vulkan-validation-layers`
  - GPU ICD: `sudo pacman -S vulkan-radeon` (AMD) or `sudo pacman -S nvidia-utils` (NVIDIA)
- Ubuntu / Debian:
  - `sudo apt-get update && sudo apt-get install -y libvulkan-dev vulkan-validationlayers`
  - GPU ICD: `sudo apt-get install -y mesa-vulkan-drivers` (AMD/Intel) or `sudo apt-get install -y nvidia-driver-<version>`
- Fedora:
  - `sudo dnf install -y vulkan-headers vulkan-loader vulkan-validation-layers`
  - GPU ICD: `sudo dnf install -y mesa-vulkan-drivers` (AMD/Intel) or `sudo dnf install -y akmod-nvidia`

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
vad = false
mode = "inject"
```

If `model` is omitted, `sv` defaults to `${XDG_DATA_HOME:-~/.local/share}/soundvibes/models/ggml-base.en.bin`.

## Usage
```bash
sv --daemon
```

In another terminal:

```bash
sv
```

## Daemon Lifecycle
Run `sv --daemon` in the foreground for quick tests, or use a user systemd service
to keep it running across sessions.

Example user unit (`~/.config/systemd/user/sv.service`):

```ini
[Unit]
Description=SoundVibes daemon

[Service]
ExecStart=%h/.cargo/bin/sv --daemon
Restart=on-failure

[Install]
WantedBy=default.target
```

Enable and start:

```bash
systemctl --user daemon-reload
systemctl --user enable --now sv.service
```

Stop it cleanly:

```bash
systemctl --user stop sv.service
```

## Output Formats
- `plain` (default): prints the final transcript after capture stops.
- `jsonl`: emits JSON lines with `type`, `text`, `timestamp`.

## Text Injection
Set `mode = "inject"` in the config to inject text at the focused cursor.

- Wayland: install `wtype` (virtual keyboard).
- X11: install `xdotool` (XTest).

`sv` tries Wayland injection first, then X11, and falls back to stdout with a warning.

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
