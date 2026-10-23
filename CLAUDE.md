# CLAUDE.md

This file gives Claude Code guidance for working in this repository.

## Project overview

fluentb is a system-tray app that makes writing faster and clearer in any application:

- **Dictate hotkey** — speak and have the transcription typed straight into the focused text field (Slack, Gmail, VS Code, Notes, etc.).
- **Rewrite hotkey** — take text the user has already typed and replace it with clean, fluent, correctly styled text.

Key constraints from the product description:

- **Local transcription.** Speech-to-text runs on-device with Whisper. Audio must never leave the user's machine; don't add network calls that send audio anywhere.
- **Local rewrite.** Rewriting runs on-device with a local LLM (llama.cpp, GGUF models). User text must never leave the machine; no cloud rewrite engines.
- **Models are downloaded on first run**, never bundled. The only allowed network traffic is model downloads and the update check. Specs live in `docs/specs/`.
- **Written in Rust.**
- **Lives in the system tray** and should stay out of the way: no main window, minimal UI, low idle resource use.
- **Platforms:** macOS is supported now. Windows and Linux are planned, so keep OS-specific code (global hotkeys, accessibility/text insertion, tray, microphone access) behind platform abstractions instead of scattering it through the codebase.

## Repository state

The crate is at an early stage (M1–M3 done; see `docs/specs/`). Update this file with build, test, and architecture details as more of the crate lands.

- `src/facade.rs` — the `Facade` trait: the only boundary between the pure core and side effects (hotkeys, microphone, Whisper, rewrite engine, text insertion, tray). Each platform provides its own implementation.
- `src/app/` — the pure core. `transition::next(state, event) -> (State, Vec<Effect>)` decides everything and returns effects as data; `runtime` is the only code that calls the facade, feeding each effect's result back as an `Event`. Tray updates and notifications are best effort, so a failure to report never triggers another report.
- `src/config/` — settings. `parse(text, os)` is pure and never fails: invalid or missing values fall back to their defaults and produce `ConfigWarning`s. `load` is the only file I/O (it creates a commented default file on first run). `run_app` takes the `Config` and shows all warnings in one notification at startup.
- `src/accelerator.rs` — validated hotkeys (`Accelerator`); `Cmd` means Command on macOS and Ctrl elsewhere.
- `src/platform/` — `Os`, plus pure helpers shared by platforms (`keymap`: `Accelerator` → `global-hotkey`; `applescript`). Pure code takes an `Os` parameter instead of using `cfg!`, so every OS's behaviour is testable on any machine.
- `src/platform/macos/` — the macOS `Facade` and entry point (`run`). The main thread runs the `tao` event loop (macOS requires hotkeys there); the state manager runs on a worker thread and asks the main thread to register hotkeys via `MainRequest`. This directory is thin OS glue, excluded from coverage and tested by hand: keep logic out of it and in pure modules (`audio`, `transcript`, `models`, `platform::keymap`).
- `src/audio.rs`, `src/transcript.rs`, `src/models.rs` — pure helpers: resampling to 16 kHz mono, cleaning Whisper output, model file paths and language selection.
- Building needs CMake and the Xcode Command Line Tools (whisper.cpp). `cargo test -- --ignored` runs a real transcription and needs `ggml-base.en.bin` in `~/Library/Application Support/fluentb/models/`.
- `src/testing.rs` — `FakeFacade` (test-only): scripted input events, recorded side effects, and per-call failure injection via `Call`.

## Conventions

- Standard Cargo workflow: `cargo build`, `cargo test`, `cargo fmt`, `cargo clippy`.
- Format with `rustfmt` and keep `clippy` clean before committing.
- CI (`.github/workflows/ci.yml`) runs fmt, clippy with `-D warnings`, rustdoc with `-D warnings`, tests under `cargo llvm-cov --fail-under-lines 95` (excluding OS glue in `src/main.rs` and `src/platform/macos/`), an MSRV build (1.88, `rust-version` in `Cargo.toml`), `cargo deny check` (`deny.toml`), and `cargo machete`. Every change needs unit tests that keep line coverage at 95% or above; test side-effecting code through a fake `Facade`.
- Lints are configured in `Cargo.toml`: clippy `pedantic` plus `unwrap_used`, `expect_used`, and `panic` (allowed in tests via `clippy.toml`), and `unsafe_code = "deny"`. Platform modules that need FFI opt in with `#![allow(unsafe_code)]`; don't add `allow`s elsewhere without a reason in a comment.
- CI pins third-party actions to full commit SHAs; Dependabot updates them weekly.

## Code style: functional

- Prefer pure functions; isolate side effects (audio capture, hotkeys, text insertion, tray) at the edges, with a pure core in between.
- Default to immutable data; use `mut` only when needed for performance or required by an API, and keep it local.
- Prefer iterator chains (`map`, `filter`, `fold`, `collect`) over manual loops with mutable accumulators.
- Model state changes as functions that return a new value (`fn next(state, event) -> State`) rather than mutating shared state.
- Use `Option`/`Result` with combinators and `?`; no `unwrap`/`expect` outside tests.
- Use enums and pattern matching for domain states instead of flags or nullable fields.
- Avoid shared mutable state (`Arc<Mutex<_>>`, globals) unless a platform API forces it.
