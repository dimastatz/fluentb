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
- **Platforms:** macOS is supported now; Windows and Linux are planned. Every design must assume several OS implementations exist (see [Platform design](#platform-design)).

## Platform design

fluentb uses the **strategy pattern**, with each OS as an **adapter**:

- **One interface.** The `Facade` trait (`src/facade.rs`) is the strategy: everything OS-dependent (global hotkeys, microphone, Whisper/LLM acceleration, selection reading, text insertion, tray, notifications, permissions, launch at login) is a method on it, expressed in OS-neutral domain types. The core (`src/app/`) only ever sees `&mut dyn Facade`.
- **One place.** All platform-dependent code lives under `src/platform/`, one adapter per OS: `platform::macos` today, `platform::windows` and `platform::linux` later. Each adapter translates the `Facade` calls to its OS APIs and owns its OS-only dependencies (`[target.'cfg(...)'.dependencies]` in `Cargo.toml`).
- **One selection point.** `platform::run()` picks the adapter for the OS the binary is built for, at compile time, and `main` just calls it. No code outside `src/platform/` uses `#[cfg(target_os)]`, `cfg!(windows)` or similar (tests excepted); OS differences in pure logic take an `Os` parameter instead.
- **Shared, pure helpers.** Logic that adapters have in common, or that differs by OS but needs no OS API (key mapping, paths, AppleScript quoting, session detection), is a pure function in a cross-platform module, tested on every OS. Adapters stay thin glue.
- **Adding an OS** means adding `src/platform/<os>/` with a `Facade` implementation and a `run`, plus a branch in `platform::run()`. The core, config and other adapters don't change. If a feature can't fit the `Facade`, extend the trait for every OS (an OS without support returns a `FacadeError` explaining why) rather than special-casing one.

## Repository state

The crate is at an early stage (M1–M5 implemented; see `docs/specs/` for status). Update this file with build, test, and architecture details as more of the crate lands.

- `src/facade.rs` — the `Facade` trait: the only boundary between the pure core and side effects (hotkeys, microphone, Whisper, rewrite engine, text insertion, tray). Each platform provides its own implementation.
- `src/app/` — the pure core. `transition::next(state, event) -> (State, Vec<Effect>)` decides everything and returns effects as data; `runtime` is the only code that calls the facade, feeding each effect's result back as an `Event`. Tray updates and notifications are best effort, so a failure to report never triggers another report.
- `src/config/` — settings. `parse(text, os)` is pure and never fails: invalid or missing values fall back to their defaults and produce `ConfigWarning`s. `load` is the only file I/O (it creates a commented default file on first run). `run_app` takes the `Config` and shows all warnings in one notification at startup.
- `src/accelerator.rs` — validated hotkeys (`Accelerator`); `Cmd` means Command on macOS and Ctrl elsewhere.
- `src/platform/` — all platform-dependent code: `run()` (the single place that picks the OS adapter), `Os`, and helpers shared by platforms: `keymap` (`Accelerator` → `global-hotkey`), `applescript`, `bridge` (routes raw hotkey and menu `Signal`s from one channel to `InputEvent`s), and `instance` (the single-instance file lock). Pure code takes an `Os` parameter instead of using `cfg!`, so every OS's behaviour is testable on any machine.
- `src/platform/macos/` — the macOS adapter: its `Facade` and entry point (`run`). The main thread runs the `tao` event loop and owns the menu-bar icon (macOS requires hotkeys and the tray there); the state manager runs on a worker thread, reads hotkey and menu events through the bridge channel, and sends `MainRequest`s to register hotkeys and update the icon. This directory is thin OS glue, excluded from coverage and tested by hand: keep logic out of it and in pure modules (`audio`, `transcript`, `models`, `platform::keymap`).
- `src/tray/` — the tray as data: `menu::menu_model` builds the menu from the config and status, and `icon::icon` draws each status icon in code (no image files).
- `src/rewrite/` — pure rewrite helpers: `prompt` (chat templates as segments, so user text is tokenized without special tokens), `output::clean_output` (strip preambles and wrappers, reject empty or runaway output), `check_length`, and `residency` (when to unload the idle model).
- `src/audio.rs`, `src/transcript.rs`, `src/models.rs` — pure helpers: resampling to 16 kHz mono, cleaning Whisper output, model files (`Model::Whisper`/`Model::Rewrite`: file names, download URLs, paths) and language selection.
- llama.cpp is linked **dynamically** (`llama-cpp-2` `dynamic-link`), because whisper-rs links its own ggml statically and two static copies clash. `build.rs` adds the rpaths. `cargo test --release -- --ignored` also runs a real rewrite, which needs `Qwen3-1.7B-Q4_K_M.gguf` in the models folder. `eval/rewrite/` holds the manual model comparison.
- Building needs CMake and the Xcode Command Line Tools (whisper.cpp). `cargo test -- --ignored` runs a real transcription and needs `ggml-base.en.bin` in `~/Library/Application Support/fluentb/models/`.
- `src/testing.rs` — `FakeFacade` (test-only): scripted input events, recorded side effects, and per-call failure injection via `Call`.

## Conventions

- Standard Cargo workflow: `cargo build`, `cargo test`, `cargo fmt`, `cargo clippy`.
- Format with `rustfmt` and keep `clippy` clean before committing.
- CI (`.github/workflows/ci.yml`) runs fmt, clippy with `-D warnings`, rustdoc with `-D warnings`, tests under `cargo llvm-cov --fail-under-lines 95` (excluding OS glue in `src/main.rs` and `src/platform/macos/`), an MSRV build (1.90, `rust-version` in `Cargo.toml`), `cargo deny check` (`deny.toml`), and `cargo machete`. Every change needs unit tests that keep line coverage at 95% or above; test side-effecting code through a fake `Facade`.
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
