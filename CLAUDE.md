# CLAUDE.md

This file gives Claude Code guidance for working in this repository.

## Project overview

fluentb is a system-tray app that makes writing faster and clearer in any application:

- **Dictate hotkey** — speak and have the transcription typed straight into the focused text field (Slack, Gmail, VS Code, Notes, etc.).
- **Rewrite hotkey** — take text the user has already typed and replace it with clean, fluent, correctly styled text.

Key constraints from the product description:

- **Local transcription.** Speech-to-text runs on-device with Whisper. Audio must never leave the user's machine; don't add network calls that send audio anywhere.
- **Written in Rust.**
- **Lives in the system tray** and should stay out of the way: no main window, minimal UI, low idle resource use.
- **Platforms:** macOS is supported now. Windows and Linux are planned, so keep OS-specific code (global hotkeys, accessibility/text insertion, tray, microphone access) behind platform abstractions instead of scattering it through the codebase.

## Repository state

The crate is at an early stage. `src/facade.rs` defines the `Facade` trait: the only boundary between the pure state manager and side effects (hotkeys, microphone, Whisper, rewrite engine, text insertion, tray). Each platform provides its own implementation; tests use a fake. Update this file with build, test, and architecture details as more of the crate lands.

## Conventions

- Standard Cargo workflow: `cargo build`, `cargo test`, `cargo fmt`, `cargo clippy`.
- Format with `rustfmt` and keep `clippy` clean before committing.
- CI (`.github/workflows/ci.yml`) runs fmt, clippy with `-D warnings`, and tests under `cargo llvm-cov --fail-under-lines 95`. Every change needs unit tests that keep line coverage at 95% or above; test side-effecting code through a fake `Facade`.

## Code style: functional

- Prefer pure functions; isolate side effects (audio capture, hotkeys, text insertion, tray) at the edges, with a pure core in between.
- Default to immutable data; use `mut` only when needed for performance or required by an API, and keep it local.
- Prefer iterator chains (`map`, `filter`, `fold`, `collect`) over manual loops with mutable accumulators.
- Model state changes as functions that return a new value (`fn next(state, event) -> State`) rather than mutating shared state.
- Use `Option`/`Result` with combinators and `?`; no `unwrap`/`expect` outside tests.
- Use enums and pattern matching for domain states instead of flags or nullable fields.
- Avoid shared mutable state (`Arc<Mutex<_>>`, globals) unless a platform API forces it.
