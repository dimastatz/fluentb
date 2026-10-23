//! Cleans up Whisper's raw segment text before it is typed into an app.

/// Joins Whisper segments into one line of text.
///
/// Drops non-speech annotations that Whisper emits, such as `[BLANK_AUDIO]`,
/// `[Music]`, or a segment that is only `(upbeat music)`, and collapses whitespace.
pub fn clean<'a>(segments: impl IntoIterator<Item = &'a str>) -> String {
    segments
        .into_iter()
        .map(str::trim)
        .filter(|segment| !is_annotation(segment))
        .map(strip_bracketed)
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// A whole segment wrapped in brackets or parentheses describes sound, not speech.
fn is_annotation(segment: &str) -> bool {
    let wrapped = |open, close| {
        segment.starts_with(open)
            && segment.ends_with(close)
            && segment[1..].find(close) == Some(segment.len() - 2)
    };
    segment.len() >= 2 && (wrapped('[', ']') || wrapped('(', ')'))
}

/// Removes `[...]` spans inside a segment; dictated speech never contains brackets.
fn strip_bracketed(segment: &str) -> String {
    segment
        .split('[')
        .enumerate()
        .map(|(i, part)| match (i, part.split_once(']')) {
            (0, _) => part.to_owned(),
            (_, Some((_, after))) => format!(" {after}"),
            // An unclosed `[` is kept as typed.
            (_, None) => format!("[{part}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_and_trims_segments() {
        assert_eq!(
            clean([" Hello there.", " How are you? "]),
            "Hello there. How are you?"
        );
    }

    #[test]
    fn drops_annotation_segments() {
        assert_eq!(clean(["[BLANK_AUDIO]"]), "");
        assert_eq!(clean(["(upbeat music)", " Okay."]), "Okay.");
        assert_eq!(clean(["[Music]", "Hi", "[ Silence ]"]), "Hi");
    }

    #[test]
    fn keeps_parentheses_inside_speech() {
        assert_eq!(
            clean(["Send it (the draft) today."]),
            "Send it (the draft) today."
        );
        assert_eq!(clean(["(one) and (two)"]), "(one) and (two)");
    }

    #[test]
    fn strips_inline_bracketed_spans() {
        assert_eq!(clean(["So [inaudible] we ship."]), "So we ship.");
        assert_eq!(clean(["Done [BLANK_AUDIO]"]), "Done");
    }

    #[test]
    fn keeps_unclosed_brackets() {
        assert_eq!(clean(["array[0"]), "array[0");
    }

    #[test]
    fn empty_input_is_empty() {
        assert_eq!(clean([]), "");
        assert_eq!(clean(["", "  "]), "");
        assert_eq!(clean(["("]), "(");
    }
}
