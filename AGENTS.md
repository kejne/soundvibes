# Agent Instructions

This repository is a Rust CLI app for offline speech-to-text on Linux. Use these notes to work efficiently and consistently.

## Source-of-Truth Rules

- Follow these instructions plus any user-provided requirements.

## Issue Tracking (bd/beads)

This project uses **bd (beads)** for issue tracking. Run `bd prime` for workflow context, or install hooks (`bd hooks install`) for auto-injection.

Quick reference:
- `bd ready` - Find unblocked work
- `bd show <id>` - View issue details
- `bd update <id> --status in_progress` - Claim work
- `bd create "Title" --type task --priority 2` - Create issue
- `bd close <id>` - Complete work
- `bd sync` - Sync with git (run at session end)

## Agent Workflow

- Apply TDD by default: write/extend tests before or alongside feature changes.
- Prefer automated tests over manual steps whenever possible.
- Keep tests deterministic and fast; skip gracefully when external files are missing.
- Call out any gaps when tests cannot be written (e.g., hardware-dependent flows).
- For new functionality, add or extend an automated acceptance test and run it as part of validation.
- Automated acceptance criteria live in `docs/acceptance-tests.md` and must map to tests in `tests/acceptance.rs`.

## Build / Run / Lint / Test

These commands are inferred from repo files and standard Rust conventions. Prefer these unless the user requests otherwise.

Build:
- `cargo build`
- `cargo build --release`

Run:
- `cargo run --` (basic CLI)
- `sv` (after install/build)
- `sv --daemon` (daemon mode)

Mise tasks:
- `mise run download-model` (downloads ggml model)
- `SIZE=small mise run download-model` (pick model size)
- `mise run run-local` (alias for `cargo run --`)
- `mise run debug-local` (runs with local model + VAD debug)

Tests:
- `cargo test` (all tests)
- `cargo test transcribes_sample_audio` (single test by name)
- `cargo test --test whisper_integration` (single integration test file)
- `SV_MODEL_PATH=... cargo test --test whisper_integration` (override model path)
- `cargo test --test acceptance` (automated acceptance tests)
- `cargo test --test acceptance --features test-support` (acceptance tests using test support mocks)
- `SV_HARDWARE_TESTS=1 cargo test --test acceptance` (hardware acceptance tests)

Formatting / linting:
- `cargo fmt`
- `cargo clippy --all-targets --all-features` (use when linting is requested)

Validation plan requirement:
- Always note which checks you ran or plan to run (tests, manual acceptance checks, etc.).

## Coding Style Guidelines (Rust)

Imports:
- Prefer module declarations at top (`mod audio;`).
- Order `use` groups: external crates, then `std`, then internal crate (`sv::...`).
- Keep `use` lists explicit; avoid glob imports in production code.

Formatting:
- Use `cargo fmt` default (rustfmt). 4-space indentation, trailing commas where idiomatic.
- Keep lines readable; use line breaks for long argument lists or match arms.

Types and data handling:
- Use `PathBuf` for owned paths and `&Path` for borrowed paths.
- Prefer `Option<T>` for optional config values and `Result<T, AppError>` for fallible operations.
- Use `u32` for sample rates, `u64` for durations in milliseconds.

Naming conventions:
- Types/enums/traits in `PascalCase` (e.g., `AudioHost`, `VadMode`).
- Functions/variables in `snake_case`.
- Boolean flags use `is_`/`has_`/`enable_` prefixes when clarity helps.

Error handling:
- Prefer custom error types (`AppError`, `AudioError`, `WhisperError`) that implement `Display` + `Error`.
- Map lower-level errors into domain errors with `map_err` and contextual messages.
- For CLI failures, print with `eprintln!` and exit using `AppError::exit_code()`.
- Avoid panics in runtime code; use `expect`/`unwrap` only in tests.

Configuration handling:
- CLI options use `clap` derive (`#[derive(Parser)]`).
- Config file uses TOML via `serde::Deserialize` and `#[serde(default)]`.
- When merging config sources, favor CLI flags over config values.

Concurrency and IO:
- Hotkey and daemon listeners use `std::thread` + `mpsc` channels.
- Use timeouts (`recv_timeout`) instead of blocking forever.
- Keep socket handling resilient: clean up stale sockets, surface explicit errors.

Audio/VAD specifics:
- VAD config lives in `src/audio.rs`; keep thresholds and timing constants there.
- When working with audio streams, surface device errors via `AudioErrorKind`.

FFI bindings:
- `src/whisper.rs` wraps generated bindings from `build.rs`.
- Keep unsafe blocks tight and local; return safe Rust types to callers.

Tests:
- Tests that depend on external files should skip gracefully when inputs are missing.
- Use locks (`OnceLock<Mutex<()>>`) for tests that touch shared resources.

## Repo Layout

- `src/main.rs`: CLI entrypoint, config, runtime orchestration.
- `src/audio.rs`: CPAL/ALSA capture, VAD trimming utilities.
- `src/whisper.rs`: whisper.cpp FFI wrapper.
- `tests/whisper_integration.rs`: integration test with a model file.
- `docs/*.md`: product and acceptance documentation.

## Release / Session Completion

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

MANDATORY WORKFLOW:
1. File issues for remaining work (bd).
2. Run quality gates if code changed (tests, lint, build).
3. Update issue status (close finished, update in-progress).
4. PUSH TO REMOTE:
   ```bash
   git pull --rebase
   bd sync
   git push
   git status  # MUST show "up to date with origin"
   ```
5. Clean up (clear stashes, prune remote branches).
6. Verify all changes committed and pushed.
7. Hand off context for the next session.

CRITICAL RULES:
- Work is NOT complete until `git push` succeeds.
- NEVER stop before pushing - that leaves work stranded locally.
- NEVER say "ready to push when you are" - YOU must push.
- If push fails, resolve and retry until it succeeds.
