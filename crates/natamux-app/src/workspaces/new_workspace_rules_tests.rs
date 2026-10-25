//! Tests of `new_workspace_rules.rs`, apart to keep that file small.

use super::*;

fn source(kind: &str, name: Option<&str>) -> SourceChoice {
    SourceChoice::Project(ContextSource {
        kind: kind.into(),
        name: name.map(str::to_string),
        skill: None,
        agent: None,
        prompt: None,
    })
}

fn form(title: &str) -> StartForm {
    StartForm {
        title: title.into(),
        repo: "default".into(),
        agent: "claude-code".into(),
        ..StartForm::default()
    }
}

fn file(id: u64, name: &str, size: u64) -> PickedFile {
    PickedFile {
        id,
        name: name.into(),
        size,
        path: PathBuf::from(name),
    }
}

#[test]
fn says_what_the_agent_reads_for_each_source() {
    let hints = [
        (
            source("github-issue", None),
            "The agent reads the issue (its title, text, labels and comments) and the files as the task's context.",
        ),
        (
            source("skill", Some("Ticket")),
            "The agent runs the Ticket skill on the reference, and reads what it finds and the files as the task's context.",
        ),
        (
            source("mcp", Some("Tracker")),
            "The agent asks the Tracker MCP server about the reference, and reads what it finds and the files as the task's context.",
        ),
        (
            manual(),
            "The agent reads the title, the notes and the files as the task's context.",
        ),
    ];
    for (choice, hint) in hints {
        assert_eq!(choice.context_hint(""), hint);
    }
}

#[test]
fn says_under_the_reference_what_to_enter_for_each_source() {
    assert_eq!(
        source("github-issue", None).ref_hint(""),
        "The issue number, such as #42, or its address. Natamux reads the issue from GitHub."
    );
    assert_eq!(
        source("skill", Some("Ticket")).ref_hint(""),
        "The key or address of the ticket. The Ticket skill reads it."
    );
    assert_eq!(
        source("mcp", Some("Tracker")).ref_hint(""),
        "The key or address of the ticket. The agent reads it through the Tracker MCP server."
    );
    assert_eq!(
        source("github-issue", None).ref_placeholder(),
        "#42 or the issue address"
    );
}

#[test]
fn says_the_picked_skill_reads_the_reference_before_a_skill_is_picked() {
    let skill = SourceChoice::AgentSkill;
    assert_eq!(
        skill.ref_hint(""),
        "The key or address of the record. The picked skill reads it."
    );
    assert_eq!(
        skill.ref_hint("jira-ticket"),
        "The key or address of the record. The jira-ticket skill reads it."
    );
}

#[test]
fn keeps_a_skill_from_the_settings_apart_from_an_agent_skill() {
    let ticket = source("skill", Some("Ticket"));
    let choices = source_choices(Some(&[]), true);
    assert_eq!(choices, vec![SourceChoice::AgentSkill]);
    assert_ne!(ticket.key(), SourceChoice::AgentSkill.key());
    assert_eq!(ticket.label(), "Ticket (skill)");
    assert_eq!(SourceChoice::AgentSkill.label(), "An agent skill");
    assert_eq!(
        ticket.context("ABC-1", ""),
        TaskContext {
            source: "skill".into(),
            name: Some("Ticket".into()),
            skill: None,
            reference: "ABC-1".into(),
        }
    );
}

#[test]
fn offers_it_next_to_my_notes_with_no_settings() {
    assert_eq!(
        source_choices(None, true),
        vec![manual(), SourceChoice::AgentSkill]
    );
    assert_eq!(source_choices(None, false), vec![manual()]);
}

#[test]
fn goes_back_to_my_notes_in_a_project_that_does_not_offer_the_picked_source() {
    let choices = source_choices(None, false);
    assert_eq!(pick_source(&choices, "github-issue"), manual());
    let more = vec![manual(), source("skill", Some("Linear"))];
    assert_eq!(pick_source(&more, "skill:Linear"), more[1]);
}

#[test]
fn picks_the_project_it_was_opened_for() {
    let ids = vec!["natamux".to_string(), "web".to_string()];
    assert_eq!(initial_repo(Some("web"), &ids), "web");
    assert_eq!(initial_repo(None, &ids), "natamux");
    assert_eq!(initial_repo(None, &[]), "default");
}

#[test]
fn starts_in_the_first_project_when_the_one_it_was_opened_for_is_gone() {
    let ids = vec!["natamux".to_string()];
    assert_eq!(shown_repo("gone", &ids), "natamux");
    assert_eq!(shown_repo("gone", &[]), "gone");
    assert_eq!(shown_repo("natamux", &ids), "natamux");
}

#[test]
fn sends_the_title_the_project_and_the_default_agent_and_nothing_else() {
    let input = new_workspace_input(&form(" Fix the login "), &manual(), Vec::new());
    assert_eq!(
        serde_json::to_value(&input).unwrap(),
        serde_json::json!({ "title": "Fix the login", "repo": "default", "agent": "claude-code" })
    );
}

#[test]
fn sends_no_notes_when_none_are_typed() {
    let typed = StartForm {
        notes: "  \n   \n ".into(),
        ..form("Fix the login")
    };
    assert_eq!(
        new_workspace_input(&typed, &manual(), Vec::new()).prompt,
        None
    );
}

#[test]
fn keeps_the_line_breaks_inside_the_notes() {
    let typed = StartForm {
        notes: "Keep the old URL.\n\nAsk first.\n".into(),
        ..form("Fix the login")
    };
    assert_eq!(
        new_workspace_input(&typed, &manual(), Vec::new())
            .prompt
            .as_deref(),
        Some("Keep the old URL.\n\nAsk first.")
    );
}

