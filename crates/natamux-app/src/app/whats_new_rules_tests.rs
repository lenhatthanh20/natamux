use super::*;

#[test]
fn shows_once_after_an_update_to_a_newer_version() {
    assert!(show_on_start(Some("1.0.19"), "1.0.20", false));
    assert!(show_on_start(Some("1.0.19"), "1.0.20", true));
    assert!(!show_on_start(Some("1.0.20"), "1.0.20", true));
    assert!(!show_on_start(Some("1.0.20"), "1.0.20", false));
}

#[test]
fn an_older_build_after_a_newer_one_shows_nothing() {
    assert!(!show_on_start(Some("1.0.21"), "1.0.20", true));
}

#[test]
fn without_a_saved_version_only_an_update_shows_the_notes() {
    // The first update from a version before this feature: the updater's folder is there.
    assert!(show_on_start(None, "1.0.20", true));
    // A new install.
    assert!(!show_on_start(None, "1.0.20", false));
}

#[test]
fn a_saved_version_that_does_not_parse_counts_as_older() {
    assert!(show_on_start(Some("not a version"), "1.0.20", false));
    assert!(show_on_start(Some(""), "1.0.20", false));
    // A build whose own version does not parse shows its notes once, not on every start.
    assert!(!show_on_start(Some("1.1.0-beta.1"), "1.1.0-beta.1", true));
}

#[test]
fn cuts_the_notes_at_the_download_heading() {
    let text =
        "Intro.\n\n## Desktop app\n\n- New.\n\n## Download\n\n| a | b |\n\n## Before you open it\n";
    assert_eq!(notes_body(text), "Intro.\n\n## Desktop app\n\n- New.");
}

#[test]
fn keeps_notes_without_a_download_heading_and_trims_the_end() {
    assert_eq!(notes_body("Intro.\n\n- One.\n\n\n"), "Intro.\n\n- One.");
}

#[test]
fn cuts_only_at_the_exact_heading() {
    let text = "## Downloads made faster\n\n- Yes.\n\n## Download\nrest";
    assert_eq!(notes_body(text), "## Downloads made faster\n\n- Yes.");
    assert_eq!(notes_body("## Download\r\nrest"), "");
}

#[test]
fn embeds_the_notes_of_this_version() {
    let body = notes_body(NOTES_FILE);
    assert!(!body.is_empty());
    assert!(!body.lines().any(|line| line.trim_end() == "## Download"));
    assert_eq!(VERSION, env!("CARGO_PKG_VERSION"));
}

#[test]
fn the_dialog_text_ends_with_the_changelog_link() {
    let text = dialog_text("Intro.\n\n## Download\nrest");
    assert!(text.starts_with("Intro.\n\n"));
    assert!(text.ends_with(&format!("[{CHANGELOG_URL}]({CHANGELOG_URL})")));
}
