//! Turns config file text into a [`Config`], keyed by key with per-key fallback.

use super::{Config, ConfigWarning, Dictation, Hotkeys, Rewrite};
use crate::accelerator;
use crate::platform::Os;
use toml::{Table, Value};

type Warnings = Vec<ConfigWarning>;

const SUPPORTED_VERSION: i64 = 1;
const TOP_LEVEL_KEYS: [&str; 4] = ["version", "hotkeys", "dictation", "rewrite"];
const HOTKEY_KEYS: [&str; 2] = ["dictate", "rewrite"];
const DICTATION_KEYS: [&str; 3] = ["model", "language", "mode"];
const REWRITE_KEYS: [&str; 5] = [
    "style",
    "model",
    "keep_loaded",
    "unload_after_minutes",
    "max_characters",
];

/// Parses config file text. Never fails: a syntax error yields all defaults, and an
/// invalid or missing value yields that value's default. Every problem comes back as
/// a [`ConfigWarning`].
#[must_use]
pub fn parse(text: &str, os: Os) -> (Config, Warnings) {
    let defaults = Config::defaults(os);
    match text.parse::<Table>() {
        Ok(root) => from_table(&root, defaults, os),
        Err(error) => (
            defaults,
            vec![ConfigWarning::Syntax(error.message().to_owned())],
        ),
    }
}

fn from_table(root: &Table, defaults: Config, os: Os) -> (Config, Warnings) {
    let (hotkeys_table, hotkeys_section) = section(root, "hotkeys", &HOTKEY_KEYS);
    let (dictation_table, dictation_section) = section(root, "dictation", &DICTATION_KEYS);
    let (rewrite_table, rewrite_section) = section(root, "rewrite", &REWRITE_KEYS);

    let parse_accelerator =
        |text: &str| accelerator::parse(text, os).map_err(|error| error.to_string());
    let (dictate, dictate_warnings) = field(
        hotkeys_table,
        "hotkeys.dictate",
        defaults.hotkeys.dictate,
        parse_accelerator,
    );
    let (rewrite, rewrite_warnings) = field(
        hotkeys_table,
        "hotkeys.rewrite",
        defaults.hotkeys.rewrite,
        parse_accelerator,
    );
    let (hotkeys, conflict) = without_conflict(Hotkeys { dictate, rewrite }, os);

    let (model, whisper_warnings) = field(
        dictation_table,
        "dictation.model",
        defaults.dictation.model,
        str::parse,
    );
    let (language, language_warnings) = field(
        dictation_table,
        "dictation.language",
        defaults.dictation.language,
        str::parse,
    );
    let (mode, mode_warnings) = field(
        dictation_table,
        "dictation.mode",
        defaults.dictation.mode,
        str::parse,
    );
    let (rewrite, rewrite_settings_warnings) = rewrite_settings(rewrite_table, defaults.rewrite);

    let config = Config {
        hotkeys,
        dictation: Dictation {
            model,
            language,
            mode,
        },
        rewrite,
    };
    let warnings = [
        unknown_keys(root, None, &TOP_LEVEL_KEYS),
        version(root),
        hotkeys_section,
        dictate_warnings,
        rewrite_warnings,
        conflict,
        dictation_section,
        whisper_warnings,
        language_warnings,
        mode_warnings,
        rewrite_section,
        rewrite_settings_warnings,
    ]
    .concat();
    (config, warnings)
}

/// Reads the `[rewrite]` settings.
fn rewrite_settings(table: Option<&Table>, defaults: Rewrite) -> (Rewrite, Warnings) {
    let (style, style_warnings) = field(table, "rewrite.style", defaults.style, str::parse);
    let (rewrite_model, rewrite_model_warnings) =
        field(table, "rewrite.model", defaults.model, str::parse);
    let (keep_loaded, keep_loaded_warnings) =
        value_field(table, "rewrite.keep_loaded", defaults.keep_loaded, boolean);
    let (unload_after_minutes, unload_warnings) = value_field(
        table,
        "rewrite.unload_after_minutes",
        defaults.unload_after_minutes,
        |value| positive(value, 24 * 60),
    );
    let (max_characters, max_characters_warnings) = value_field(
        table,
        "rewrite.max_characters",
        defaults.max_characters,
        |value| positive(value, 100_000),
    );

    let rewrite = Rewrite {
        style,
        model: rewrite_model,
        keep_loaded,
        unload_after_minutes,
        max_characters,
    };
    let warnings = [
        style_warnings,
        rewrite_model_warnings,
        keep_loaded_warnings,
        unload_warnings,
        max_characters_warnings,
    ]
    .concat();
    (rewrite, warnings)
}

/// Looks up a `[section]` table, warning if it isn't a table or has unknown keys.
fn section<'a>(root: &'a Table, name: &str, known: &[&str]) -> (Option<&'a Table>, Warnings) {
    match root.get(name) {
        None => (None, Vec::new()),
        Some(Value::Table(table)) => (Some(table), unknown_keys(table, Some(name), known)),
        Some(_) => (None, vec![invalid(name, "expected a table")]),
    }
}

