# Installation Guide

SoundVibes provides a one-command install script that handles everything from dependencies to configuration.

## Quick Install

```bash
curl -fsSL https://raw.githubusercontent.com/kejne/soundvibes/main/install.sh | sh
```

Or download and run manually:

```bash
curl -fsSL -o install.sh https://raw.githubusercontent.com/kejne/soundvibes/main/install.sh
chmod +x install.sh
./install.sh
```

## What the Install Script Does

The install script automates the entire onboarding process:

1. **Platform Check** - Verifies Linux x86_64
2. **Dependency Installation** - Installs required system packages for your distro
3. **Binary Installation** - Downloads and installs the latest release from GitHub
4. **Configuration** - Creates default config at `~/.config/soundvibes/config.toml`
5. **Text Injection Tools** - Auto-detects Wayland/X11 and installs `wtype` or `xdotool`
6. **Systemd Service** - Optionally sets up auto-start service
7. **Model Download** - Optionally pre-downloads the whisper model

## Supported Distributions

- **Ubuntu** 22.04, 24.04, and newer
- **Debian** 12 and newer
- **Arch Linux** / Manjaro
- **Fedora** 38 and newer

## Install Options

### Non-Interactive Installation

For automated setups (CI, dotfiles, etc.):

```bash
./install.sh --yes
```

This answers "yes" to all prompts without user interaction.

### Skip Dependencies

If you prefer to manage dependencies manually:

```bash
./install.sh --no-deps
```

### Skip Systemd Service

If you don't want the systemd service:

```bash
./install.sh --no-service
```

### Pre-download Model

Avoid the delay on first use by downloading the model during installation:

```bash
./install.sh --download-model
```

### Custom Installation Path

Install to a custom location:

```bash
./install.sh --prefix=/usr/local
```

Or just the binary:

```bash
./install.sh --bin-dir=/usr/local/bin
```

## Manual Installation

If you prefer to install manually:

### 1. Install Dependencies

**Ubuntu/Debian:**
```bash
sudo apt-get update
sudo apt-get install -y build-essential cmake pkg-config libasound2-dev \
    libvulkan1 libvulkan-dev vulkan-validation-layers mesa-vulkan-drivers \
    curl unzip
```

**Arch Linux:**
```bash
sudo pacman -Syu --needed base-devel cmake pkgconf alsa-lib \
    vulkan-headers vulkan-icd-loader vulkan-validation-layers \
    glslang curl unzip
```

**Fedora:**
```bash
sudo dnf install -y cmake pkgconf-pkg-config alsa-lib-devel \
    vulkan-headers vulkan-loader vulkan-validation-layers \
    mesa-vulkan-drivers glslang curl unzip
```

### 2. Download Binary

```bash
# Download latest release
curl -L -o soundvibes.tar.gz \
    https://github.com/kejne/soundvibes/releases/latest/download/soundvibes-linux-x86_64.tar.gz

# Extract
tar -xzf soundvibes.tar.gz

# Install to ~/.local/bin
mkdir -p ~/.local/bin
mv sv ~/.local/bin/
chmod +x ~/.local/bin/sv

# Clean up
rm soundvibes.tar.gz
```

### 3. Add to PATH

Ensure `~/.local/bin` is in your PATH. Add to your shell config:

**Bash:**
```bash
echo 'export PATH="${HOME}/.local/bin:${PATH}"' >> ~/.bashrc
```

**Zsh:**
```bash
echo 'export PATH="${HOME}/.local/bin:${PATH}"' >> ~/.zshrc
```

### 4. Create Configuration

```bash
mkdir -p ~/.config/soundvibes
cat > ~/.config/soundvibes/config.toml << 'EOF'
model_size = "small"
model_language = "auto"
download_model = true
device = "default"
audio_host = "alsa"
sample_rate = 16000
format = "plain"
mode = "stdout"
language = "en"
vad = "on"
vad_silence_ms = 1200
vad_threshold = 0.01
vad_chunk_ms = 100
debug_audio = false
debug_vad = false
dump_audio = false
EOF
```

### 5. Install Text Injection Tools

**Wayland:**
```bash
# Ubuntu/Debian (23.04+)
sudo apt-get install wtype

# Arch
sudo pacman -S wtype
```

**X11:**
```bash
# Ubuntu/Debian
sudo apt-get install xdotool

# Arch
sudo pacman -S xdotool
```

## Uninstall

Remove SoundVibes completely:

```bash
./install.sh --uninstall
```

This will:
- Stop and disable the systemd service
- Remove the binary
- Optionally remove configuration and models

## Post-Installation

### Start the Daemon

```bash
sv daemon start
```

Or use systemd (if enabled during install):

```bash
systemctl --user start sv.service
```

### Toggle Recording

```bash
sv
```

### Window Manager Integration

**i3/sway:**
```bash
# Add to ~/.config/i3/config or ~/.config/sway/config
exec --no-startup-id sv daemon start
bindsym $mod+Shift+v exec sv
```

**Hyprland:**
```bash
# Add to ~/.config/hypr/hyprland.conf
exec-once = sv daemon start
bind = SUPER, V, exec, sv
```

**GNOME:**
1. Set up custom keyboard shortcut in Settings → Keyboard → Custom Shortcuts
2. Add `sv` as the command
3. Add to Startup Applications for auto-start

## Troubleshooting

### Binary not found after installation

Restart your shell or source your config:

```bash
source ~/.bashrc  # or ~/.zshrc
```

### Missing dependencies

Run the install script with dependency installation:

```bash
./install.sh --yes
```

Or manually install for your distribution (see Manual Installation section).

### Permission denied

Ensure the binary is executable:

```bash
chmod +x ~/.local/bin/sv
```

### Model download fails

Models are downloaded on first run. If it fails:

1. Check internet connection
2. Verify write permissions to `~/.local/share/soundvibes/models/`
3. Try running with `sv daemon start` manually to see errors

## Building from Source

If you prefer to build from source instead of using pre-built binaries:

```bash
git clone https://github.com/kejne/soundvibes.git
cd soundvibes

# Install build dependencies (Ubuntu/Debian example)
mise run prepare-dev  # or install manually

# Build
cargo build --release

# Install binary
cp target/release/sv ~/.local/bin/
```

See the [README](../README.md) for more details on building from source.

## Getting Help

- **Documentation**: https://github.com/kejne/soundvibes
- **Issues**: https://github.com/kejne/soundvibes/issues
- **Discussions**: https://github.com/kejne/soundvibes/discussions
