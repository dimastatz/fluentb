//! Reads facts about the machine from command output, for choosing default models.

/// The first language in the output of `defaults read -g AppleLanguages`, e.g.
/// `(\n    "en-US",\n    "he-IL"\n)` gives `en-US`.
#[must_use]
pub fn first_language(apple_languages: &str) -> Option<String> {
    apple_languages
        .lines()
        .map(|line| line.trim().trim_end_matches(',').trim_matches('"'))
        .find(|line| !line.is_empty() && *line != "(" && *line != ")")
        .map(str::to_owned)
}

/// A byte count printed by `sysctl -n hw.memsize`.
#[must_use]
pub fn memory_bytes(sysctl: &str) -> Option<u64> {
    sysctl.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_preferred_language() {
        assert_eq!(
            first_language("(\n    \"en-US\",\n    \"he-IL\"\n)\n").as_deref(),
            Some("en-US")
        );
        assert_eq!(first_language("(\n    he\n)").as_deref(), Some("he"));
        assert_eq!(first_language("(\n)\n"), None);
        assert_eq!(first_language(""), None);
    }

    #[test]
    fn reads_the_memory_size() {
        assert_eq!(memory_bytes("17179869184\n"), Some(17_179_869_184));
        assert_eq!(memory_bytes("unknown"), None);
    }
}
