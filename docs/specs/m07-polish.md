# M7 — Polish and robustness

**Status:** In review (manual checklist pending)

## Summary

Turn a working app into one people keep running all day: more input modes, cancellation, feedback cues, resilience to bad states, privacy-safe logging, and launch at login.

## Goals

- Dictation feels responsive and forgiving.
- The app recovers from every failure without a restart.
- Problems can be diagnosed from logs that contain no user content.

## Non-goals

- New platforms (M9, M10).
- New features beyond dictate and rewrite.

## User stories

- As a user, I hold the hotkey to talk and release it to insert (push-to-talk).
- As a user, I press Esc to cancel a recording I started by mistake.
- As a user, I hear a subtle sound when recording starts and stops.
- As a user, fluentb starts automatically when I log in.

## Requirements

### Functional

1. **Push-to-talk** — `dictation.mode = "push-to-talk"`. The core receives `HotkeyReleased(Hotkey)`; key down starts recording and key up stops it. Facade change: hotkey events carry press or release.
2. **Cancel** — while `Recording`, `Transcribing`, or `Rewriting`, Esc (registered only during those states) discards the work and returns to `Idle` without inserting anything.
3. **Max recording length** — configurable (default 120 s). A `Tick`/`Timeout` event stops recording automatically and transcribes.
4. **Silence handling**
   - Trim leading and trailing silence (pure energy-based VAD) before transcription.
   - If the whole clip is silent, skip transcription and notify "No speech detected".
5. **Audio cues** — optional start and stop sounds (`dictation.sounds = true`), played via the facade (`Effect::PlayCue`).
6. **Error tray state** — show `Error` for 2 s, then `Idle` (a timer event).
7. **Device changes** — if the input device disappears mid-recording (e.g. AirPods disconnect), stop gracefully and transcribe what was captured.
8. **Launch at login** — a tray toggle using `SMAppService` (macOS 13+).
9. **Logging** — `tracing` with a rolling file in the log dir.
   - Log state transitions, timings, and error kinds.
   - Never log audio, transcripts, selections, or rewrite output. Enforce this by logging event *kinds* only.
   - "Reveal logs" in the tray menu.
10. **Crash safety** — the microphone is released on every exit path (Drop guards), and panics are caught at the worker boundary and reported as an error state.

### Non-functional

- Transcription latency ≤ M3 targets after VAD trimming.
- 24-hour soak test: no memory growth beyond the model, and no stuck states.

## Testing

- Core transition tests for push-to-talk, cancel, timeout, and silence.
- Pure VAD tests on synthetic signals (silence, tone, silence-tone-silence).
- A log-redaction test: a tracing subscriber captures all output during the fake flows and asserts that no transcript text appears.
- Manual tests: device unplugged, sleep/wake during recording, and the soak run.

## Acceptance criteria

- [ ] Push-to-talk and toggle modes both work.
- [ ] Esc cancels in every busy state.
- [ ] Silent recordings never insert text.
- [x] Logs contain no user content (enforced by a test).
- [ ] Launch at login can be toggled from the menu.

## Implementation notes

Decisions made while building M7:

- **Facade changes:**
  - `Hotkey::Cancel`;
  - `InputEvent::HotkeyReleased`, `TimerFired(Timer)` and `MicrophoneLost`;
  - `play_cue(Cue)` and `start_timer(Timer, Duration)`, where starting a timer again replaces the pending one;
  - `FacadeError::Cancelled`, which the core never reports, and `Internal`, for a caught panic.
