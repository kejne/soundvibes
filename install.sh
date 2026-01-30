#!/bin/sh
# SoundVibes Install Script
# One-command onboarding for offline speech-to-text on Linux

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default settings
PREFIX="${HOME}/.local"
BIN_DIR="${PREFIX}/bin"
CONFIG_DIR="${XDG_CONFIG_HOME:-${HOME}/.config}/soundvibes"
DATA_DIR="${XDG_DATA_HOME:-${HOME}/.local/share}/soundvibes"
SERVICE_DIR="${HOME}/.config/systemd/user"
REPO="kejne/soundvibes"
INSTALL_DEPS=true
SETUP_SERVICE=true
DOWNLOAD_MODEL=false
AUTO_YES=false
UNINSTALL=false

# Print functions
print_error() {
    printf "${RED}✗ %s${NC}\n" "$1" >&2
}

print_success() {
    printf "${GREEN}✓ %s${NC}\n" "$1"
}

print_info() {
    printf "${BLUE}ℹ %s${NC}\n" "$1"
}

print_warn() {
    printf "${YELLOW}⚠ %s${NC}\n" "$1"
}

print_header() {
    printf "\n${BLUE}%s${NC}\n" "$1"
    printf "${BLUE}%s${NC}\n" "$(echo "$1" | sed 's/./-/g')"
}

# Detect distribution
detect_distro() {
    if [ -f /etc/os-release ]; then
        . /etc/os-release
        echo "$ID"
    elif [ -f /etc/debian_version ]; then
        echo "debian"
    elif [ -f /etc/arch-release ]; then
        echo "arch"
    elif [ -f /etc/fedora-release ]; then
        echo "fedora"
    else
        echo "unknown"
    fi
}

DISTRO=$(detect_distro)

# Check if command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Check if running on Linux x86_64
check_platform() {
    print_header "Checking Platform"
    
    if [ "$(uname -s)" != "Linux" ]; then
        print_error "SoundVibes only supports Linux. Detected: $(uname -s)"
        exit 1
    fi
    
    if [ "$(uname -m)" != "x86_64" ]; then
        print_error "SoundVibes only supports x86_64 architecture. Detected: $(uname -m)"
        exit 1
    fi
    
    print_success "Platform check passed (Linux x86_64)"
}

# Check for existing installation
check_existing() {
    print_header "Checking Existing Installation"
    
    if command_exists sv; then
        SV_PATH=$(command -v sv)
        print_warn "Existing sv binary found at: $SV_PATH"
        
        if [ "$AUTO_YES" = false ]; then
            printf "Overwrite existing installation? [y/N] "
            read -r response </dev/tty
            case "$response" in
                [Yy]*)
                    print_info "Will overwrite existing installation"
                    ;;
                *)
                    print_error "Installation cancelled"
                    exit 1
                    ;;
            esac
        fi
    else
        print_success "No existing installation found"
    fi
}

# Check PATH
ensure_path() {
    print_header "Checking PATH"
    
    case ":${PATH}:" in
        *:"${BIN_DIR}":*)
            print_success "${BIN_DIR} is already in PATH"
            ;;
        *)
            print_warn "${BIN_DIR} is not in your PATH"
            print_info "Adding to shell configuration..."
            
            # Detect shell and add to appropriate config
            if [ -n "$ZSH_VERSION" ] || [ -f "${HOME}/.zshrc" ]; then
                echo 'export PATH="${HOME}/.local/bin:${PATH}"' >> "${HOME}/.zshrc"
                print_success "Added to ~/.zshrc"
                print_info "Run 'source ~/.zshrc' or restart your shell after installation"
            elif [ -n "$BASH_VERSION" ] || [ -f "${HOME}/.bashrc" ]; then
                echo 'export PATH="${HOME}/.local/bin:${PATH}"' >> "${HOME}/.bashrc"
                print_success "Added to ~/.bashrc"
                print_info "Run 'source ~/.bashrc' or restart your shell after installation"
            else
                print_warn "Could not detect shell config file"
                print_info "Please manually add ${BIN_DIR} to your PATH"
            fi
            ;;
    esac
}

