# M6 — Model management and first run

## Summary

Make fluentb work out of the box: on first run, download and verify the Whisper model and the local rewrite model, walk the user through granting permissions, and teach them the hotkeys. No models are bundled with the app; everything is downloaded on first run.

## Goals

- A new user goes from launch to their first successful dictation in under 2 minutes, with no manual file handling.
- Downloaded models are verified and resumable.
- Dictation becomes usable as soon as the Whisper model is ready, without waiting for the larger rewrite model.
- Permission problems are caught during onboarding, not on the first hotkey press.

## Non-goals

- Bundling models inside the app.
- Fine-tuning or custom models.
- An in-app model browser beyond the registry.

## User stories

- As a new user, I launch fluentb, it downloads what it needs with visible progress, asks for permissions, and tells me which hotkey to press.
- As a new user, I can start dictating while the rewrite model is still downloading.
- As a user on a slow machine, I switch to smaller models from the tray menu.

## Requirements

### Functional

1. **Model registry** — a static, pure table of `{ id, kind, url, size_bytes, sha256 }`:
   - `kind = Whisper`: tiny, base, small, medium, large-v3 (plus `.en` variants), using whisper.cpp GGML files.
   - `kind = Rewrite`: the GGUF instruct models chosen by the M5 evaluation (default plus a light option).
2. **Storage** — `<data_dir>/fluentb/models/<id>.<ext>`, e.g. `~/Library/Application Support/fluentb/models/` on macOS.
3. **Download**
   - Stream to a `.part` file with HTTP Range resume and verify SHA-256 before renaming.
   - Check free disk space before starting, and fail early with a clear message.
   - Show progress in the tray status line ("Downloading speech model… 42%", "Downloading rewrite model… 10%") and notify on completion or failure.
   - Retry with backoff, and fail with a clear message when offline. The next launch resumes the download.
   - Model downloads are the only network access fluentb makes besides the update check (M8). Nothing is uploaded, and no audio or text is ever sent.
4. **First-run order**
   - Download the Whisper model first (about 150 MB for `base`), then the rewrite model (about 1–2 GB).
   - Downloads run one at a time, in the background.
5. **Feature gating** — each hotkey checks its model:
   - Dictate before the Whisper model is ready: notify "Speech model still downloading (42%)".
   - Rewrite before the rewrite model is ready: notify "Rewrite model still downloading (10%)".
6. **Model switching** — tray submenus "Speech model ▸ …" and "Rewrite model ▸ …" update the config, download the model if needed, and hot-swap it once loaded.
7. **Disk cleanup** — "Remove unused models" in the menu.
8. **Onboarding** (first run, tracked by a `first_run_complete` marker file):
   1. Welcome notification: "fluentb lives in your menu bar."
   2. Start the model downloads in the background immediately.
   3. Request microphone permission; poll or listen until it is granted.
   4. Request Accessibility permission; open the System Settings pane directly.
   5. When the Whisper model is ready, show the hotkeys and invite the user to try a dictation.
   6. When the rewrite model is ready, notify "Rewrite is ready: select text and press ⌘⇧R."
9. **Default model choice**
   - Whisper: `base.en` if the system language is English, otherwise `base`.
   - Rewrite: the default model, or the light model on machines with under 16 GB of RAM.
10. **Permission status** in the tray menu: "⚠ Accessibility not granted — Fix…" while missing.

### Non-functional

- No telemetry. Download URLs are listed in the README.
- The app stays responsive during downloads.
- Total first-run download size is shown before starting (e.g. "fluentb will download about 2.2 GB of models").

## Design notes

- Per-model state as an enum: `ModelState { Missing, Downloading { progress }, Ready(path), Failed(reason) }`. The core receives `ModelStateChanged(kind, state)` events and gates each hotkey on its own model.
- The download queue is a pure function over the registry and the files on disk (`fn pending_downloads(&Config, &[Installed]) -> Vec<ModelId>`); only the HTTP transfer is I/O.
- Represent onboarding as a pure step machine (`fn next_step(OnboardingState, Event)`) so it can be tested without a GUI.

## Testing

- Registry tests: unique IDs, and every entry has a valid URL and hash format.
- Downloader tests against a local HTTP test server (`httpmock`) covering the full download, a resume, a hash mismatch, a 404, and too little disk space.
- `pending_downloads` and default-model-choice tests.
- Feature-gating tests in the core: hotkeys pressed while each model is `Downloading`, `Failed`, or `Ready`.
- Onboarding step-machine tests.
- A manual fresh-user test on a clean macOS account, including killing the app mid-download.

## Acceptance criteria

- [ ] A fresh install reaches a working dictation without manual steps beyond permission clicks.
- [ ] Dictation works before the rewrite model finishes downloading.
- [ ] A corrupted or partial download is detected and resumed or retried.
- [ ] Switching a model takes effect without restarting.
- [ ] Missing permissions are visible in the tray menu.
