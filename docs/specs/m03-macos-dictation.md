# M3 — macOS dictation end to end

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

- [ ] Dictation works in all four target apps.
- [ ] The user's clipboard is unchanged after insertion.
- [ ] The microphone indicator turns off when recording stops.
- [ ] Denied permissions produce the correct notification.
- [ ] CI stays green on `macos-latest`.

## Open questions

- Is clipboard paste or synthetic typing the better default? Measure reliability across the target apps.
- CI needs a macOS runner for platform code; Linux CI can only build the core.