- **Push-to-talk:** the transition table stops a recording on either a press or a release of Dictate. `app::input::accepts` filters by mode: toggle mode drops releases, and push-to-talk drops a second press while recording. On macOS, hotkey registration now only (un)registers the difference, so adding Esc mid-recording can't lose the release of the key being held.
- **Cancel:**
  - `next` adds `Effect::ListenForCancel(busy)` whenever the app goes from idle to busy or back, so Esc is registered exactly while busy (checked for every state and event pair).
  - The runtime is synchronous, so Esc can't reach the core while `transcribe` or `rewrite` blocks. On macOS, the hotkey handler also sets a shared flag the moment Esc is pressed. whisper.cpp's abort callback and the llama.cpp token loop check that flag and return `Cancelled`, which the core treats like Esc: back to idle, with no notification. The flag is cleared when a recording or rewrite starts.
  - The raw whisper abort callback is used because `set_abort_callback_safe` leaks its closure on every call.
- **Timers:** `MaxRecording` (`dictation.max_recording_seconds`, default 120) and `ClearError` (2 s). A timer that fires late is harmless: `MaxRecording` only acts while recording, and `ClearError` only while idle. The macOS `next_event` waits for whichever comes first, the next timer or the rewrite model's unload (`platform::timers`).
- **Silence:** `audio::trim_silence` keeps 30 ms frames above an RMS of 0.008 (about −42 dBFS), plus 200 ms of padding. The core drops a recording shorter than 300 ms quietly and says "No speech detected" for a silent one, so Whisper never sees silence, which it tends to fill with made-up words.
- **Cues:** `dictation.sounds` (default on) plays the system Tink and Pop sounds at 40% volume through `afplay`, without waiting. The start cue plays before the microphone opens, and the stop cue after it closes.
- **Lost microphone:** cpal's `DeviceNotAvailable` error sends `Signal::MicrophoneLost`. The core stops recording, transcribes what was captured, and says why. A default device that cpal reroutes (`DeviceChanged`) just keeps recording.
- **Launch at login:** `SMAppService` needs an app bundle, which arrives with M8. Until then, the "Launch at login" menu toggle writes or deletes a LaunchAgent at `~/Library/LaunchAgents/io.github.dimastatz.fluentb.plist`, which runs the current executable at login (`platform::login` builds the plist). Switch to `SMAppService` in M8.
- **Logging:**
  - `tracing` writes to a daily rolling file in `~/Library/Logs/fluentb`, keeping 7 days (`logging`). Only fluentb's own targets are recorded.
  - The runtime logs each transition (from-state, event kind, to-state, effect kinds) and each effect's kind and duration. Failures are logged by subsystem (`FacadeError::kind`), never by message.
  - `Event::kind` and `Effect::kind` contain no payloads. A test captures all output from dictation, rewrite, failure and panic flows that carry marked text, and asserts that the marked text never appears.
  - "Reveal logs" in the menu opens the folder.
- **Crash safety:**
  - The runtime runs each effect under `catch_unwind`. A panic in a reported effect becomes `Internal("<effect> crashed")`; a panic in a best-effort one (tray, notification, cue, timer, Esc registration) is swallowed, so reporting can't loop.
  - The macOS worker also catches a panic while reading input, and notifies before exiting.
  - The microphone stream closes when `Recording` is dropped, so it is released on every exit path.

## Manual checklist

- [ ] Toggle mode: press, speak, press again inserts the text, with the start and stop sounds.
- [ ] Push-to-talk (`mode = "push-to-talk"`): hold, speak, release inserts the text, and holding longer doesn't stop early.
- [ ] Esc while recording, transcribing (a long clip), and rewriting cancels each without inserting anything; Esc works normally in other apps when fluentb is idle.
- [ ] With `max_recording_seconds = 10`, a recording stops by itself after 10 s and is transcribed.
- [ ] A recording of silence says "No speech detected" and inserts nothing.
- [ ] Disconnect AirPods while recording: recording stops and what was said is inserted.
- [ ] A failure shows the error icon for about 2 s, then the idle icon.
- [ ] "Launch at login" toggles its check mark; after logging out and in, fluentb is running.
- [ ] "Reveal logs" opens `~/Library/Logs/fluentb`; after a dictation and a rewrite, the log has transitions and timings but no text.
- [ ] Sleep and wake during a recording: the app recovers.
- [ ] 24-hour soak: memory stays flat apart from the models, and no state gets stuck.
