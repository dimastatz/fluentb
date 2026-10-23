# M0 — Foundation

**Status:** Done (branch `platform-facade`, commit `5d22326`)

## Summary

Set up the crate, the side-effect boundary, and CI so every later milestone starts from a tested, enforced baseline.

## Goals

- Define the `Facade` trait (`src/facade.rs`) as the only boundary between the pure core and the outside world.
- Define the shared domain types: `Hotkey`, `HotkeyBinding`, `InputEvent`, `Audio`, `TrayStatus`, `FacadeError`.
- Add CI that runs fmt, clippy (`-D warnings`), and tests under `cargo llvm-cov --fail-under-lines 95`.

## Non-goals

- Any real platform implementation.
- Any state machine logic.

## Delivered

- `Facade` trait with input (`register_hotkeys`, `next_event`), dictation (`start_recording`, `stop_recording`, `transcribe`), rewrite (`selected_text`, `rewrite`), and output (`insert_text`, `set_tray_status`, `notify`) methods.
- `FakeFacade` in tests that scripts events and records calls.
- The `Facade` is the strategy interface that every OS adapter implements; see [Platform architecture](README.md#platform-architecture).
- `.github/workflows/ci.yml`.

## Acceptance criteria

- [x] `cargo test` passes with a fake facade.
- [x] CI enforces fmt, clippy, and 95% line coverage.