# Install system dependencies
install_deps() {
    if [ "$INSTALL_DEPS" = false ]; then
        print_info "Skipping dependency installation (--no-deps flag set)"
        return
    fi
    
    print_header "Installing System Dependencies"
    
    case "$DISTRO" in
        ubuntu|debian)
            print_info "Detected Debian/Ubuntu system"
            sudo apt-get update
            sudo apt-get install -y \
                build-essential \
                cmake \
                pkg-config \
                libasound2-dev \
                libvulkan1 \
                libvulkan-dev \
                vulkan-validationlayers \
                mesa-vulkan-drivers \
                curl \
                unzip
            
            # Install Vulkan SDK for Ubuntu/Debian
            if ! command_exists glslangValidator && [ ! -d "${HOME}/.local/share/vulkan-sdk" ]; then
                print_info "Installing Vulkan SDK..."
                VULKAN_VERSION="1.3.296.0"
                VULKAN_URL="https://sdk.lunarg.com/sdk/download/${VULKAN_VERSION}/linux/vulkansdk-linux-x86_64-${VULKAN_VERSION}.tar.gz"
                
                mkdir -p "${HOME}/.local/share/vulkan-sdk"
                curl -L "$VULKAN_URL" -o "/tmp/vulkan-sdk.tar.gz"
                tar -xzf "/tmp/vulkan-sdk.tar.gz" -C "${HOME}/.local/share/vulkan-sdk"
                rm "/tmp/vulkan-sdk.tar.gz"
                print_success "Vulkan SDK installed to ~/.local/share/vulkan-sdk"
            fi
            ;;
            
        arch|manjaro)
            print_info "Detected Arch Linux system"
            sudo pacman -Syu --needed --noconfirm \
                base-devel \
                cmake \
                pkgconf \
                alsa-lib \
                vulkan-headers \
                vulkan-icd-loader \
                vulkan-validation-layers \
                glslang \
                curl \
                unzip
            ;;
            
        fedora)
            print_info "Detected Fedora system"
            sudo dnf install -y \
                cmake \
                pkgconf-pkg-config \
                alsa-lib-devel \
                vulkan-headers \
                vulkan-loader \
                vulkan-validation-layers \
                mesa-vulkan-drivers \
                glslang \
                curl \
                unzip
            ;;
            
        *)
            print_warn "Unknown distribution: $DISTRO"
            print_info "Please install dependencies manually:"
            print_info "  - cmake, pkg-config, ALSA development libraries"
            print_info "  - Vulkan headers and loader"
            print_info "  - curl, unzip"
            print_info "Refer to your distribution's package manager documentation."
            
            if [ "$AUTO_YES" = false ]; then
                printf "Continue without installing dependencies? [y/N] "
                read -r response </dev/tty
                case "$response" in
                    [Yy]*)
                        print_info "Continuing without dependency installation"
                        ;;
                    *)
                        print_error "Installation cancelled"
                        exit 1
                        ;;
                esac
            fi
            ;;
    esac
    
    print_success "System dependencies installed"
}

# Install text injection tools
install_text_injection() {
    print_header "Text Injection Tools"
    
    if [ -n "$WAYLAND_DISPLAY" ]; then
        print_info "Wayland display server detected"
        
        if command_exists wtype; then
            print_success "wtype is already installed"
        else
            print_info "Installing wtype for Wayland text injection..."
            
            case "$DISTRO" in
                ubuntu|debian)
                    # wtype is in Debian 12+ and Ubuntu 23.04+
                    sudo apt-get install -y wtype || {
                        print_warn "wtype not available in official repositories"
                        print_info "You may need to build from source: https://github.com/atx/wtype"
                    }
                    ;;
                arch|manjaro)
                    sudo pacman -S --needed --noconfirm wtype
                    ;;
                fedora)
                    sudo dnf install -y wtype || {
                        print_warn "wtype may not be available in official repositories"
                        print_info "You may need to build from source: https://github.com/atx/wtype"
                    }
                    ;;
                *)
                    print_warn "Please install wtype manually for Wayland support"
                    print_info "https://github.com/atx/wtype"
                    ;;
            esac
        fi
        
    elif [ -n "$DISPLAY" ]; then
        print_info "X11 display server detected"
        
        if command_exists xdotool; then
            print_success "xdotool is already installed"
        else
            print_info "Installing xdotool for X11 text injection..."
            
            case "$DISTRO" in
                ubuntu|debian)
                    sudo apt-get install -y xdotool
                    ;;
                arch|manjaro)
                    sudo pacman -S --needed --noconfirm xdotool
                    ;;
                fedora)
                    sudo dnf install -y xdotool
                    ;;
                *)
                    print_warn "Please install xdotool manually for X11 support"
                    ;;
            esac
        fi
        
    else
        print_warn "Could not detect display server (neither Wayland nor X11)"
        print_info "Text injection will not work. Install wtype (Wayland) or xdotool (X11) manually."
    fi
}

