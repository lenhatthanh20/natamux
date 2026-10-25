//! Import issues: what went wrong with GitHub and the button text (`features/projects/issues.ts`).

const NOT_INSTALLED: &str = "The GitHub CLI is not installed. Install `gh`, run `gh auth login` in a terminal, then try again.";
const NOT_SIGNED_IN: &str =
    "GitHub CLI is not signed in. Run `gh auth login` in a terminal, then try again.";
const OTHER: &str =
    "Natamux could not read the issues from GitHub. Check the connection and try again.";

/// What went wrong with GitHub, in a sentence that says how to fix it. The raw text stays
/// available behind "Show details".
pub fn issue_problem(message: &str) -> &'static str {
    let text = message.to_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|needle| text.contains(needle));
    if has(&["not installed", "not found", "enoent", "no such file"]) {
        NOT_INSTALLED
    } else if has(&[
        "auth",
        "log in",
        "login",
        "token",
        "credential",
        "403",
        "401",
    ]) {
        NOT_SIGNED_IN
    } else {
        OTHER
    }
}

pub fn import_label(count: usize) -> String {
    match count {
        0 => "Import".to_owned(),
        1 => "Import 1 issue".to_owned(),
        n => format!("Import {n} issues"),
    }
}

/// The toast after an import.
pub fn imported_text(count: usize) -> String {
    match count {
        1 => "Imported 1 issue.".to_owned(),
        n => format!("Imported {n} issues."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn says_how_to_sign_in_to_the_github_cli() {
        assert_eq!(
            issue_problem("gh: To get started with GitHub CLI, please run: gh auth login"),
            "GitHub CLI is not signed in. Run `gh auth login` in a terminal, then try again."
        );
    }

    #[test]
    fn says_when_the_cli_is_missing() {
        assert!(issue_problem("spawn gh ENOENT").contains("not installed"));
    }

    #[test]
    fn has_a_plain_sentence_for_anything_else() {
        assert!(issue_problem("socket hang up").contains("could not read the issues"));
    }

    #[test]
    fn counts_the_picked_issues() {
        assert_eq!(import_label(0), "Import");
        assert_eq!(import_label(1), "Import 1 issue");
        assert_eq!(import_label(3), "Import 3 issues");
    }

    #[test]
    fn says_how_many_issues_were_imported() {
        assert_eq!(imported_text(1), "Imported 1 issue.");
        assert_eq!(imported_text(4), "Imported 4 issues.");
    }
}
