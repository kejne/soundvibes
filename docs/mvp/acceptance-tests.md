# Acceptance Tests: Offline STT CLI (sv)

These tests validate the MVP behavior for the offline Linux CLI.

## Environment
- Linux x86_64 machine with a working microphone.
- Model file available at `${XDG_DATA_HOME:-~/.local/share}/soundvibes/models/ggml-base.en.bin`.
- Config file at `${XDG_CONFIG_HOME:-~/.config}/soundvibes/config.toml`.
- No network required.

## Tests

### AT-01: CLI starts with valid model
- Setup: set `model` in config to `${XDG_DATA_HOME:-~/.local/share}/soundvibes/models/ggml-base.en.bin`.
- Command: `sv --daemon`
- Expect: process starts, listens on socket, no error output.
- Pass: exit code is `0` after user stops the process.

### AT-02: Missing model returns error
- Setup: set `model` in config to `${XDG_DATA_HOME:-~/.local/share}/soundvibes/models/missing.bin`.
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