# Download and install binary
install_binary() {
    print_header "Installing SoundVibes Binary"
    
    # Get latest release URL
    print_info "Fetching latest release information..."
    
    # Fetch release info with better error handling
    API_RESPONSE=$(curl -fsSL --max-time 30 --connect-timeout 10 \
        "https://api.github.com/repos/${REPO}/releases/latest" 2>&1) || {
        CURL_EXIT=$?
        print_error "Failed to contact GitHub API (exit code: $CURL_EXIT)"
        print_info "Error details: $API_RESPONSE"
        print_info "Please check your internet connection"
        print_info "You can also manually download from: https://github.com/${REPO}/releases"
        exit 1
    }
    
    LATEST_URL=$(echo "$API_RESPONSE" | \
        grep -o '"browser_download_url": "[^"]*sv-linux-x86_64\.tar\.gz"' | \
        grep -o 'https://[^"]*' | head -1)
    
    if [ -z "$LATEST_URL" ]; then
        print_error "Could not find download URL in GitHub API response"
        print_info "The release may not have a Linux x86_64 binary yet"
        print_info "You can manually download from: https://github.com/${REPO}/releases"
        exit 1
    fi
    
    print_info "Downloading from: $LATEST_URL"
    
    # Create temp directory
    TMP_DIR=$(mktemp -d)
    trap "rm -rf $TMP_DIR" EXIT
    
    # Download with progress and timeout
    print_info "Downloading binary (this may take a moment)..."
    curl -fSL --max-time 120 --connect-timeout 30 \
        --progress-bar \
        -o "${TMP_DIR}/soundvibes.tar.gz" "$LATEST_URL" || {
        print_error "Failed to download SoundVibes"
        print_info "You can manually download from: $LATEST_URL"
        exit 1
    }
    
    # Extract
    tar -xzf "${TMP_DIR}/soundvibes.tar.gz" -C "$TMP_DIR" || {
        print_error "Failed to extract archive"
        exit 1
    }
    
     # Install binary
     mkdir -p "$BIN_DIR"
     cp "${TMP_DIR}/sv-linux-x86_64" "$BIN_DIR/sv"
     chmod +x "$BIN_DIR/sv"
    
    print_success "Binary installed to ${BIN_DIR}/sv"
}

# Create configuration
create_config() {
    print_header "Creating Configuration"
    
    mkdir -p "$CONFIG_DIR"
    
    CONFIG_FILE="${CONFIG_DIR}/config.toml"
    
    if [ -f "$CONFIG_FILE" ]; then
        print_warn "Configuration file already exists at ${CONFIG_FILE}"
        print_info "Keeping existing configuration"
    else
        cat > "$CONFIG_FILE" << 'EOF'
# SoundVibes Configuration File
# Documentation: https://github.com/kejne/soundvibes#configuration

# Model settings
model_size = "small"        # Options: tiny, base, small, medium, large, auto
model_language = "auto"     # Options: auto, en
download_model = true       # Auto-download missing models

# Audio settings
device = "default"          # Audio input device name
audio_host = "alsa"         # Options: default, alsa
sample_rate = 16000         # Hz

# Output settings
format = "plain"            # Options: plain, jsonl
mode = "stdout"             # Options: stdout, inject
language = "en"             # Transcription language

# VAD (Voice Activity Detection) settings
vad = "on"                  # Options: on, off
vad_silence_ms = 1200       # Silence timeout in milliseconds
vad_threshold = 0.01        # Energy threshold
vad_chunk_ms = 100          # Chunk size in milliseconds

# Debug settings (set to true for troubleshooting)
debug_audio = false
debug_vad = false
dump_audio = false          # Save audio to WAV files for debugging
EOF
        
        print_success "Configuration created at ${CONFIG_FILE}"
    fi
}

