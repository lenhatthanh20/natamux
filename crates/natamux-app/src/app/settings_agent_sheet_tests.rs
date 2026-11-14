use super::*;

fn agent(id: &str, base: &str, command: &str) -> CustomAgent {
    CustomAgent {
        id: id.to_owned(),
        base: base.to_owned(),
        command: command.to_owned(),
    }
}

#[test]
fn the_base_select_lists_the_built_in_agents_then_other_cli() {
    let options = base_options();
    assert_eq!(options.len(), AGENT_CHOICES.len() + 1);
    assert_eq!(options[0].1, "Claude Code");
    assert_eq!(options.last().map(|(id, _)| id.as_str()), Some(OTHER_BASE));
    assert_eq!(base_label("other"), "Other CLI");
    assert_eq!(base_label("codex"), "Codex");
}

#[test]
fn shows_the_default_program_of_each_built_in_agent() {
    let programs: Vec<_> = AGENT_CHOICES
        .iter()
        .map(|choice| default_program(choice.id))
        .collect();
    assert_eq!(
        programs,
        ["claude", "codex", "gemini", "copilot", "cursor-agent"]
    );
}

#[test]
fn another_cli_gets_a_cyan_terminal_tile_and_a_built_in_base_a_blue_bot() {
    assert_eq!(agent_tile(OTHER_BASE), ("terminal", TileColor::Cyan));
    assert_eq!(agent_tile("claude-code"), ("bot", TileColor::Blue));
    assert_eq!(agent_tile("cursor"), ("bot", TileColor::Blue));
}

#[test]
fn the_footnote_says_what_natamux_does_with_the_base() {
    assert_eq!(
        base_footnote(OTHER_BASE),
        "No hooks: Natamux sees the bell, natamux ask and natamux done."
    );
    assert_eq!(
        base_footnote("codex"),
        "Natamux starts it like Codex, with its hooks."
    );
}

#[test]
fn a_new_agent_is_titled_add_agent_and_an_edited_one_by_its_id() {
    assert_eq!(sheet_title(false, ""), "Add agent");
    assert_eq!(sheet_title(true, "claude-qwen"), "claude-qwen");
}

#[test]
fn a_right_agent_has_no_errors() {
    let errors = sheet_errors(&agent("claude-qwen", "claude-code", "cq"), &[]);
    assert!(errors.is_empty());
}

#[test]
fn an_empty_or_badly_written_id_gets_the_daemon_s_message() {
    for id in ["", "Claude Qwen", "qwen-", "a_b"] {
        let errors = sheet_errors(&agent(id, "codex", "cq"), &[]);
        assert_eq!(errors.id.as_deref(), Some(ID_FORMAT), "{id}");
        assert_eq!(errors.command, None);
    }
    let long = "a".repeat(33);
    let errors = sheet_errors(&agent(&long, "codex", "cq"), &[]);
    assert_eq!(errors.id.as_deref(), Some(ID_LONG));
}

#[test]
fn an_id_of_a_built_in_agent_or_another_row_is_taken() {
    let others = [agent("qwen", "codex", "q")];
    for id in ["claude-code", "other", "qwen"] {
        let errors = sheet_errors(&agent(id, "codex", "cq"), &others);
        assert_eq!(
            errors.id,
            Some(format!("The agent id \"{id}\" is taken")),
            "{id}"
        );
    }
}

#[test]
fn an_empty_or_too_long_command_is_refused() {
    let errors = sheet_errors(&agent("qwen", "other", ""), &[]);
    assert_eq!(errors.command.as_deref(), Some(NO_COMMAND));
    assert_eq!(errors.id, None);
    let long = "a".repeat(501);
    let errors = sheet_errors(&agent("qwen", "other", &long), &[]);
    assert_eq!(errors.command.as_deref(), Some(COMMAND_LONG));
}
