# M10 — Linux support

## Summary

Implement the `Facade` for Linux desktops. X11 gets full support; Wayland gets the best support its security model allows, with documented limitations.

## Goals

- Dictate and rewrite on X11 (GNOME on Xorg, KDE X11, i3, …).
- Working dictation on Wayland GNOME and KDE through portals or documented helpers.
- A distributable package.

## Non-goals

- Every desktop environment and compositor; support the major ones and document the rest.
- Distribution-specific packages beyond the chosen format.

## User stories

- As a Linux user on KDE, I dictate into Slack and VS Code with a global hotkey.
- As a Wayland user, I'm told clearly what works and what extra setup is needed.

## Requirements

### Functional

1. **Hotkeys**
   - X11: `global-hotkey` (`XGrabKey`).
   - Wayland: the `org.freedesktop.portal.GlobalShortcuts` portal (KDE 6, GNOME 48+), with a fallback instructing the user to bind a desktop shortcut to `fluentb --toggle-dictate` (a CLI command that signals the running instance over a Unix socket).
2. **Microphone** — `cpal` with the ALSA backend, which also works through PipeWire/PulseAudio compatibility.
3. **Transcription and rewrite** — `whisper-rs` and `llama-cpp-2` with CPU by default and an optional Vulkan/CUDA build feature. Both stay fully local.
4. **Text insertion**
   - X11: clipboard + XTest Ctrl+V with clipboard restore, falling back to XTest typing.
   - Wayland: the `org.freedesktop.portal.RemoteDesktop` portal (keyboard), or `wtype`/`ydotool` if present. Otherwise copy to the clipboard and notify "Press Ctrl+V to paste".
5. **Selection reading** — X11 PRIMARY selection; on Wayland, `wl-paste --primary` or the clipboard fallback.
6. **Tray** — StatusNotifierItem (`ksni` or `tray-icon` with libappindicator). GNOME needs the AppIndicator extension; document this.
7. **Notifications** — `notify-rust` (D-Bus).
8. **Paths** — XDG config, data, and state directories.
9. **Launch at login** — an XDG autostart `.desktop` file.
10. **Packaging** — AppImage first; Flatpak later (it needs portal-only operation).
11. **Session detection** — a pure function picks the backend from `XDG_SESSION_TYPE` and `WAYLAND_DISPLAY`.

### Non-functional

- CI matrix adds `ubuntu-latest` for the core and Linux-specific pure code. The existing CI already runs on Linux for core tests.
- Missing optional helpers (`wtype`, `ydotool`) degrade gracefully with a one-time notification.

## Testing

- Pure tests for session detection and backend selection.
- A manual matrix: GNOME Wayland, GNOME X11, KDE Wayland, KDE X11, and one tiling WM (i3 or sway).

## Acceptance criteria

- [ ] Full dictate and rewrite parity on X11.
- [ ] Dictation works on GNOME and KDE Wayland, with the limitations listed in the README.
- [ ] The AppImage runs on Ubuntu 22.04+ and Fedora 40+.
- [ ] The core needed no Linux-specific changes.

## Open questions

- Is the Wayland fallback via `ydotool`, which needs `uinput` permissions, acceptable, or should the app go portal-only?
- Is Flatpak worth the sandbox constraints for v1?
