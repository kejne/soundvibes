# Contributing

Thanks for helping improve SoundVibes. This document is for humans.

Note: This repository is developed by agents using beads for issue tracking. Humans do not need to configure beads to contribute.

## Development Setup
### Requirements
- Linux x86_64
- Rust toolchain (stable)
- Microphone input device (for runtime tests)

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
