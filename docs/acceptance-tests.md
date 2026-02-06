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
- Command: `sv daemon start`
- Expect: process starts, listens on socket, no error output.
- Pass: exit code is `0` after user stops the process.

### AT-01a: Missing model is auto-downloaded
- Setup: remove `${XDG_DATA_HOME:-~/.local/share}/soundvibes/models/ggml-small.bin`, set `model_size` to `small` and `model_language` to `auto`.
- Command: `sv daemon start`
- Expect: model download occurs before startup completes.
- Pass: model file exists at the default location and daemon starts.

### AT-01b: Language selects model variant
- Setup: set `language = "en"` without `model_language`.
- Command: `sv daemon start`
- Expect: model download uses the `.en` variant.
- Pass: model file path resolves to `ggml-<size>.en.bin` when language is `en` and `model_language` is unset.

### AT-02: Missing model returns error
- Setup: set `model` in config to `${XDG_DATA_HOME:-~/.local/share}/soundvibes/models/missing.bin` and set `download_model = false`.
- Command: `sv daemon start`
- Expect: error message indicating missing model.
- Pass: exit code is `2`.

### AT-03: Invalid input device
- Setup: set `device` in config to `"nonexistent"`.
- Command: `sv daemon start`
- Expect: error message indicating device not found.
- Pass: exit code is `3`.

### AT-04: Daemon toggle capture
- Setup: set `model` in config to `${XDG_DATA_HOME:-~/.local/share}/soundvibes/models/ggml-base.en.bin`.
- Command: `sv daemon start` in one terminal, `sv` to toggle on, then `sv` to toggle off.
- Action: speak a short sentence while capture is toggled on.
- Expect: final transcript is printed after toggling off.
- Pass: final output appears shortly after toggle off.

### AT-05: JSONL output format
- Setup: set `format` in config to `"jsonl"`.
- Command: `sv daemon start` in one terminal, `sv` to toggle on, then `sv` to toggle off.
- Action: speak a short sentence while capture is toggled on.
- Expect: output lines are valid JSON with `type`, `text`, `timestamp`.
- Pass: JSONL lines parse and include required fields.

### AT-06: Offline operation
- Setup: set `model` in config to `${XDG_DATA_HOME:-~/.local/share}/soundvibes/models/ggml-base.en.bin`.
- Command: disconnect network, run `sv daemon start`, then `sv` to toggle on/off.
- Expect: no network access required.
- Pass: transcription works without network connectivity.

### AT-07: GPU auto-select and CPU fallback
- Setup: run on a machine with a supported NVIDIA/AMD GPU and another machine without GPU support.
- Command: `sv daemon start`.
- Expect: GPU machine logs show a GPU backend selected; CPU-only machine logs show fallback to CPU.
- Pass: transcription succeeds on both, and no manual GPU selection is required.

### AT-08: Release artifacts published
- Setup: create a GitHub Release with a tag.
- Command: `gh release view <tag> --json assets`.
- Expect: assets include the Linux x86_64 tarball and corresponding SHA256 checksum file.
- Pass: assets are downloadable and checksum matches the tarball contents.

### AT-09: PR quality gates mirror local checks
- Setup: open a pull request targeting `main`.
- Command: `mise run ci` locally and the CI workflow for the PR.
- Expect: the same set of checks run in both environments.
- Pass: both local and CI runs complete successfully with matching steps.

### AT-10: Marketing site build and smoke test
- Setup: ensure Node.js and npm are installed, export `SV_WEB_TESTS=1`.
- Command: `cargo test --test acceptance -- at10_marketing_site_builds_and_smoke_test`.
- Expect: `web/` dependencies install, Astro builds, and the UI smoke test passes.
- Pass: the acceptance test exits 0.

### AT-11: Systemd service starts in graphical session
- Setup: create a temp HOME with an existing `config.toml`, and run installer with mocked `curl`, `tar`, and `systemctl` to avoid network/system modifications.
- Command: `cargo test --test acceptance -- at11_installer_is_idempotent_and_preserves_config`.
- Expect: running installer twice succeeds, generated service unit uses `After=graphical-session.target` and `WantedBy=graphical-session.target`, and existing config content is unchanged.
- Pass: test exits 0 with preserved config and graphical-session-targeted unit.
