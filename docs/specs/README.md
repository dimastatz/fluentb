# fluentb milestone specs

One PRD per milestone. Each milestone ends in something demoable and keeps CI green
(fmt, clippy `-D warnings`, line coverage ≥ 95%).

| #   | Milestone                                              | Status  | Depends on |
| --- | ------------------------------------------------------ | ------- | ---------- |
| M0  | [Foundation](m00-foundation.md)                        | Done    | —          |
| M1  | [Pure state manager](m01-state-manager.md)             | Done    | M0         |
| M2  | [Configuration](m02-configuration.md)                  | Done    | M0         |
| M3  | [macOS dictation end to end](m03-macos-dictation.md)   | Done      | M1, M2     |
| M4  | [Tray app shell](m04-tray-shell.md)                    | Done      | M3       |
| M5  | [Rewrite](m05-rewrite.md)                              | Done      | M3, M4   |
| M6  | [Model management and first run](m06-model-onboarding.md) | Done      | M3, M4 |
| M7  | [Polish and robustness](m07-polish.md)                 | In review | M5, M6   |
| M8  | [macOS distribution](m08-macos-distribution.md)        | Planned | M7         |
| M9  | [Windows support](m09-windows.md)                      | Planned | M8         |
| M10 | [Linux support](m10-linux.md)                          | Planned | M8         |

## PRD template

Each spec uses the same sections: Summary, Goals, Non-goals, User stories,
Requirements (functional and non-functional), Design notes, Testing, Acceptance
criteria, Open questions.

## Cross-cutting rules

These apply to every milestone (see `CLAUDE.md`):

- Audio and user text never leave the machine: transcription (Whisper) and rewrite (local LLM via llama.cpp) both run on-device.
- Models are never bundled; they are downloaded and verified on first run (M6).
- All side effects go through the `Facade` trait; the core stays pure.
- Every design assumes several OS implementations. See [Platform architecture](#platform-architecture).

## Platform architecture

These requirements apply to every milestone that touches the OS:

1. **Strategy.** The `Facade` trait is the only interface between the core and the OS. Every OS-dependent capability is a `Facade` method that uses OS-neutral types. The core takes `&mut dyn Facade` and never knows which OS it runs on.
2. **Adapters in one place.** Each OS has one adapter module under `src/platform/`: `platform::macos`, `platform::windows` and `platform::linux`. All OS-specific code and OS-only dependencies belong to that adapter.
3. **One selection point.** `platform::run()` selects the adapter for the OS the binary is built for, at compile time. `main` calls only `platform::run()`. No non-test code outside `src/platform/` uses `#[cfg(target_os)]` or `cfg!` on the OS.
4. **Pure, shared logic.** OS-dependent decisions that need no OS API (key mapping, file paths, backend selection) are pure functions that take an `Os` value, so they are tested on every CI runner. Adapters stay thin and are tested by hand.
5. **New features cover every OS.** When a feature adds a `Facade` method, every adapter implements it. An OS that can't support it yet returns a `FacadeError` with a clear message, rather than the core special-casing that OS.
6. **New OSes don't touch the core.** Adding an OS means adding an adapter and one branch in `platform::run()`; `src/app/`, `src/config/` and the other adapters stay unchanged.
- No `unwrap`/`expect` outside tests; no shared mutable state unless an OS API forces it.
