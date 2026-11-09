use super::*;

// The lines as each CLI printed them, not logged in, with colors and cursor moves taken out.
#[test]
fn finds_the_sign_in_line_of_each_built_in_agent() {
    for (kind, line) in [
        (
            "claude-code",
            "Choose the text style that looks best with your terminal",
        ),
        ("claude-code", "⎿  Not logged in · Please run /login"),
        ("codex", "> 1. Sign in with ChatGPT"),
        (
            "gemini",
            "│   How would you like to authenticate for this project? │",
        ),
        ("copilot", "Please use /login to sign in to use Copilot"),
        (
            "cursor",
            "Error: Authentication required. Please run 'agent login' first, or set \
             CURSOR_API_KEY environment variable.",
        ),
    ] {
        let text = format!("starting\r\n  {line}  \r\nmore");
        let one_spaced = line.split_whitespace().collect::<Vec<_>>().join(" ");
        assert_eq!(sign_in_line(kind, &text), Some(one_spaced), "{kind}");
    }
}

#[test]
fn makes_runs_of_spaces_one_and_matches_without_regard_to_case() {
    assert_eq!(
        sign_in_line("claude-code", " Choose  the   text style").as_deref(),
        Some("Choose the text style")
    );
    assert_eq!(
        sign_in_line("codex", "SIGN IN WITH CHATGPT").as_deref(),
        Some("SIGN IN WITH CHATGPT")
    );
}

#[test]
fn every_launchable_agent_has_sign_in_lines() {
    for agent in crate::launch::LAUNCHABLE_AGENTS {
        assert!(
            SIGN_IN_LINES.iter().any(|(kind, _)| *kind == agent),
            "{agent}"
        );
    }
}

#[test]
fn finds_nothing_for_another_agent_or_other_text() {
    assert_eq!(sign_in_line("claude-code", "Sign in with ChatGPT"), None);
    assert_eq!(sign_in_line("my-agent", "Please run /login"), None);
    assert_eq!(sign_in_line("claude-code", "Welcome back!\r\n> "), None);
}
