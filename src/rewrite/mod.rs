//! Rewrite helpers that need no model or OS: the prompt, the output cleanup, the
//! selection limits, and when to unload the model. The engine itself lives in each
//! platform adapter.

pub mod output;
pub mod prompt;
pub mod residency;

/// Refuses selections longer than `max_characters`, with a message for the user. The
/// message never includes the text itself.
pub fn check_length(text: &str, max_characters: usize) -> Result<(), String> {
    let length = text.chars().count();
    if length > max_characters {
        Err(format!(
            "the selection is too long to rewrite ({length} characters; the limit is \
             {max_characters}). Select less text, or raise rewrite.max_characters in the config"
        ))
    } else {
        Ok(())
    }
}

/// The most tokens a rewrite may produce: about 1.5× the input plus a margin, so a
/// model that rambles is cut off early.
#[must_use]
pub const fn max_new_tokens(input_tokens: usize) -> usize {
    input_tokens * 3 / 2 + 64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_selections_up_to_the_limit() {
        assert_eq!(check_length("héllo", 5), Ok(()));
        let error = check_length("héllo!", 5).unwrap_err();
        assert!(error
            .starts_with("the selection is too long to rewrite (6 characters; the limit is 5)"));
        assert!(!error.contains("héllo"), "never echoes the text");
    }

    #[test]
    fn token_budget_grows_with_the_input() {
        assert_eq!(max_new_tokens(0), 64);
        assert_eq!(max_new_tokens(100), 214);
    }
}