# Setup systemd service
setup_systemd() {
    if [ "$SETUP_SERVICE" = false ]; then
        print_info "Skipping systemd service setup (--no-service flag set)"
        return
    fi
    
    if ! command_exists systemctl; then
        print_warn "systemctl not found. Skipping systemd service setup."
        return
    fi
    
    print_header "Systemd Service Setup"
    
    if [ "$AUTO_YES" = false ]; then
        printf "Set up systemd user service for automatic startup? [Y/n] "
        read -r response </dev/tty
        case "$response" in
            [Nn]*)
                print_info "Skipping systemd service setup"
                print_info "To manually set up later, create: ${SERVICE_DIR}/sv.service"
                return
                ;;
        esac
    fi
    
    mkdir -p "$SERVICE_DIR"
    
    cat > "${SERVICE_DIR}/sv.service" << EOF
[Unit]
Description=SoundVibes daemon
After=sound.target

[Service]
Type=simple
ExecStart=${BIN_DIR}/sv daemon start
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
EOF
    
    # Reload systemd
    systemctl --user daemon-reload
    
    # Enable but don't start yet
    systemctl --user enable sv.service
    
    print_success "Systemd service created and enabled"
    print_info "Start the service with: systemctl --user start sv.service"
    print_info "Or simply run: sv daemon start"
}

# Pre-download model
predownload_model() {
    if [ "$DOWNLOAD_MODEL" = false ]; then
        return
    fi
    
    print_header "Pre-downloading Model"
    
    if [ "$AUTO_YES" = false ]; then
        printf "Pre-download whisper model (~500MB)? This avoids delay on first use. [y/N] "
        read -r response </dev/tty
        case "$response" in
            [Yy]*)
                print_info "Will pre-download model"
                ;;
            *)
                print_info "Skipping model pre-download"
                return
                ;;
        esac
    fi
    
    print_info "Starting daemon to trigger model download..."
    
    # Start daemon briefly to trigger model download
    if sv daemon start &
    then
        DAEMON_PID=$!
        sleep 5
        
        # Stop daemon
        sv daemon stop 2>/dev/null || true
        wait $DAEMON_PID 2>/dev/null || true
        
        print_success "Model download initiated"
        print_info "Models are stored in: ${DATA_DIR}/models/"
    else
        print_warn "Could not start daemon for model download"
        print_info "The model will be downloaded automatically on first use"
    fi
}

# Uninstall function
uninstall() {
    print_header "Uninstalling SoundVibes"
    
    # Stop and disable service
    if command_exists systemctl && [ -f "${SERVICE_DIR}/sv.service" ]; then
        print_info "Stopping systemd service..."
        systemctl --user stop sv.service 2>/dev/null || true
        systemctl --user disable sv.service 2>/dev/null || true
        rm -f "${SERVICE_DIR}/sv.service"
        systemctl --user daemon-reload
        print_success "Systemd service removed"
    fi
    
    # Remove binary
    if [ -f "${BIN_DIR}/sv" ]; then
        rm -f "${BIN_DIR}/sv"
        print_success "Binary removed from ${BIN_DIR}/sv"
    fi
    
    # Ask about config and data
    if [ "$AUTO_YES" = false ]; then
        printf "Remove configuration and downloaded models? [y/N] "
        read -r response </dev/tty
        case "$response" in
            [Yy]*)
                rm -rf "$CONFIG_DIR"
                rm -rf "$DATA_DIR"
                print_success "Configuration and models removed"
                ;;
            *)
                print_info "Keeping configuration at: ${CONFIG_DIR}"
                print_info "Keeping models at: ${DATA_DIR}"
                ;;
        esac
    fi
    
    print_success "SoundVibes has been uninstalled"
}

