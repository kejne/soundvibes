# SoundVibes (sv)

Offline voice-to-text CLI for Linux.

## Overview
`sv` captures audio from your microphone using start/stop toggles and runs offline speech-to-text with a small whisper.cpp model. It aims for minimal runtime dependencies and ships as a single binary plus a local model file.

## Quick Start

### 1) One-Line Install (Recommended)

```bash
curl -fsSL https://raw.githubusercontent.com/kejne/soundvibes/main/install.sh | sh
```

Or download and run with options:

```bash
curl -fsSL -o install.sh https://raw.githubusercontent.com/kejne/soundvibes/main/install.sh
chmod +x install.sh
./install.sh
```

The install script handles everything: dependencies, binary installation, configuration, and optional systemd service setup.

See [docs/installation.md](docs/installation.md) for detailed options and manual installation steps.

### 2) Requirements
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

### 3) Manual Install from GitHub Releases

If you prefer manual installation, download the latest Linux release from:

https://github.com/kejne/soundvibes/releases

Example (replace the version with the latest):

```bash
curl -L -o soundvibes.tar.gz https://github.com/kejne/soundvibes/releases/download/v0.1.0/soundvibes-linux-x86_64.tar.gz
tar -xzf soundvibes.tar.gz
mkdir -p "$HOME/.local/bin"
mv sv "$HOME/.local/bin/sv"
```

Make sure `~/.local/bin` is on your `PATH`.

### 4) Configure
Create a config file at `${XDG_CONFIG_HOME:-~/.config}/soundvibes/config.toml`.

```toml
model = "/home/you/.local/share/soundvibes/models/ggml-base.en.bin"
model_size = "small"
model_language = "auto"
download_model = true
language = "auto"
device = "default"
sample_rate = 16000
format = "plain"
vad = false
mode = "stdout"
```

If `model` is omitted, `sv` builds a default model path under
`${XDG_DATA_HOME:-~/.local/share}/soundvibes/models/` based on `model_size` and
`model_language` (defaults to the small general model).

`sv` downloads the model automatically on first run if it is missing.

### 4) Run
Start the daemon:

```bash
sv daemon start
```

In another terminal, trigger a capture:

```bash
sv
```

Update the model while the daemon is running:

```bash
sv daemon set-model --size small --model-language en
```

Stop the daemon:

```bash
sv daemon stop
```

## Environment Setup Tips
- i3: add a keybinding to run `sv`, and let a user systemd service or `exec --no-startup-id sv daemon start` keep the daemon alive.
- Hyprland: use `exec-once = sv daemon start` in `hyprland.conf`, plus `bind = SUPER, V, exec, sv` for capture.
- GNOME: add a custom keyboard shortcut (Settings -> Keyboard -> View and Customize Shortcuts) with command `sv`, and use Startup Applications or a user systemd service for the daemon.

## Daemon Lifecycle
Run `sv daemon start` in the foreground for quick tests, or use a user systemd service
to keep it running across sessions.

Example user unit (`~/.config/systemd/user/sv.service`):

```ini
[Unit]
Description=SoundVibes daemon

[Service]
ExecStart=%h/.local/bin/sv daemon start
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
- PRD: `docs/prd.md`
- Technical design: `docs/technical-design.md`
- Acceptance tests: `docs/acceptance-tests.md`

## Contributing
See `CONTRIBUTING.md` for development setup, tests, and workflow.
