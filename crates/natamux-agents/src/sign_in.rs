//! What each agent prints when it waits for the user to sign in. Without a login an agent shows
//! its sign-in screen and waits, and no hook runs, so only its output tells.

/// Words of each agent's screen when it is not logged in, matched without regard to case. Each
/// was seen in the real CLI, not logged in, on 2026-11-09; `docs/agent-adapters.md` names the
/// versions. Check them again when a CLI changes.
pub const SIGN_IN_LINES: [(&str, &[&str]); 5] = [
    // The first start shows the theme picker, then the login; a later one answers the prompt.
    (
        "claude-code",
        &["Choose the text style", "Please run /login"],
    ),
    ("codex", &["Sign in with ChatGPT"]),
    ("gemini", &["How would you like to authenticate"]),
    ("copilot", &["Please use /login to sign in"]),
    // Its interactive start shows a blank screen; only `-p` says so, and ends.
    ("cursor", &["Authentication required"]),
];

/// The first line of the text that holds a sign-in line of the agent kind, with its runs of
/// spaces made one. None for an agent with no known sign-in screen, such as a custom agent on
/// "other".
pub fn sign_in_line(kind: &str, text: &str) -> Option<String> {
    let (_, words) = SIGN_IN_LINES.iter().find(|(name, _)| *name == kind)?;
    text.split(['\r', '\n'])
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .find(|line| {
            let lower = line.to_lowercase();
            words
                .iter()
                .any(|word| lower.contains(&word.to_lowercase()))
        })
}

#[cfg(test)]
#[path = "sign_in_tests.rs"]
mod tests;
