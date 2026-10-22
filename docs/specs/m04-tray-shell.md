# M4 — Tray app shell

## Summary

Turn the M3 background binary into a proper menu-bar app: a tray icon that reflects state, a small menu, no Dock icon, and an event loop that merges tray and hotkey events into the core.

## Goals

- The user can always see whether fluentb is idle, recording, or busy.
- The user can quit and open the config from the menu.
- The app costs close to nothing when idle.

## Non-goals

- A settings window (config stays a file).
- Onboarding (M6).
- Launch at login (M7).

## User stories

- As a user, I glance at the menu bar and see that fluentb is recording.
- As a user, I click the icon to see my hotkeys and quit the app.

## Requirements

### Functional

1. **Tray icon** — `tray-icon` crate on a `tao` (or `winit`) event loop. Template (monochrome) icons for:
   - `Idle`, `Recording` (filled/red dot), `Busy` (shared by `Transcribing`/`Rewriting`), `Error`.
2. **Menu**
   - A disabled status line ("Idle", "Recording…", "Transcribing…").
   - "Dictate: ⌘⇧D" and "Rewrite: ⌘⇧R" (informational, generated from config).
   - "Open config…" opens `config.toml` in the default editor.
   - "Quit fluentb" sends `QuitRequested`.
3. **Event bridge** — hotkey, tray-menu, and worker-completion events are merged into one channel that `Facade::next_event` reads.
4. **App bundle basics** — `LSUIElement = true` in Info.plist, so there is no Dock icon and no main window.
5. **Single instance** — a second launch exits and notifies "fluentb is already running".

### Non-functional

- Idle CPU ≈ 0% and RSS mostly made up of the Whisper model. Document the measured baseline.
- The UI thread never blocks on transcription or rewrite.

## Design notes

- `tao` requires the event loop on the main thread. The core runs on a worker thread, and `set_tray_status` posts a user event to the main loop. This is a forced cross-thread boundary, so keep it to a single channel.
- Generate the menu from `Config` and `State` with a pure function (`fn menu_model(&Config, &State) -> MenuModel`) so it can be tested without a GUI.

## Testing

- Unit tests for `menu_model` and accelerator display formatting (`Cmd+Shift+D` → `⌘⇧D`).
- Event-bridge tests with channels only, using no GUI.
- A manual checklist for icon states, menu actions, and single instance.

## Acceptance criteria

- [ ] The icon changes within 100 ms of each state change.
- [ ] Quit stops recording and exits cleanly.
- [ ] There is no Dock icon or window.
- [ ] A second instance is refused.

## Open questions

- `tao` + `tray-icon` or `winit`? Pick whichever has better Windows/Linux tray parity for M9/M10.
