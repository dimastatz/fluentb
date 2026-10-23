//! Builds `AppleScript` snippets for `osascript`.

/// Quotes `text` as an `AppleScript` string literal.
#[must_use]
pub fn string_literal(text: &str) -> String {
    let escaped: String = text
        .chars()
        .map(|c| match c {
            '\\' => "\\\\".to_owned(),
            '"' => "\\\"".to_owned(),
            '\n' => "\\n".to_owned(),
            '\r' => "\\r".to_owned(),
            '\t' => "\\t".to_owned(),
            other => other.to_string(),
        })
        .collect();
    format!("\"{escaped}\"")
}

/// A script that shows `message` as a macOS notification titled `title`.
#[must_use]
pub fn notification(title: &str, message: &str) -> String {
    format!(
        "display notification {} with title {}",
        string_literal(message),
        string_literal(title)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_quotes_backslashes_and_control_characters() {
        assert_eq!(string_literal("plain"), "\"plain\"");
        assert_eq!(
            string_literal("say \"hi\"\\\n\tok\r"),
            "\"say \\\"hi\\\"\\\\\\n\\tok\\r\""
        );
    }

    #[test]
    fn builds_a_notification_script() {
        assert_eq!(
            notification("fluentb", "Model \"base\" missing"),
            "display notification \"Model \\\"base\\\" missing\" with title \"fluentb\""
        );
    }
}