fn unknown_keys(table: &Table, section: Option<&str>, known: &[&str]) -> Warnings {
    table
        .keys()
        .filter(|key| !known.contains(&key.as_str()))
        .map(|key| {
            ConfigWarning::UnknownKey(section.map_or_else(|| key.clone(), |s| format!("{s}.{key}")))
        })
        .collect()
}

fn version(root: &Table) -> Warnings {
    let reason = match root.get("version") {
        None | Some(Value::Integer(SUPPORTED_VERSION)) => return Vec::new(),
        Some(Value::Integer(other)) => {
            format!("unsupported version {other} (expected {SUPPORTED_VERSION})")
        }
        Some(_) => "expected an integer".to_owned(),
    };
    vec![invalid("version", &reason)]
}

/// Reads one string setting, falling back to `default` if it is absent or invalid.
fn field<T>(
    table: Option<&Table>,
    key: &str,
    default: T,
    parse: impl Fn(&str) -> Result<T, String>,
) -> (T, Warnings) {
    value_field(table, key, default, |value| match value {
        Value::String(text) => parse(text),
        _ => Err("expected a string".into()),
    })
}

/// Reads one setting of any TOML type, falling back to `default` if it is absent or
/// invalid.
fn value_field<T>(
    table: Option<&Table>,
    key: &str,
    default: T,
    parse: impl Fn(&Value) -> Result<T, String>,
) -> (T, Warnings) {
    let name = key.rsplit('.').next().unwrap_or(key);
    match table.and_then(|table| table.get(name)) {
        None => (default, Vec::new()),
        Some(value) => parse(value).map_or_else(
            |reason| (default, vec![invalid(key, &reason)]),
            |value| (value, Vec::new()),
        ),
    }
}

fn boolean(value: &Value) -> Result<bool, String> {
    match value {
        Value::Boolean(flag) => Ok(*flag),
        _ => Err("expected true or false".into()),
    }
}

/// A whole number from 1 to `max`.
fn positive<T: TryFrom<i64>>(value: &Value, max: i64) -> Result<T, String> {
    match value {
        Value::Integer(n) if (1..=max).contains(n) => {
            T::try_from(*n).map_err(|_| format!("expected a whole number from 1 to {max}"))
        }
        _ => Err(format!("expected a whole number from 1 to {max}")),
    }
}

/// Falls back to the default hotkeys if Dictate and Rewrite share one.
fn without_conflict(hotkeys: Hotkeys, os: Os) -> (Hotkeys, Warnings) {
    if hotkeys.dictate == hotkeys.rewrite {
        (Hotkeys::defaults(os), vec![ConfigWarning::HotkeyConflict])
    } else {
        (hotkeys, Vec::new())
    }
}

