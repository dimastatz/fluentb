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
const REWRITE_KEYS: [&str; 1] = ["style"];

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
    let (style, style_warnings) = field(
        rewrite_table,
        "rewrite.style",
        defaults.rewrite.style,
        str::parse,
    );

    let config = Config {
        hotkeys,
        dictation: Dictation {
            model,
            language,
            mode,
        },
        rewrite: Rewrite { style },
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
        style_warnings,
    ]
    .concat();
    (config, warnings)
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
    let name = key.rsplit('.').next().unwrap_or(key);
    match table.and_then(|table| table.get(name)) {
        None => (default, Vec::new()),
        Some(Value::String(text)) => parse(text).map_or_else(
            |reason| (default, vec![invalid(key, &reason)]),
            |value| (value, Vec::new()),
        ),
        Some(_) => (default, vec![invalid(key, "expected a string")]),
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
    use crate::config::{DictationMode, Language, ModelSize, Style, WhisperModel, DEFAULT_FILE};

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
                model: WhisperModel {
                    size: ModelSize::Small,
                    english_only: false
                },
                language: Language::Code("he".into()),
                mode: DictationMode::PushToTalk,
            }
        );
        assert_eq!(config.rewrite.style, Style::Concise);
    }

    #[test]
    fn partial_file_keeps_other_defaults() {
        let (config, warnings) = parse_mac("[rewrite]\nstyle = \"formal\"\n");

        assert_eq!(warnings, []);
        assert_eq!(
            config,
            Config {
                rewrite: Rewrite {
                    style: Style::Formal
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
