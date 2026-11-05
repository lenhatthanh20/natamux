use super::*;

fn agent(id: &str, base: &str, command: &str) -> CustomAgent {
    CustomAgent {
        id: id.to_owned(),
        base: base.to_owned(),
        command: command.to_owned(),
    }
}

#[test]
fn a_row_shows_its_id_or_new_agent() {
    assert_eq!(row_label(&agent("qwen", "other", "q")), "qwen");
    assert_eq!(row_label(&agent("", "other", "q")), "new-agent");
}

#[test]
fn a_row_s_help_is_the_base_and_the_command() {
    let row = agent("q", "claude-code", "cq");
    let (base, command) = row_help(&row);
    assert_eq!((base.as_str(), command), ("Claude Code", "cq"));
    let row = agent("q", "other", "");
    let (base, command) = row_help(&row);
    assert_eq!((base.as_str(), command), ("Other CLI", "no command"));
}

#[test]
fn the_bar_says_whether_the_list_is_saved() {
    assert_eq!(status_text(true), "Unsaved changes to custom agents");
    assert_eq!(status_text(false), "Custom agents saved on the daemon");
}

#[test]
fn save_is_on_only_for_a_changed_list_that_was_read_and_is_not_saving() {
    assert!(save_enabled(true, false, true));
    assert!(!save_enabled(false, false, true));
    assert!(!save_enabled(true, true, true));
    assert!(!save_enabled(true, false, false));
}
