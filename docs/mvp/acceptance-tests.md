# Acceptance Tests: Offline STT CLI (sv)

These tests validate the MVP behavior for the offline Linux CLI.

## Environment
- Linux x86_64 machine with a working microphone.
- Model file available at `./models/ggml-tiny.en.bin`.
- No network required.

## Tests

### AT-01: CLI starts with valid model
- Command: `sv --model ./models/ggml-tiny.en.bin`
- Expect: process starts, begins capturing audio, no error output.
- Pass: exit code is `0` after user stops the process.

### AT-02: Missing model returns error
- Command: `sv --model ./models/missing.bin`
- Expect: error message indicating missing model.
- Pass: exit code is `2`.

### AT-03: Invalid input device
- Command: `sv --model ./models/ggml-tiny.en.bin --device "nonexistent"`
- Expect: error message indicating device not found.
- Pass: exit code is `3`.

### AT-04: Partial transcripts appear
- Command: `sv --model ./models/ggml-tiny.en.bin`
- Action: speak a short sentence with pauses.
- Expect: partial output updates during speech.
- Pass: partial text updates appear within a few seconds.

### AT-05: Final transcript emitted
- Command: `sv --model ./models/ggml-tiny.en.bin`
- Action: speak a short sentence and stop.
- Expect: final transcript is printed after end-of-utterance.
- Pass: final output appears after silence timeout.

### AT-06: JSONL output format
- Command: `sv --model ./models/ggml-tiny.en.bin --format jsonl`
- Action: speak a short sentence.
- Expect: output lines are valid JSON with `type`, `text`, `timestamp`.
- Pass: JSONL lines parse and include required fields.

### AT-07: Offline operation
- Command: disconnect network, run `sv --model ./models/ggml-tiny.en.bin`
- Expect: no network access required.
- Pass: transcription works without network connectivity.
