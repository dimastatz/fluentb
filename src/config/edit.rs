//! Changes one setting in the config file's text, keeping comments and layout.

/// Sets `key` in `[section]` to the string `value`. An existing line for the key is
/// replaced; otherwise the key is added at the top of the section, and the section
/// is appended if the file has none.
#[must_use]
pub fn set_string(text: &str, section: &str, key: &str, value: &str) -> String {
    let setting = format!("{key} = {}", quoted(value));
    let lines: Vec<&str> = text.lines().collect();
    let header = format!("[{section}]");
    let Some(start) = lines.iter().position(|line| line.trim() == header) else {
        let separator = if text.is_empty() || text.ends_with("\n\n") {
            ""
        } else if text.ends_with('\n') {
            "\n"
        } else {
            "\n\n"
        };
        return format!("{text}{separator}{header}\n{setting}\n");
    };
    let end = lines[start + 1..]
        .iter()
        .position(|line| line.trim_start().starts_with('['))
        .map_or(lines.len(), |offset| start + 1 + offset);
    let existing = (start + 1..end).find(|&index| is_key_line(lines[index], key));
    let edited: Vec<String> = match existing {
        Some(index) => lines
            .iter()
            .enumerate()
            .map(|(i, line)| {
                if i == index {
                    setting.clone()
                } else {
                    (*line).to_owned()
                }
            })
            .collect(),
        None => lines[..=start]
            .iter()
            .map(|line| (*line).to_owned())
            .chain([setting])
            .chain(lines[start + 1..].iter().map(|line| (*line).to_owned()))
            .collect(),
    };
    let newline = if text.ends_with('\n') { "\n" } else { "" };
    format!("{}{newline}", edited.join("\n"))
}

/// `value` as a TOML basic string.
fn quoted(value: &str) -> String {
    let escaped: String = value
        .chars()
        .map(|c| match c {
            '\\' => "\\\\".to_owned(),
            '"' => "\\\"".to_owned(),
            c if c.is_control() => format!("\\u{:04X}", u32::from(c)),
            c => c.to_string(),
        })
        .collect();
    format!("\"{escaped}\"")
}

/// Whether `line` assigns `key` (ignoring comments).
fn is_key_line(line: &str, key: &str) -> bool {
    line.trim_start()
        .strip_prefix(key)
        .is_some_and(|rest| rest.trim_start().starts_with('='))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{parse, Choice, RewriteModel, DEFAULT_FILE};
    use crate::platform::Os;

    #[test]
    fn replaces_a_value_and_keeps_comments() {
        let edited = set_string(DEFAULT_FILE, "rewrite", "model", "gemma3-4b");

        let (config, warnings) = parse(&edited, Os::MacOs);
        assert_eq!(warnings, []);
        assert_eq!(config.rewrite.model, Choice::Fixed(RewriteModel::Gemma3_4B));
        assert_eq!(edited.lines().count(), DEFAULT_FILE.lines().count());
        assert!(edited.contains("# The local model that rewrites text"));
        assert!(
            edited.contains("[dictation]\n# auto"),
            "other sections untouched"
        );
        assert!(
            edited.contains("model = \"auto\""),
            "dictation.model untouched"
        );
    }

    #[test]
    fn only_touches_the_named_section() {
        let text = "[dictation]\nmodel = \"auto\"\n\n[rewrite]\nmodel = \"auto\"\n";

        assert_eq!(
            set_string(text, "dictation", "model", "small"),
            "[dictation]\nmodel = \"small\"\n\n[rewrite]\nmodel = \"auto\"\n"
        );
    }

    #[test]
    fn ignores_commented_and_similar_keys() {
        let text = "[rewrite]\n# model = \"old\"\nmodels = 1\n";

        assert_eq!(
            set_string(text, "rewrite", "model", "qwen3-4b"),
            "[rewrite]\nmodel = \"qwen3-4b\"\n# model = \"old\"\nmodels = 1\n"
        );
    }

    #[test]
    fn adds_a_missing_section() {
        assert_eq!(
            set_string("version = 1\n", "rewrite", "model", "qwen3-4b"),
            "version = 1\n\n[rewrite]\nmodel = \"qwen3-4b\"\n"
        );
        assert_eq!(
            set_string("version = 1", "rewrite", "model", "x"),
            "version = 1\n\n[rewrite]\nmodel = \"x\"\n"
        );
        assert_eq!(
            set_string("", "rewrite", "model", "x"),
            "[rewrite]\nmodel = \"x\"\n"
        );
        assert_eq!(
            set_string("a = 1\n\n", "rewrite", "model", "x"),
            "a = 1\n\n[rewrite]\nmodel = \"x\"\n"
        );
    }

    #[test]
    fn escapes_the_value() {
        let edited = set_string("[s]\n", "s", "k", "a\"b\\c\nd");

        assert_eq!(edited, "[s]\nk = \"a\\\"b\\\\c\\u000Ad\"\n");
        let table: toml::Table = edited.parse().unwrap();
        assert_eq!(table["s"]["k"].as_str(), Some("a\"b\\c\nd"));
    }

    #[test]
    fn keeps_a_missing_final_newline_missing() {
        assert_eq!(set_string("[s]\nk = 1", "s", "k", "v"), "[s]\nk = \"v\"");
    }
}
