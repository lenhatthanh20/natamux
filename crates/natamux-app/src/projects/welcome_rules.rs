//! Words and small rules of the welcome screen, kept out of the view so tests can reach them.

use natamux_proto::projects::repo::Repo;

/// One row of step 2. `agent` is an `AGENT_CHOICES` id.
pub struct SignIn {
    // Only the test that ties the rows to AGENT_CHOICES reads it.
    #[cfg_attr(not(test), allow(dead_code))]
    pub agent: &'static str,
    pub label: &'static str,
    pub command: &'static str,
    pub note: Option<&'static str>,
}

pub const SIGN_INS: [SignIn; 5] = [
    SignIn {
        agent: "claude-code",
        label: "Claude Code",
        command: "claude",
        note: Some("then /login"),
    },
    SignIn {
        agent: "codex",
        label: "Codex",
        command: "codex login",
        note: None,
    },
    SignIn {
        agent: "gemini",
        label: "Gemini CLI",
        command: "gemini",
        note: Some("then pick a sign-in"),
    },
    SignIn {
        agent: "copilot",
        label: "GitHub Copilot",
        command: "copilot",
        note: Some("then /login"),
    },
    SignIn {
        agent: "cursor",
        label: "Cursor",
        command: "cursor-agent login",
        note: None,
    },
];

pub const NO_PROJECT_TIP: &str = "Add a project first";

/// Step 1 body once a project exists; `None` with no project.
pub fn project_line(repos: &[Repo]) -> Option<String> {
    let first = repos.first()?;
    Some(match repos.len() - 1 {
        0 => format!("{} is ready.", first.name),
        more => format!("{} and {more} more are ready.", first.name),
    })
}

/// The project "Open workflow settings" opens.
pub fn settings_target(repos: &[Repo]) -> Option<&str> {
    repos.first().map(|repo| repo.id.as_str())
}

pub fn added_text(name: &str) -> String {
    format!("Added {name}.")
}

pub fn copied_text(label: &str) -> String {
    format!("Copied {label} command.")
}

#[cfg(test)]
#[path = "welcome_rules_tests.rs"]
mod tests;
