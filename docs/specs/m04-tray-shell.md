# M4 — Tray app shell

**Status:** In review (manual checklist pending)

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
- [x] A second instance is refused.

## Implementation notes

Decisions made while building M4:

- **Crates:** `tray-icon` 0.26 (with its `muda` menus) on the existing `tao` event loop. Both are by the Tauri team and cover macOS, Windows and Linux, which answers the open question for M9/M10.
- **Pure models:** `tray::menu::menu_model(&Config, TrayStatus, Os)` builds the menu, and `tray::icon::icon(TrayStatus)` draws the icons. The menu takes the `TrayStatus` the core already sends through `set_tray_status`, not the `State`, so the facade needs no new method. Hotkeys are shown the way each OS writes them (`⇧⌘D` in Apple's modifier order on macOS, `Ctrl+Shift+D` elsewhere).
- **Icons drawn in code:** the icons are anti-aliased shapes (signed distance functions) rendered at 44 px, so no image files ship. Idle is the fluentb bird from the logo, Recording a red filled microphone, Busy three dots, and Error a struck-through bird. (Idle and Error were an outlined and a struck-through microphone until issue #20: a microphone looked like every other dictation app and like macOS's own microphone indicator.) All but Recording are template images, so macOS tints them for light and dark menu bars.
- **Event bridge:** the hotkey and menu handlers run on the main thread and only forward raw `Signal`s into one channel. `platform::bridge::route` maps each signal to an `InputEvent` for the core, or to `OpenConfig`, which the adapter handles itself with `open -t`. The core is unchanged.
- **Status updates:** `set_tray_status` posts `MainRequest::SetStatus` to the event loop, which updates the icon, the tooltip and the status line. It never waits, so the worker is never blocked by the UI and the other way round.
- **Single instance:** `platform::instance` takes an OS file lock (`File::try_lock`) on `fluentb.lock` next to the config file. The OS releases it when the process exits, even after a crash, so a stale lock can't block the next launch. A second launch shows "fluentb is already running." and exits with code 1.
- **No Dock icon:** `ActivationPolicy::Accessory` at runtime. `packaging/macos/Info.plist` sets `LSUIElement` for the app bundle that M8 builds.
- **Measured on Apple Silicon:** idle CPU 0%; memory about 225 MB, almost all of it the `base.en` model.
- **Toolchain:** the minimum Rust version rises from 1.88 to 1.90, required by `tray-icon` and `muda` (`File::try_lock` needs 1.89). cargo-deny allows MPL-2.0 for `option-ext` only, which comes in through tray-icon's Linux `dirs` dependency.

## Manual checklist

- [ ] The icon is the bird when idle, a red microphone while recording, and three dots while transcribing.
- [ ] The icon reads well in both light and dark menu bars.
- [ ] The menu shows the status line and the configured hotkeys.
- [ ] "Open config…" opens `config.toml` in a text editor.
- [ ] "Quit fluentb" exits, including while recording (the microphone indicator turns off).
- [ ] There is no Dock icon and no window.

## Open questions

- ~~`tao` + `tray-icon` or `winit`?~~ Resolved: `tao` + `tray-icon` (see Implementation notes).