fn invalid(key: &str, reason: &str) -> ConfigWarning {
    ConfigWarning::InvalidValue {
        key: key.into(),
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accelerator::{Key, Modifier};
    use crate::config::{
        Choice, DictationMode, Language, ModelSize, RewriteModel, Style, WhisperModel, DEFAULT_FILE,
    };

    fn parse_mac(text: &str) -> (Config, Warnings) {
        parse(text, Os::MacOs)
    }

    #[test]
    fn empty_file_gives_defaults() {
        assert_eq!(parse_mac(""), (Config::defaults(Os::MacOs), Vec::new()));
    }

    #[test]
    fn default_file_gives_defaults_on_every_os() {
        [Os::MacOs, Os::Windows, Os::Linux]
            .into_iter()
            .for_each(|os| {
                assert_eq!(
                    parse(DEFAULT_FILE, os),
                    (Config::defaults(os), Vec::new()),
                    "{os:?}"
                );
            });
    }

    #[test]
    fn reads_every_setting() {
        let text = r#"
            version = 1

            [hotkeys]
            dictate = "Option+Space"
            rewrite = "Ctrl+Alt+R"

            [dictation]
            model = "small"
            language = "he"
            mode = "push-to-talk"

            [rewrite]
            style = "concise"
            model = "gemma3-4b"
            keep_loaded = true
            unload_after_minutes = 30
            max_characters = 8000
        "#;

        let (config, warnings) = parse_mac(text);

        assert_eq!(warnings, []);
        assert_eq!(
            config.hotkeys.dictate,
            accelerator::parse("Alt+Space", Os::MacOs).unwrap()
        );
        assert_eq!(
            config.hotkeys.rewrite.modifiers(),
            [Modifier::Ctrl, Modifier::Alt]
        );
        assert_eq!(config.hotkeys.rewrite.key(), Key::Char('R'));
        assert_eq!(
            config.dictation,
            Dictation {
                model: Choice::Fixed(WhisperModel {
                    size: ModelSize::Small,
                    english_only: false
                }),
                language: Language::Code("he".into()),
                mode: DictationMode::PushToTalk,
            }
        );
        assert_eq!(
            config.rewrite,
            Rewrite {
                style: Style::Concise,
                model: Choice::Fixed(RewriteModel::Gemma3_4B),
                keep_loaded: true,
                unload_after_minutes: 30,
                max_characters: 8000,
            }
        );
    }

    #[test]
    fn partial_file_keeps_other_defaults() {
        let (config, warnings) = parse_mac("[rewrite]\nstyle = \"formal\"\n");

        assert_eq!(warnings, []);
        assert_eq!(
            config,
            Config {
                rewrite: Rewrite {
                    style: Style::Formal,
                    ..Config::defaults(Os::MacOs).rewrite
                },
                ..Config::defaults(Os::MacOs)
            }
        );
    }

    #[test]
    fn invalid_value_falls_back_for_that_key_only() {
        let text = "[dictation]\nmodel = \"huge\"\nlanguage = \"de\"\n";

        let (config, warnings) = parse_mac(text);

        assert_eq!(
            config.dictation.model,
            Config::defaults(Os::MacOs).dictation.model
        );
        assert_eq!(config.dictation.language, Language::Code("de".into()));
        assert!(matches!(
            warnings.as_slice(),
            [ConfigWarning::InvalidValue { key, .. }] if key == "dictation.model"
        ));
    }

    #[test]
    fn reports_each_invalid_value() {
        let text = r#"
            [hotkeys]
            dictate = "Shift+D"
            rewrite = 5

            [dictation]
            language = "english"
            mode = "hold"

            [rewrite]
            style = "pirate"
            model = "qwen2.5-3b"
            keep_loaded = "yes"
            unload_after_minutes = 0
            max_characters = 1.5
        "#;

        let (config, warnings) = parse_mac(text);

        assert_eq!(config, Config::defaults(Os::MacOs));
        assert_eq!(
            warnings,
            [
                invalid("hotkeys.dictate", "hotkey needs Ctrl, Alt, or Cmd/Super"),
                invalid("hotkeys.rewrite", "expected a string"),
                invalid(
                    "dictation.language",
                    "unknown language \"english\" (expected \"auto\" or a two-letter code such as \"en\")"
                ),
                invalid(
                    "dictation.mode",
                    "unknown mode \"hold\" (expected toggle or push-to-talk)"
                ),
                invalid(
                    "rewrite.style",
                    "unknown style \"pirate\" (expected neutral, casual, formal, or concise)"
                ),
                invalid(
                    "rewrite.model",
                    "unknown rewrite model \"qwen2.5-3b\" (expected auto, qwen3-4b, gemma3-4b, or qwen3-1.7b)"
                ),
                invalid("rewrite.keep_loaded", "expected true or false"),
                invalid(
                    "rewrite.unload_after_minutes",
                    "expected a whole number from 1 to 1440"
                ),
                invalid(
                    "rewrite.max_characters",
                    "expected a whole number from 1 to 100000"
                ),
            ]
        );
    }

    #[test]
    fn warns_about_unknown_keys_and_sections() {
        let text = "theme = \"dark\"\n[hotkeys]\ncancel = \"Esc\"\n[extras]\n";

        let (config, warnings) = parse_mac(text);

        assert_eq!(config, Config::defaults(Os::MacOs));
        assert_eq!(
            warnings,
            [
                ConfigWarning::UnknownKey("extras".into()),
                ConfigWarning::UnknownKey("theme".into()),
                ConfigWarning::UnknownKey("hotkeys.cancel".into()),
            ]
        );
    }

    #[test]
    fn section_that_is_not_a_table_is_ignored() {
        let (config, warnings) = parse_mac("rewrite = \"formal\"\n");

        assert_eq!(config, Config::defaults(Os::MacOs));
        assert_eq!(warnings, [invalid("rewrite", "expected a table")]);
    }

    #[test]
    fn unsupported_version_is_reported() {
        let (_, warnings) = parse_mac("version = 2\n");
        assert_eq!(
            warnings,
            [invalid("version", "unsupported version 2 (expected 1)")]
        );

        let (_, warnings) = parse_mac("version = \"1\"\n");
        assert_eq!(warnings, [invalid("version", "expected an integer")]);
    }

    #[test]
    fn conflicting_hotkeys_fall_back_to_defaults() {
        let text = "[hotkeys]\ndictate = \"Ctrl+Alt+D\"\nrewrite = \"alt+ctrl+d\"\n";

        let (config, warnings) = parse(text, Os::Linux);

        assert_eq!(config.hotkeys, Hotkeys::defaults(Os::Linux));
        assert_eq!(warnings, [ConfigWarning::HotkeyConflict]);
    }

    #[test]
    fn syntax_error_gives_defaults() {
        let (config, warnings) = parse_mac("[hotkeys\ndictate = ");

        assert_eq!(config, Config::defaults(Os::MacOs));
        assert!(matches!(warnings.as_slice(), [ConfigWarning::Syntax(_)]));
    }
}
