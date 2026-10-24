//! Turns raw model output into text that can replace the selection, or rejects it.

use std::fmt;

/// Why model output can't be used. The original text is then left untouched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputError {
    Empty,
    /// Far longer than the input: the model rambled or answered instead of rewriting.
    TooLong,
}

impl fmt::Display for OutputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "the model returned no text"),
            Self::TooLong => write!(f, "the model returned far more text than it was given"),
        }
    }
}

impl std::error::Error for OutputError {}

/// Output may be at most this many times the input length (plus [`SLACK`]).
const MAX_GROWTH: usize = 3;
/// Extra characters allowed, so very short inputs can still grow a little.
const SLACK: usize = 20;

/// Opening lines models sometimes add despite being told not to.
const PREAMBLES: [&str; 8] = [
    "here is",
    "here's",
    "sure",
    "certainly",
    "rewritten text",
    "rewritten version",
    "revised text",
    "corrected text",
];

/// Cleans `raw` model output for replacing `original`:
///
/// - drops a think block, `<text>` tags, a preamble line ("Here is the rewrite:"),
///   spaces at line ends, and quotes or code fences wrapped around the whole answer
///   that the original didn't have;
/// - keeps the original's leading and trailing whitespace, so replacing a selection
///   that ends in a newline doesn't join two lines.
pub fn clean_output(raw: &str, original: &str) -> Result<String, OutputError> {
    let body = [
        without_think_block,
        without_text_tags,
        without_preamble,
        without_line_end_spaces,
        |text: &str| without_wrapper(text, "```", "```"),
    ]
    .into_iter()
    .fold(raw.trim().to_owned(), |text, step| {
        step(&text).trim().to_owned()
    });
    let original_body = original.trim();
    let body = [("\"", "\""), ("“", "”"), ("'", "'"), ("«", "»")]
        .into_iter()
        .filter(|(open, close)| {
            !(original_body.starts_with(open) && original_body.ends_with(close))
        })
        .fold(body, |text, (open, close)| {
            without_wrapper(&text, open, close).trim().to_owned()
        });
    if body.is_empty() {
        return Err(OutputError::Empty);
    }
    if body.chars().count() > original_body.chars().count() * MAX_GROWTH + SLACK {
        return Err(OutputError::TooLong);
    }
    let leading = &original[..original.len() - original.trim_start().len()];
    let trailing = &original[original.trim_end().len()..];
    Ok(format!("{leading}{body}{trailing}"))
}

fn without_think_block(text: &str) -> String {
    text.find("</think>").map_or_else(
        || text.to_owned(),
        |end| text[end + "</think>".len()..].to_owned(),
    )
}

/// Drops spaces at line ends, such as Markdown's two-space line break.
fn without_line_end_spaces(text: &str) -> String {
    text.lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
}

fn without_text_tags(text: &str) -> String {
    text.replace("<text>", "").replace("</text>", "")
}

/// Drops the first line if it is a preamble ending in a colon.
fn without_preamble(text: &str) -> String {
    let (first, rest) = text.split_once('\n').unwrap_or((text, ""));
    let lower = first.trim().to_lowercase();
    let is_preamble =
        lower.ends_with(':') && PREAMBLES.iter().any(|preamble| lower.starts_with(preamble));
    if is_preamble {
        rest.to_owned()
    } else {
        text.to_owned()
    }
}

/// Removes `open`…`close` around the whole text, including a code fence's language.
fn without_wrapper(text: &str, open: &str, close: &str) -> String {
    text.strip_prefix(open)
        .and_then(|inner| inner.strip_suffix(close))
        .filter(|inner| !inner.contains(open) || open != close)
        .map_or_else(
            || text.to_owned(),
            |inner| {
                if open == "```" {
                    // Skip a language tag such as ```text on the opening line.
                    inner
                        .split_once('\n')
                        .map_or(inner, |(_, code)| code)
                        .to_owned()
                } else {
                    inner.to_owned()
                }
            },
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clean(raw: &str) -> Result<String, OutputError> {
        clean_output(raw, "Original text that is long enough for most answers.")
    }

    #[test]
    fn keeps_plain_output() {
        assert_eq!(clean("  Fixed text.\n"), Ok("Fixed text.".into()));
    }

    #[test]
    fn strips_think_blocks_tags_and_preambles() {
        let cases = [
            "<think>\nreasoning\n</think>\n\nFixed text.",
            "<text>\nFixed text.\n</text>",
            "Here is the rewritten text:\nFixed text.",
            "Sure! Here's a clearer version:\n\nFixed text.",
            "Rewritten text:\nFixed text.",
        ];

        cases
            .iter()
            .for_each(|raw| assert_eq!(clean(raw), Ok("Fixed text.".into()), "{raw:?}"));
    }

    #[test]
    fn keeps_a_first_line_that_only_looks_like_a_preamble() {
        assert_eq!(
            clean("Here is the plan:\n1. Ship it."),
            Ok("1. Ship it.".into()),
            "a colon line starting like a preamble is dropped"
        );
        assert_eq!(
            clean("Sure thing, see you at 5."),
            Ok("Sure thing, see you at 5.".into()),
            "no colon, so it is content"
        );
    }

    #[test]
    fn strips_wrappers_the_original_did_not_have() {
        let cases = [
            "\"Fixed text.\"",
            "“Fixed text.”",
            "'Fixed text.'",
            "«Fixed text.»",
            "```\nFixed text.\n```",
            "```text\nFixed text.\n```",
        ];

        cases
            .iter()
            .for_each(|raw| assert_eq!(clean(raw), Ok("Fixed text.".into()), "{raw:?}"));
    }

    #[test]
    fn keeps_wrappers_the_original_had() {
        assert_eq!(
            clean_output("\"Be brief.\"", "\"be breif\""),
            Ok("\"Be brief.\"".into())
        );
        assert_eq!(
            clean_output("```\nlet x = 1;\n```", "```\nlet x=1\n```"),
            Ok("let x = 1;".into()),
            "code fences are always unwrapped"
        );
    }

    #[test]
    fn keeps_inner_quotes() {
        assert_eq!(
            clean("\"Yes,\" she said, \"now.\""),
            Ok("\"Yes,\" she said, \"now.\"".into()),
            "quotes inside mean the outer ones are part of the text"
        );
        assert_eq!(clean("He said \"hi\"."), Ok("He said \"hi\".".into()));
    }

    #[test]
    fn drops_spaces_at_line_ends() {
        assert_eq!(clean("Thanks,  \nDima"), Ok("Thanks,\nDima".into()));
    }

    #[test]
    fn restores_the_original_surrounding_whitespace() {
        assert_eq!(
            clean_output("Fixed line.", "  teh line\n"),
            Ok("  Fixed line.\n".into())
        );
    }

    #[test]
    fn rejects_empty_or_runaway_output() {
        assert_eq!(clean("  \n"), Err(OutputError::Empty));
        assert_eq!(
            clean("<think>only thoughts</think>"),
            Err(OutputError::Empty)
        );
        assert_eq!(
            clean_output("Hi there, friend!", "hi"),
            Ok("Hi there, friend!".into())
        );
        assert_eq!(
            clean_output(&"word ".repeat(10), "hi"),
            Err(OutputError::TooLong)
        );
    }

    #[test]
    fn errors_explain_the_problem() {
        assert_eq!(OutputError::Empty.to_string(), "the model returned no text");
        assert_eq!(
            OutputError::TooLong.to_string(),
            "the model returned far more text than it was given"
        );
    }
}
