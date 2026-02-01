<div align="center">
  <img src="docs/assets/soundvibes.png" alt="SoundVibes Logo" width="200">
  <h1>SoundVibes (sv)</h1>
  <p>Open source voice-to-text for Linux</p>
</div>

## Overview

SoundVibes (sv) is an offline speech-to-text tool for Linux. It captures audio from your microphone using start/stop toggles and transcribes locally using whisper.cpp. No cloud, no latency, no subscriptions.

## Quick Start

### 1. Install

```bash
curl -fsSL https://raw.githubusercontent.com/kejne/soundvibes/main/install.sh | sh
```

Or download manually from [GitHub Releases](https://github.com/kejne/soundvibes/releases).

### 2. Start the Daemon

```bash
sv daemon start
```

### 3. Toggle Recording

```bash
sv
```

Bind the toggle command to a hotkey in your desktop environment for hands-free use.

## Documentation

- **Website**: [https://soundvibes.teashaped.dev](https://soundvibes.teashaped.dev) - Full documentation with installation guide, configuration reference, and troubleshooting
- **Contributing**: See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup and build instructions

## Quick Tips

**Desktop Environment Setup:**
- **i3/Sway**: `bindsym $mod+Shift+v exec sv`
- **Hyprland**: `bind = SUPER, V, exec, sv`
- **GNOME/KDE**: Add custom keyboard shortcut with command `sv`

**Systemd Service:**
```bash
systemctl --user enable --now sv.service
```

## Requirements

- Linux x86_64
- Microphone input device
- Optional: Vulkan for GPU acceleration
- Optional: `wtype` (Wayland) or `xdotool` (X11) for text injection

See the [website](https://soundvibes.teashaped.dev) for detailed requirements and configuration options.

## License

This project is licensed under the GNU General Public License v3.0 - see the [LICENSE](LICENSE) file for details.
