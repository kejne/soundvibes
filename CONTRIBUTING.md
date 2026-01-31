# Contributing

Thanks for helping improve SoundVibes. This document is for humans.

Note: This repository is developed by agents using beads for issue tracking. Humans do not need to configure beads to contribute.

## Development Setup
### Requirements
- Linux x86_64
- Rust toolchain (stable)
- Microphone input device (for runtime tests)

### Build Dependencies
Install the following to compile sv from source:

**Arch Linux:**
```bash
sudo pacman -Syu base-devel cmake pkgconf rust alsa-lib vulkan-headers \
    vulkan-icd-loader vulkan-validation-layers glslang
```

**Ubuntu / Debian:**
```bash
sudo apt-get update
sudo apt-get install -y build-essential cmake pkg-config libasound2-dev \
    libvulkan-dev vulkan-validationlayers clang
```

**Fedora:**
```bash
sudo dnf install -y cmake pkgconf-pkg-config alsa-lib-devel vulkan-headers \
    vulkan-loader vulkan-validation-layers glslang clang-devel
```

### GPU Drivers (Optional)
Vulkan GPU acceleration is enabled by default. Install GPU drivers for your hardware:

- **Arch Linux:**
  - AMD: `sudo pacman -S vulkan-radeon`
  - NVIDIA: `sudo pacman -S nvidia-utils`
  
- **Ubuntu / Debian:**
  - AMD/Intel: `sudo apt-get install -y mesa-vulkan-drivers`
  - NVIDIA: `sudo apt-get install -y nvidia-driver-<version>`
  
- **Fedora:**
  - AMD/Intel: `sudo dnf install -y mesa-vulkan-drivers`
  - NVIDIA: `sudo dnf install -y akmod-nvidia`

To build without GPU support: `cargo build --no-default-features`

### Clone and Build
```bash
git clone https://github.com/kejne/soundvibes.git
cd soundvibes
cargo build
```

### Model Setup
`sv` downloads the configured whisper.cpp ggml model automatically on first run if it is missing.

## Testing
- Run all tests: `cargo test`
- Acceptance tests: `cargo test --test acceptance`
- Acceptance tests with mocks: `cargo test --test acceptance --features test-support`
- Hardware acceptance tests: `SV_HARDWARE_TESTS=1 cargo test --test acceptance`
- Single test: `cargo test transcribes_sample_audio`
- Integration test: `cargo test --test whisper_integration`

Acceptance criteria live in `docs/acceptance-tests.md` and must map to tests in `tests/acceptance.rs`.

## Linting and Formatting
- Format: `cargo fmt`
- Lint: `cargo clippy --all-targets --all-features`

## Mise Tasks
Common dev tasks are available via mise:

- `mise run prepare-dev` - install build prerequisites for your distro
- `mise run run-local` - run `sv` with a local model
- `mise run ci` - run format, lint, tests, and release build

## Pull Requests
- Prefer small, focused changes.
- Add or update tests for new behavior.
