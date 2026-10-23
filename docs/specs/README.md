# fluentb milestone specs

One PRD per milestone. Each milestone ends in something demoable and keeps CI green
(fmt, clippy `-D warnings`, line coverage ≥ 95%).

| #   | Milestone                                              | Status  | Depends on |
| --- | ------------------------------------------------------ | ------- | ---------- |
| M0  | [Foundation](m00-foundation.md)                        | Done    | —          |
| M1  | [Pure state manager](m01-state-manager.md)             | Done    | M0         |
| M2  | [Configuration](m02-configuration.md)                  | Done    | M0         |
| M3  | [macOS dictation end to end](m03-macos-dictation.md)   | In review | M1, M2     |
| M4  | [Tray app shell](m04-tray-shell.md)                    | Planned | M3         |
| M5  | [Rewrite](m05-rewrite.md)                              | Planned | M3, M4     |
| M6  | [Model management and first run](m06-model-onboarding.md) | Planned | M3, M4  |
| M7  | [Polish and robustness](m07-polish.md)                 | Planned | M5, M6     |
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
- OS-specific code lives behind platform modules (`platform::macos`, `platform::windows`, `platform::linux`).
- No `unwrap`/`expect` outside tests; no shared mutable state unless an OS API forces it.