#[test]
fn hides_the_notes_when_the_context_comes_from_an_issue_and_does_not_send_them() {
    let typed = StartForm {
        notes: "Keep the old URL".into(),
        reference: "#4".into(),
        ..form("Fix the login")
    };
    let input = new_workspace_input(&typed, &source("github-issue", None), Vec::new());
    assert_eq!(input.prompt, None);
    assert_eq!(
        serde_json::to_value(input.context).unwrap(),
        serde_json::json!({ "source": "github-issue", "ref": "#4" })
    );
}

#[test]
fn sends_the_picked_skill_and_the_reference_and_no_notes() {
    let typed = StartForm {
        notes: "Keep the old URL".into(),
        reference: " ABC-1 ".into(),
        skill: "jira-ticket".into(),
        ..form("Fix the login")
    };
    let input = new_workspace_input(&typed, &SourceChoice::AgentSkill, Vec::new());
    assert_eq!(input.prompt, None);
    assert_eq!(
        serde_json::to_value(input.context).unwrap(),
        serde_json::json!({ "source": "skill", "skill": "jira-ticket", "ref": "ABC-1" })
    );
}

#[test]
fn asks_for_a_skill_a_reference_and_an_agent_before_it_starts() {
    let skill = SourceChoice::AgentSkill;
    let mut typed = form("Fix the login");
    assert_eq!(blocked_reason(&typed, &skill), Some("Pick a skill"));
    typed.skill = "jira-ticket".into();
    assert_eq!(
        blocked_reason(&typed, &skill),
        Some("Enter the reference of the context")
    );
    typed.reference = "ABC-1".into();
    typed.agent = String::new();
    assert_eq!(
        blocked_reason(&typed, &skill),
        Some("Pick an agent to run the skill")
    );
    typed.agent = "codex".into();
    assert_eq!(blocked_reason(&typed, &skill), None);
}

#[test]
fn asks_for_a_title_first() {
    assert_eq!(
        blocked_reason(&form("  "), &manual()),
        Some("Enter a title")
    );
}

#[test]
fn starts_from_the_default_branch_and_sends_no_base_while_it_is_kept() {
    let list = BranchList {
        branches: vec!["dev".into(), "main".into()],
        current: Some("dev".into()),
        default_branch: Some("main".into()),
    };
    assert_eq!(start_branch(Some(&list)).as_deref(), Some("main"));
    assert_eq!(branch_text(Some(&list), "", None), "main (default)");
    assert_eq!(base_after_pick("main", Some("main")), "");
    assert_eq!(branch_text(None, "", None), "The default branch");
}

#[test]
fn sends_the_picked_branch_as_the_base_even_the_current_one() {
    let list = BranchList {
        branches: vec!["dev".into(), "main".into()],
        current: Some("dev".into()),
        default_branch: Some("main".into()),
    };
    let base = base_after_pick("dev", start_branch(Some(&list)).as_deref());
    assert_eq!(base, "dev");
    assert_eq!(branch_text(Some(&list), &base, None), "dev (current)");
    let typed = StartForm {
        base,
        ..form("Fix")
    };
    let input = new_workspace_input(&typed, &manual(), Vec::new());
    assert_eq!(input.base.as_deref(), Some("dev"));
}

#[test]
fn shows_starting_the_task_while_it_waits_and_start_task_after_an_error() {
    assert_eq!(start_label(true), "Starting the task");
    assert_eq!(start_label(false), "Start task");
}

#[test]
fn refuses_a_file_over_20_mb_with_a_reason_and_keeps_the_others() {
    let (kept, refused) = fit_attachments(
        &[],
        vec![file(1, "big.mov", 20_000_001), file(2, "notes.md", 5)],
    );
    assert_eq!(kept, vec![file(2, "notes.md", 5)]);
    assert_eq!(
        refused,
        vec!["big.mov is larger than 20 MB, the most for one file".to_string()]
    );
}

#[test]
fn refuses_the_files_after_the_20th() {
    let current: Vec<PickedFile> = (0..19).map(|i| file(i, &format!("f{i}"), 1)).collect();
    let (kept, refused) =
        fit_attachments(&current, vec![file(19, "a.txt", 1), file(20, "b.txt", 1)]);
    assert_eq!(kept.len(), 1);
    assert_eq!(
        refused,
        vec!["b.txt is not attached: a task takes at most 20 files".to_string()]
    );
}

#[test]
fn sends_the_picked_files_as_base64_by_name() {
    assert_eq!(base64_encode(b"hello"), "aGVsbG8=");
    assert_eq!(base64_encode(&[0, 255]), "AP8=");
    assert_eq!(base64_encode(b"abc"), "YWJj");
    assert_eq!(base64_encode(b""), "");
    let dir = std::env::temp_dir().join(format!("nw-rules-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("notes.md");
    std::fs::write(&path, "hello").unwrap();
    let picked = PickedFile {
        path,
        ..file(1, "notes.md", 5)
    };
    assert_eq!(
        read_uploads(&[picked]).unwrap(),
        vec![AttachmentUpload {
            name: "notes.md".into(),
            data: "aGVsbG8=".into(),
        }]
    );
    let missing = file(2, "gone.md", 1);
    assert_eq!(
        read_uploads(&[missing]).unwrap_err(),
        "Could not read gone.md"
    );
}

#[test]
fn formats_sizes_as_core_does() {
    assert_eq!(format_bytes(5), "5 B");
    assert_eq!(format_bytes(1500), "1.5 KB");
    assert_eq!(format_bytes(2_500_000), "2.5 MB");
}
