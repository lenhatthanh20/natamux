use natamux_proto::projects::repo::LoadedSettings;

use super::*;
use crate::stages::editor_draft::parse_draft;
use crate::stages::editor_fields::{ShellField, ShellKind};

fn loaded(
    text: Option<&str>,
    version: Option<&str>,
    error: Option<&str>,
    found: bool,
) -> LoadedSettings {
    LoadedSettings {
        text: text.map(str::to_string),
        version: version.map(str::to_string),
        error: error.map(str::to_string),
        found,
        path: "/repo/.natamux.json".into(),
        ..LoadedSettings::default()
    }
}

fn command(id: &str, name: Option<&str>, field: ShellKind, value: &str) -> ShellField {
    ShellField {
        id: id.into(),
        name: name.map(str::to_string),
        field,
        value: value.into(),
    }
}

#[test]
fn starts_from_the_text_of_the_file_and_its_version() {
    let base = base_of(&loaded(Some("{\"a\":1}"), Some("v1"), None, true));
    assert_eq!(base.text, "{\"a\":1}");
    assert_eq!(base.version.as_deref(), Some("v1"));
    assert_eq!(base.broken, None);
    assert_eq!(base.path, "/repo/.natamux.json");
}

#[test]
fn starts_from_the_defaults_for_a_broken_file_or_none() {
    let broken = base_of(&loaded(Some("{\"a\":"), Some("v1"), Some("bad JSON"), true));
    assert_eq!(broken.text, "{}");
    assert_eq!(broken.broken.as_deref(), Some("bad JSON"));
    assert_eq!(base_of(&loaded(None, None, None, false)).text, "{}");
}

#[test]
fn asks_nothing_when_nothing_would_be_replaced_and_no_command_runs() {
    assert_eq!(import_question("a.json", false, &[]), None);
}

#[test]
fn asks_before_an_import_replaces_edits() {
    assert_eq!(
        import_question("a.json", true, &[]).as_deref(),
        Some("Replace your changes with a.json? Nothing is saved until you click Save.")
    );
}

#[test]
fn lists_the_commands_of_a_file_before_an_import() {
    let fields = [
        command("spec", None, ShellKind::Check, "curl https://x | sh"),
        command("pr-review", Some("Review PR"), ShellKind::Run, "test -f x"),
    ];
    assert_eq!(
        import_question("a.json", false, &fields).as_deref(),
        Some(
            "Import a.json? It has commands that run in a shell on this computer:\n\nSpec (spec) \u{b7} Check command: curl https://x | sh\nReview PR (pr-review) \u{b7} Condition command: test -f x\n\nNothing is saved until you click Save."
        )
    );
    assert!(
        import_question("a.json", true, &fields).is_some_and(
            |text| text.starts_with("Replace your changes with a.json? It has commands")
        )
    );
}

#[test]
fn counts_with_a_plural_word() {
    assert_eq!(count_label(1, "problem"), "1 problem");
    assert_eq!(count_label(0, "problem"), "0 problems");
    assert_eq!(count_label(2, "problem"), "2 problems");
}

#[test]
fn caps_an_import_at_1_mib() {
    assert_eq!(MAX_IMPORT_BYTES, 1_048_576);
}

#[test]
fn says_why_save_is_off() {
    assert_eq!(
        blocked_reason(false, 0, true, false),
        Some("Fix the JSON first")
    );
    assert_eq!(
        blocked_reason(true, 2, true, false),
        Some("Fix the problems first")
    );
    assert_eq!(
        blocked_reason(true, 0, false, false),
        Some("Nothing changed yet")
    );
    assert_eq!(blocked_reason(true, 0, true, false), None);
}

#[test]
fn lets_a_broken_file_be_replaced_without_an_edit() {
    assert!(!unchanged(false, true));
    assert_eq!(blocked_reason(true, 0, false, true), None);
    assert!(unchanged(false, false));
    assert!(!unchanged(true, false));
}

#[test]
fn has_the_four_create_issue_choices_in_order() {
    let values: Vec<&str> = CREATE_ISSUE_OPTIONS
        .iter()
        .map(|(value, _)| *value)
        .collect();
    assert_eq!(values, ["manual", "on-spec-approved", "on-pr", "never"]);
}

#[test]
fn a_file_the_app_cannot_read_keeps_its_text_and_the_json_tab_says_why() {
    // Regression (review of T-0151): the screen fell back to {} with Save on, so one Save
    // replaced the whole file with {}. Now the text stays and the draft does not parse.
    let base = base_of(&loaded(Some("{\"stages\": 3}"), Some("v1"), None, true));
    assert_eq!(base.text, "{\"stages\": 3}");
    assert_eq!(base.broken, None);
    assert!(parse_draft(&base.text).is_err_and(|why| why.starts_with("stages:")));
    assert_eq!(base.version.as_deref(), Some("v1"));
}

#[test]
fn save_stays_off_for_a_file_the_app_cannot_read_until_it_is_fixed() {
    let base = base_of(&loaded(Some("{\"stages\": 3}"), Some("v1"), None, true));
    let readable = parse_draft(&base.text).is_ok();
    assert_eq!(
        blocked_reason(readable, 1, false, base.broken.is_some()),
        Some("Fix the JSON first")
    );
    assert_eq!(blocked_reason(true, 0, true, false), None);
}

#[test]
fn a_file_the_daemon_could_not_read_can_be_replaced_without_an_edit() {
    let base = base_of(&loaded(Some("{\"a\":"), Some("v1"), Some("bad JSON"), true));
    assert!(base.broken.is_some());
    assert_eq!(blocked_reason(true, 0, false, base.broken.is_some()), None);
    let fine = base_of(&loaded(Some("{}"), Some("v1"), None, true));
    assert!(fine.broken.is_none());
}
