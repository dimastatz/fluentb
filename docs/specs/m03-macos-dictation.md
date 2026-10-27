# M3 — macOS dictation end to end

**Status:** Done

## Summary

Implement the macOS `Facade` for the dictation path: global hotkeys, microphone capture, local Whisper transcription, and text insertion into the focused app. This is the first build a person can actually use.

## Goals

- Press Dictate, speak, press Dictate again, and see the text typed into Slack, Gmail (in a browser), VS Code, and Notes.
- Transcription is fully on-device.
- Missing permissions produce a clear, actionable message instead of silent failure.

## Non-goals

- A tray icon (M4); M3 can run as a background CLI binary with notifications only.
- Rewrite (M5).
- Model download (M6); M3 loads a model from a configured local path.

## User stories

- As a user, I dictate a Slack message without touching the keyboard except for the hotkey.
- As a user, if I haven't granted microphone or accessibility access, I'm told exactly which setting to enable.

## Requirements

### Functional

1. **Hotkeys** — register the configured accelerators with the `global-hotkey` crate, with a fallback to a `CGEventTap` if needed. Report a registration conflict as `FacadeError::Hotkeys`.
2. **Microphone** — capture with `cpal` from the default input device into an in-memory buffer.
   - Downmix to mono and resample to 16 kHz in pure functions (`fn to_mono`, `fn resample`), using `rubato` or a simple linear resampler.
   - The audio callback writes into a lock-free ring buffer or channel. This is the one place where the OS forces shared state.
3. **Transcription** — `whisper-rs` (whisper.cpp bindings).
   - Load the model once at startup, keep it in memory, and reuse it for each request.
   - Use Metal acceleration where available.
   - Pass the language from config; `auto` means language detection.
   - Strip Whisper artefacts such as `[BLANK_AUDIO]` and leading/trailing whitespace with a pure post-processing function.
4. **Text insertion**
   - Primary: write the clipboard, send Cmd+V via `CGEvent`, then restore the previous clipboard contents after a short delay.
   - Alternative: type the text with `CGEventKeyboardSetUnicodeString` for apps that block paste. Choose per app later if needed.
5. **Permissions**
   - Microphone: `AVCaptureDevice` authorization status. A denial maps to `FacadeError::Microphone("…enable in System Settings → Privacy & Security → Microphone")`.
   - Accessibility: `AXIsProcessTrustedWithOptions` with a prompt. A denial maps to `FacadeError::TextAccess(…)`.
6. **Notifications** — `notify` uses `UNUserNotificationCenter` (or `mac-notification-sys`).
7. **Threading** — slow work (transcription) runs off the hotkey/event thread. The facade returns a completion event so the M1 core is unchanged.

### Non-functional

- **Latency:** with `base.en` on an Apple Silicon Mac, ≤ 1.5 s from the stop hotkey to inserted text for a 10 s utterance.
- **Idle:** no CPU use while idle; the microphone is closed when not recording (the macOS orange dot is off).
- **Privacy:** no network crate in the audio or transcription path. A CI check (`cargo tree`) confirms that no HTTP crate depends on audio modules.
- All macOS code lives in `src/platform/macos/` behind `#[cfg(target_os = "macos")]`.

## Testing

- Unit tests for `to_mono`, `resample`, and transcript post-processing (pure).
- A thin `MacFacade` with no business logic. Exclude its glue from coverage with `#[cfg_attr(coverage_nightly, coverage(off))]` or a documented `llvm-cov` ignore pattern so the 95% gate stays meaningful.
- A manual test checklist covering Slack, Gmail in Safari/Chrome, VS Code, Notes, Terminal, and denied permissions.
- An optional `#[ignore]` integration test that transcribes a bundled WAV fixture with the tiny model.

## Acceptance criteria

- [x] Dictation works in all four target apps.
- [x] The user's clipboard is unchanged after insertion.
- [x] The microphone indicator turns off when recording stops.
- [x] Denied permissions produce the correct notification.
- [x] CI stays green on `macos-latest`.

## Implementation notes

Decisions made while building M3:

- **Threads:** macOS requires global hotkeys to be registered and delivered on the main thread, so the main thread runs a `tao` event loop (the same loop M4's tray will use) with `ActivationPolicy::Accessory` (no Dock icon). The state manager runs on a worker thread; `register_hotkeys` asks the main thread through an event-loop proxy and waits for the reply. Transcription runs on the worker, off the event thread.
- **Audio:** the cpal callback sends sample chunks over a channel, so no buffer is shared between threads. Recordings under 300 ms are treated as accidental taps and not transcribed.
- **Text insertion:** clipboard + Cmd+V, then the previous clipboard *text* is restored after 250 ms. Images or files on the clipboard are not preserved yet.
- **Notifications:** `osascript` for now, because native notifications need an app bundle (M8).
- **Model:** read from `~/Library/Application Support/fluentb/models/ggml-<model>.bin` and kept in memory. Loading with Metal takes several seconds, so it runs on a background thread: hotkeys work immediately and the first transcription waits for the load if needed. A missing model is reported at startup with the download URL; M6 automates the download.
- **Measured on Apple Silicon:** startup is immediate; idle CPU 0%; memory about 256 MB with `base.en` loaded.
- **Rewrite:** not available yet (M5); pressing the Rewrite hotkey shows a notification saying so.
- **Coverage:** `src/main.rs` and `src/platform/macos/` are excluded from coverage (`--ignore-filename-regex`); everything else, including the hotkey mapping, audio conversion and transcript cleanup, is unit tested. An ignored test (`cargo test -- --ignored`) transcribes real speech generated with `say`.
- **Toolchain:** the minimum Rust version rises from 1.85 to 1.88, required by `tao`'s `url` → ICU dependencies. cargo-deny now allows the permissive Unlicense (whisper-rs) and BSL-1.0 (arboard's Windows deps), and ignores RUSTSEC-2024-0370, which is only reachable through tao's Linux GTK backend.

## Open questions

- Is clipboard paste or synthetic typing the better default? Measure reliability across the target apps.
- CI needs a macOS runner for platform code; Linux CI can only build the core.
