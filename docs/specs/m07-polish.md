# M7 — Polish and robustness

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
- [ ] Logs contain no user content (enforced by a test).
- [ ] Launch at login can be toggled from the menu.
