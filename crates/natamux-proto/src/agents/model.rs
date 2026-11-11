//! The model of a task: typed by the person, passed to the task's agent as `--model <name>`.
//! There is no list of known models: the agent itself says when a name is wrong.

/// The longest model name the daemon takes.
pub const MAX_MODEL_LENGTH: usize = 100;

/// What the app shows under a model that `is_model_name` refuses.
pub const MODEL_NAME_ERROR: &str = "Model names use letters, digits and . _ - : / @ [ ] only";

/// A name that is safe as one word of a command line: 1 to `MAX_MODEL_LENGTH` characters of
/// `A-Z a-z 0-9 . _ - : / @ [ ]`, and no leading `-`, which the agent would read as a flag.
pub fn is_model_name(name: &str) -> bool {
    let allowed = |c: char| c.is_ascii_alphanumeric() || "._-:/@[]".contains(c);
    !name.is_empty()
        && name.chars().count() <= MAX_MODEL_LENGTH
        && !name.starts_with('-')
        && name.chars().all(allowed)
}

/// The model in the words of an agent's command line: `--model x`, `--model=x` or `-m x`.
pub fn model_of(args: Option<&str>) -> Option<String> {
    let words: Vec<&str> = args?.split_whitespace().collect();
    for (index, word) in words.iter().enumerate() {
        if let Some(model) = word.strip_prefix("--model=") {
            return (!model.is_empty()).then(|| model.to_string());
        }
        if *word == "--model" || *word == "-m" {
            return words
                .get(index + 1)
                .filter(|next| !next.starts_with('-'))
                .map(|next| next.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_the_model_names_of_the_agents() {
        let longest = "a".repeat(MAX_MODEL_LENGTH);
        for name in [
            "opus",
            "sonnet[1m]",
            "claude-opus-4-1",
            "openrouter/qwen-3",
            "gpt-5:high",
            "gemini-2.5-pro",
            "model@2025",
            longest.as_str(),
        ] {
            assert!(is_model_name(name), "{name}");
        }
    }

    #[test]
    fn refuses_names_that_are_not_one_safe_word() {
        let too_long = "a".repeat(MAX_MODEL_LENGTH + 1);
        for name in [
            "",
            "-p",
            "--model",
            "opus; rm",
            "a b",
            "$(x)",
            "`x`",
            "a|b",
            "a&b",
            "\"opus\"",
            "'opus'",
            too_long.as_str(),
        ] {
            assert!(!is_model_name(name), "{name}");
        }
    }

    #[test]
    fn reads_the_model_from_the_words_of_a_command_line() {
        assert_eq!(model_of(Some("--model sonnet")).as_deref(), Some("sonnet"));
        assert_eq!(model_of(Some("--model=opus")).as_deref(), Some("opus"));
        assert_eq!(model_of(Some("-m gpt-5")).as_deref(), Some("gpt-5"));
        assert_eq!(
            model_of(Some("--verbose  --model gpt-5-codex --yolo")).as_deref(),
            Some("gpt-5-codex")
        );
    }

    #[test]
    fn finds_no_model_when_the_args_name_none() {
        for args in [
            None,
            Some(""),
            Some("--verbose"),
            Some("--verbose --model"),
            Some("--model --verbose"),
            Some("--model="),
        ] {
            assert_eq!(model_of(args), None, "{args:?}");
        }
    }
}
