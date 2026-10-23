# M1 — Pure state manager

**Status:** Done

## Summary

Build the platform-agnostic core that decides what fluentb does in response to each event. The core is a pure transition function plus a thin interpreter that runs its effects through a `Facade`. After M1, the full dictate and rewrite flows run end to end against the fake facade.

## Goals

- Model the app as an explicit state machine with no shared mutable state.
- Keep every side effect out of the transition function.
- Handle errors and repeated hotkey presses predictably.
- Reach 100% branch coverage of the transition table with fake-facade tests.

## Non-goals

- Real hotkeys, audio, Whisper, or text insertion (M3+).
- Configuration loading (M2). M1 uses hard-coded default bindings.
- Cancellation, push-to-talk, and timeouts (M7).

## User stories

- As a user, I press Dictate once to start recording and again to stop, and the transcription appears in my text field.
- As a user, I press Rewrite with text selected, and the selection is replaced with a cleaner version.
- As a user, I press Rewrite with nothing selected and get a short notification instead of a silent failure.
- As a user, if something fails, I'm told what failed and the app returns to idle and stays usable.

## Requirements

### Functional

1. **State** — `enum State { Idle, Recording, Transcribing, Rewriting }`. Errors are not a resting state: a failure triggers a notification and a brief `TrayStatus::Error`, then a return to `Idle`.
2. **Events** — extend input with internal completion events:
   - `HotkeyPressed(Hotkey)`, `QuitRequested` (existing)
   - `RecordingStopped(Audio)`, `Transcribed(String)`, `SelectionRead(Option<String>)`, `Rewritten(String)`, `Failed(FacadeError)`
3. **Effects** — `enum Effect { StartRecording, StopRecording, Transcribe(Audio), ReadSelection, Rewrite(String), InsertText(String), SetTray(TrayStatus), Notify(String), Exit }`.
4. **Transition** — `fn next(state: State, event: Event) -> (State, Vec<Effect>)`, total over all `(state, event)` pairs:

   | State        | Event                        | Next state   | Effects                                          |
   | ------------ | ---------------------------- | ------------ | ------------------------------------------------ |
   | Idle         | Hotkey(Dictate)              | Recording    | SetTray(Recording), StartRecording               |
   | Recording    | Hotkey(Dictate)              | Transcribing | StopRecording, SetTray(Transcribing)             |
   | Transcribing | RecordingStopped(audio)      | Transcribing | Transcribe(audio)                                |
   | Transcribing | Transcribed(text)            | Idle         | InsertText(text) if non-empty, SetTray(Idle)     |
   | Idle         | Hotkey(Rewrite)              | Rewriting    | ReadSelection                                    |
   | Rewriting    | SelectionRead(Some(text))    | Rewriting    | SetTray(Rewriting), Rewrite(text)                |
   | Rewriting    | SelectionRead(None)          | Idle         | Notify("Select some text to rewrite")            |
   | Rewriting    | Rewritten(text)              | Idle         | InsertText(text), SetTray(Idle)                  |
   | any          | Failed(err)                  | Idle         | SetTray(Error), Notify(err), SetTray(Idle)       |
   | Recording    | Failed(err)                  | Idle         | also StopRecording (best effort) before notify   |
   | any          | QuitRequested                | —            | StopRecording if recording, Exit                 |
   | busy states  | other hotkeys                | unchanged    | none (ignored)                                   |

5. **Interpreter** — `fn run(effect: &Effect, facade: &mut dyn Facade) -> Option<Event>` turns each effect into a facade call and returns the follow-up event (`Ok` maps to a completion event, `Err` maps to `Failed`).
6. **Event loop** — `fn run_app(facade: &mut dyn Facade, bindings: &[HotkeyBinding]) -> Result<()>`: register hotkeys, then fold over events. Internal follow-up events are processed before blocking on `next_event` again. The loop ends on `Exit`.
7. Whitespace-only transcriptions are dropped instead of inserted.

### Non-functional

- `next` is pure: no I/O and no `mut` beyond local builders.
- No `unwrap`/`expect` outside tests.
- Module layout: `src/core/{state.rs, effect.rs, transition.rs, runtime.rs}` or similar, re-exported from `lib.rs`.

## Design notes

- Blocking `stop_recording`/`transcribe` calls are acceptable in M1. M3 can move slow work off the event thread without changing `next`, because the core only sees completion events.
- Effects are data, so tests assert on `(State, Vec<Effect>)` directly without a facade.
- Move `FakeFacade` from `facade.rs` tests into a shared `#[cfg(test)]` module so the core tests can reuse it.

## Testing

- A table-driven test over every row of the transition table.
- A property-style test: every `(state, event)` pair returns without panicking.
- Integration tests with `FakeFacade`: a full dictation, a full rewrite, rewrite with no selection, a failure in each facade method, quit while recording, and a hotkey ignored while busy.

## Acceptance criteria

- [ ] `next` covers every `(State, Event)` combination.
- [ ] Dictate and rewrite flows pass end to end against `FakeFacade`.
- [ ] Every `FacadeError` variant produces a notification and a return to `Idle`.
- [ ] Coverage ≥ 95%; clippy clean.

## Implementation notes

Decisions made while building M1:

- `Effect::DiscardRecording` stops the microphone and ignores the result. It's used on quit and on a failure while recording, so releasing the mic can't cause a second error report.
- `SetTray` and `Notify` are best effort: their errors are dropped. This guarantees that handling a failure always terminates.
- A blank selection is treated like no selection, and a blank rewrite result is never inserted. The user is notified and their text is left unchanged.
- The module is `src/app/` rather than `src/core/`, to avoid clashing with Rust's built-in `core` crate.

## Open questions

- Should `Error` be shown on the tray for a fixed duration? That needs a timer event, so it's deferred to M7.
