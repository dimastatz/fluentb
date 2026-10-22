# M9 — Windows support

## Summary

Implement the `Facade` for Windows 10/11 so dictation and rewrite work the same way they do on macOS, then package and release it.

## Goals

- Feature parity with macOS M3–M7.
- No changes to the core; all differences live in `src/platform/windows/`.
- A signed installer.

## Non-goals

- Windows on ARM builds (stretch goal).
- Microsoft Store distribution.

## User stories

- As a Windows user, I dictate into Teams, Outlook, VS Code, and Notepad with `Ctrl+Shift+D`.
- As a Windows user, fluentb sits in the notification area and starts with Windows.

## Requirements

### Functional

1. **Hotkeys** — `global-hotkey` (`RegisterHotKey`). Push-to-talk needs key-up events, so use a low-level keyboard hook (`WH_KEYBOARD_LL`) for that mode.
2. **Microphone** — `cpal` with the WASAPI backend; the pure mono/resample code is reused.
3. **Transcription and rewrite** — `whisper-rs` and `llama-cpp-2` with CPU by default and an optional CUDA or Vulkan build feature. Both stay fully local.
4. **Text insertion** — clipboard + `SendInput` Ctrl+V with clipboard restore, falling back to `SendInput` with `KEYEVENTF_UNICODE`.
5. **Selection reading** — UI Automation `TextPattern.GetSelection`, with a Ctrl+C clipboard fallback.
6. **Tray** — `tray-icon` with light and dark theme icons.
7. **Notifications** — Windows toast notifications (`winrt-notification` or `tauri-winrt-notification`).
8. **Permissions** — microphone privacy setting check (`ms-settings:privacy-microphone` link on denial). No Accessibility equivalent is needed, but elevated windows (UIPI) can't receive input; detect this and notify.
9. **Paths** — config in `%APPDATA%`, models and logs in `%LOCALAPPDATA%`.
10. **Launch at login** — `HKCU\…\Run` registry key.
11. **Packaging** — MSI via `cargo-wix` or an MSIX/winget manifest, code-signed (Authenticode). Add it to the release workflow.

### Non-functional

- CI matrix adds `windows-latest`: fmt, clippy, tests, and coverage on the core.
- Accelerator mapping: `Cmd` in config maps to `Ctrl` (done in M2).

## Testing

- Pure-code tests run unchanged on Windows CI.
- A manual checklist covering Teams, Outlook, Chrome/Gmail, VS Code, Notepad, and an elevated window (expected: notification).

## Acceptance criteria

- [ ] Dictate and rewrite work in all target apps.
- [ ] The installer is signed and installs without SmartScreen blocking (once reputation builds).
- [ ] The core needed no Windows-specific changes.
- [ ] Windows CI is green.

## Open questions

- Code-signing certificate source (EV vs OV; Azure Trusted Signing)?
- Which GPU backend to ship by default?
