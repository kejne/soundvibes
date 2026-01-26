# Acceptance Tests: Soundvibes Offline Voice-to-Text CLI

These tests validate the product behavior for the offline Linux CLI.

## Environment
- Linux x86_64 machine with a working microphone.
- Model file available at `${XDG_DATA_HOME:-~/.local/share}/soundvibes/models/ggml-base.en.bin`.
- Config file at `${XDG_CONFIG_HOME:-~/.config}/soundvibes/config.toml`.
- No network required.
- If available, a machine with a supported NVIDIA/AMD GPU for GPU-acceleration checks.

## Automation notes
- Harness helpers live under `sv::daemon::test_support` and require `cargo test --features test-support`.
- Hardware-dependent tests should be guarded with opt-in env vars:
  - `SV_MODEL_PATH` to point at a local model file for transcription tests.
  - `SV_HARDWARE_TESTS=1` to opt into microphone/GPU checks.
- Automated acceptance tests live in `tests/acceptance.rs` and should map to the AT-xx entries below.
- Run automated acceptance tests with `cargo test --test acceptance` (add `--features test-support` when using mocks).

## Tests

### AT-01: CLI starts with valid model
- Setup: set `model` in config to `${XDG_DATA_HOME:-~/.local/share}/soundvibes/models/ggml-base.en.bin`.
- Command: `sv --daemon`
- Expect: process starts, listens on socket, no error output.
- Pass: exit code is `0` after user stops the process.

### AT-01a: Missing model is auto-downloaded
- Setup: remove `${XDG_DATA_HOME:-~/.local/share}/soundvibes/models/ggml-small.bin`, set `model_size` to `small` and `model_language` to `auto`.
- Command: `sv --daemon`
- Expect: model download occurs before startup completes.
- Pass: model file exists at the default location and daemon starts.

### AT-02: Missing model returns error
- Setup: set `model` in config to `${XDG_DATA_HOME:-~/.local/share}/soundvibes/models/missing.bin` and set `download_model = false`.
- Command: `sv --daemon`
- Expect: error message indicating missing model.
- Pass: exit code is `2`.

### AT-03: Invalid input device
- Setup: set `device` in config to `"nonexistent"`.
- Command: `sv --daemon`
- Expect: error message indicating device not found.
- Pass: exit code is `3`.

### AT-04: Daemon toggle capture
- Setup: set `model` in config to `${XDG_DATA_HOME:-~/.local/share}/soundvibes/models/ggml-base.en.bin`.
- Command: `sv --daemon` in one terminal, `sv` to toggle on, then `sv` to toggle off.
- Action: speak a short sentence while capture is toggled on.
- Expect: final transcript is printed after toggling off.
- Pass: final output appears shortly after toggle off.

### AT-05: JSONL output format
- Setup: set `format` in config to `"jsonl"`.
- Command: `sv --daemon` in one terminal, `sv` to toggle on, then `sv` to toggle off.
- Action: speak a short sentence while capture is toggled on.
- Expect: output lines are valid JSON with `type`, `text`, `timestamp`.
- Pass: JSONL lines parse and include required fields.

### AT-06: Offline operation
- Setup: set `model` in config to `${XDG_DATA_HOME:-~/.local/share}/soundvibes/models/ggml-base.en.bin`.
- Command: disconnect network, run `sv --daemon`, then `sv` to toggle on/off.
- Expect: no network access required.
- Pass: transcription works without network connectivity.

### AT-07: GPU auto-select and CPU fallback
- Setup: run on a machine with a supported NVIDIA/AMD GPU and another machine without GPU support.
- Command: `sv --daemon`.
- Expect: GPU machine logs show a GPU backend selected; CPU-only machine logs show fallback to CPU.
- Pass: transcription succeeds on both, and no manual GPU selection is required.