# Print post-install summary
print_summary() {
    print_header "Installation Complete!"
    
    printf "\n${GREEN}SoundVibes is ready to use!${NC}\n\n"
    
    printf "${BLUE}Quick Start:${NC}\n"
    printf "  1. Start the daemon: ${GREEN}sv daemon start${NC}\n"
    printf "  2. Toggle recording: ${GREEN}sv${NC}\n"
    printf "  3. Stop the daemon:  ${GREEN}sv daemon stop${NC}\n\n"
    
    printf "${BLUE}Files:${NC}\n"
    printf "  Binary:     ${GREEN}${BIN_DIR}/sv${NC}\n"
    printf "  Config:     ${GREEN}${CONFIG_DIR}/config.toml${NC}\n"
    printf "  Models:     ${GREEN}${DATA_DIR}/models/${NC}\n\n"
    
    if [ -f "${SERVICE_DIR}/sv.service" ]; then
        printf "${BLUE}Systemd Service:${NC}\n"
        printf "  Start:   ${GREEN}systemctl --user start sv.service${NC}\n"
        printf "  Enable:  ${GREEN}systemctl --user enable sv.service${NC}\n"
        printf "  Status:  ${GREEN}systemctl --user status sv.service${NC}\n\n"
    fi
    
    printf "${BLUE}Window Manager Integration:${NC}\n"
    printf "  i3/sway: ${YELLOW}bindsym $mod+Shift+v exec sv${NC}\n"
    printf "  Hyprland: ${YELLOW}bind = SUPER, V, exec, sv${NC}\n\n"
    
    printf "${BLUE}Documentation:${NC} https://github.com/kejne/soundvibes\n"
    printf "${BLUE}Issues:${NC} https://github.com/kejne/soundvibes/issues\n\n"
    
    if [ "${PATH#*${BIN_DIR}}" = "${PATH}" ]; then
        print_warn "${BIN_DIR} is not in your current PATH"
        print_info "Restart your shell or run: source ~/.bashrc (or ~/.zshrc)"
    fi
}

# Show help
show_help() {
    cat << 'EOF'
SoundVibes Install Script

Usage: ./install.sh [OPTIONS]

Options:
  --prefix=PATH          Install prefix (default: ~/.local)
  --bin-dir=PATH         Binary directory (default: ~/.local/bin)
  --no-deps              Skip dependency installation
  --no-service           Skip systemd service setup
  --download-model       Pre-download whisper model
  --yes                  Auto-answer yes to all prompts (non-interactive)
  --uninstall            Remove SoundVibes installation
  --help                 Show this help message

Examples:
  ./install.sh                    # Interactive installation
  ./install.sh --yes              # Non-interactive installation
  ./install.sh --no-deps          # Skip system dependencies
  ./install.sh --uninstall        # Remove installation

For more information, visit: https://github.com/kejne/soundvibes
EOF
}

# Parse arguments
while [ $# -gt 0 ]; do
    case "$1" in
        --prefix=*)
            PREFIX="${1#*=}"
            BIN_DIR="${PREFIX}/bin"
            ;;
        --bin-dir=*)
            BIN_DIR="${1#*=}"
            ;;
        --no-deps)
            INSTALL_DEPS=false
            ;;
        --no-service)
            SETUP_SERVICE=false
            ;;
        --download-model)
            DOWNLOAD_MODEL=true
            ;;
        --yes)
            AUTO_YES=true
            ;;
        --uninstall)
            UNINSTALL=true
            ;;
        --help)
            show_help
            exit 0
            ;;
        *)
            print_error "Unknown option: $1"
            show_help
            exit 1
            ;;
    esac
    shift
done

# Main
main() {
    printf "${BLUE}"
    cat << 'EOF'
  _________                        ._______   ____._____.                  
 /   _____/ ____  __ __  ____    __| _/\   \ /   /|__\_ |__   ____   ______
 \_____  \ /  _ \|  |  \/    \  / __ |  \   Y   / |  || __ \_/ __ \ /  ___/
 /        (  <_> )  |  /   |  \/ /_/ |   \     /  |  || \_\ \  ___/ \___ \ 
/_______  /\____/|____/|___|  /\____ |    \___/   |__||___  /\___  >____  >
        \/                  \/      \/                    \/     \/     \/ 
EOF
    printf "${NC}\n"
    printf "${BLUE}Offline Speech-to-Text for Linux${NC}\n\n"
    
    if [ "$UNINSTALL" = true ]; then
        uninstall
        exit 0
    fi
    
    check_platform
    check_existing
    ensure_path
    install_deps
    install_text_injection
    install_binary
    create_config
    setup_systemd
    predownload_model
    print_summary
}

main "$@"
