# M2 — Configuration

## Summary

Let users change hotkeys, transcription settings, and rewrite style through a TOML file, with safe defaults when the file is missing or invalid.

## Goals

- One typed `Config` value built by a pure parser.
- Defaults that work with no config file.
- Clear, specific error messages for invalid values.

## Non-goals

- A settings UI (the tray menu only opens the file; see M4).
- Hot reload while running (possible later; see Open questions).
- Syncing config across machines.

## User stories

- As a user, I can change the Dictate hotkey to `Option+Space` by editing one line.
- As a user, if I make a typo in the config, fluentb still starts with defaults and tells me which key was wrong.

## Requirements

### Functional

1. **Location** — platform config dir via the `directories` crate:
   - macOS: `~/Library/Application Support/fluentb/config.toml`
   - Windows: `%APPDATA%\fluentb\config.toml`
   - Linux: `$XDG_CONFIG_HOME/fluentb/config.toml`
2. **Schema** (v1):

   ```toml
   [hotkeys]
   dictate = "Cmd+Shift+D"
   rewrite = "Cmd+Shift+R"

   [dictation]
   model = "base.en"        # tiny | base | small | medium | large-v3, optional .en
   language = "auto"        # ISO 639-1 code or "auto"
   mode = "toggle"          # toggle | push-to-talk (push-to-talk implemented in M7)

   [rewrite]
   style = "neutral"        # neutral | casual | formal | concise
   ```

3. **Parsing** — `fn parse(text: &str) -> Result<Config, ConfigError>` is pure. A missing section or key falls back to its default. Unknown keys produce a warning, not an error.
4. **Accelerator validation** — `fn parse_accelerator(s: &str) -> Result<Accelerator, ConfigError>` accepts modifiers (`Cmd`/`Ctrl`/`Alt`/`Option`/`Shift`/`Super`) plus one key. It rejects empty strings, duplicates, and bindings with no modifier, and it maps `Cmd` to `Ctrl` on non-macOS.
5. **Conflict check** — Dictate and Rewrite must not share an accelerator.
6. **Loading** — `fn load(path) -> (Config, Vec<ConfigWarning>)` is the only I/O. A missing file returns defaults silently. An unreadable or invalid file returns defaults and a warning, which is shown through `Facade::notify` at startup.
7. **Bindings** — `Config::bindings() -> Vec<HotkeyBinding>` feeds `run_app` from M1.
8. **Default file** — if no file exists, write a commented default file on first run so users can discover the options.

### Non-functional

- `serde` + `toml` for parsing. No other new dependencies besides `directories`.
- A `version = 1` key is reserved for future migrations.

## Testing

- Parse the default file, an empty file, partial files, and every invalid value.
- Accelerator table tests: valid, invalid, and platform mapping.
- Use a temp dir in `load` tests for the missing-file, unreadable-file, and valid-file cases.

## Acceptance criteria

- [ ] The app starts with no config file and uses the defaults.
- [ ] An invalid value produces a notification naming the key and falls back to its default.
- [ ] Conflicting hotkeys are rejected with a clear message.
- [ ] Coverage ≥ 95%.

## Open questions

- Hot reload: watch the file and send a `ConfigChanged` event to the core? Deferred until users ask for it.
