# Acceptance Tests: Offline STT CLI (sv)

These tests validate the MVP behavior for the offline Linux CLI.

## Environment
- Linux x86_64 machine with a working microphone.
- Model file available at `./models/ggml-tiny.en.bin`.
- Config file at `${XDG_CONFIG_HOME:-~/.config}/soundvibes/config.toml`.
- No network required.

## Tests

### AT-01: CLI starts with valid model
- Setup: set `model` in config to `./models/ggml-tiny.en.bin`.
- Command: `sv`
- Expect: process starts, begins capturing audio, no error output.
- Pass: exit code is `0` after user stops the process.

### AT-02: Missing model returns error
- Setup: set `model` in config to `./models/missing.bin`.
- Command: `sv`
- Expect: error message indicating missing model.
- Pass: exit code is `2`.

### AT-03: Invalid input device
- Setup: set `device` in config to `"nonexistent"`.
- Command: `sv`
- Expect: error message indicating device not found.
- Pass: exit code is `3`.

### AT-04: Final transcript emitted
- Setup: set `model` in config to `./models/ggml-tiny.en.bin`.
- Command: `sv`
- Action: hold the hotkey, speak a short sentence, release the hotkey.
- Expect: final transcript is printed after key release.
- Pass: final output appears shortly after release.

### AT-05: JSONL output format
- Setup: set `format` in config to `"jsonl"`.
- Command: `sv`
- Action: hold the hotkey, speak a short sentence, release the hotkey.
- Expect: output lines are valid JSON with `type`, `text`, `timestamp`.
- Pass: JSONL lines parse and include required fields.

### AT-06: Offline operation
- Setup: set `model` in config to `./models/ggml-tiny.en.bin`.
- Command: disconnect network, run `sv`
- Expect: no network access required.
- Pass: transcription works without network connectivity.
