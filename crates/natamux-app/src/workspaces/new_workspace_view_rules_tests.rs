//! Tests of `new_workspace_view_rules.rs`.

use natamux_proto::stages::catalog::default_pipeline;
use natamux_proto::stages::schema::{Gate, Stage, StageGroup, StageKind};

use super::*;
use crate::workspaces::new_workspace_rules::manual;

fn form(title: &str) -> StartForm {
    StartForm {
        title: title.into(),
        repo: "storefront".into(),
        agent: "claude-code".into(),
        ..StartForm::default()
    }
}

fn list(default_branch: Option<&str>) -> BranchList {
    BranchList {
        branches: vec!["dev".into(), "main".into()],
        current: Some("dev".into()),
        default_branch: default_branch.map(str::to_string),
    }
}

#[test]
fn names_the_stages_of_the_pipeline_in_order_with_groups_flattened_and_off_stages_left_out() {
    assert_eq!(
        stage_names(&default_pipeline()),
        ["Context", "Spec", "Implement", "Review", "Release"]
    );
    let mut off = Stage::new("testing", StageKind::Agent, Gate::Auto);
    off.enabled = Some(false);
    let mut named = Stage::new("build", StageKind::Agent, Gate::Auto);
    named.name = Some("Build it".into());
    let steps = vec![
        Step::Group(StageGroup {
            id: "both".into(),
            name: None,
            parallel: vec![
                named,
                Stage::new("write-notes", StageKind::Agent, Gate::Auto),
            ],
        }),
        Step::Stage(off),
    ];
    assert_eq!(stage_names(&steps), ["Build it", "Write notes"]);
}

#[test]
fn the_footer_note_gives_the_first_blocked_reason() {
    assert_eq!(
        footer_note(&form(" "), &manual(), "main", 5),
        FooterNote::Blocked("Enter a title")
    );
    let mut single = form("");
    single.single_agent = true;
    single.agent.clear();
    assert_eq!(
        footer_note(&single, &manual(), "main", 5),
        FooterNote::Blocked(CHOOSE_AGENT)
    );
}

#[test]
fn the_footer_note_says_what_start_task_makes_when_nothing_blocks() {
    let filled = form("ABC-123 Fix the login redirect");
    assert_eq!(
        footer_note(&filled, &manual(), "main", 5),
        FooterNote::Plan("New worktree from main · Claude Code · 5 stages".into())
    );
    let no_worktree = StartForm {
        no_worktree: true,
        ..filled.clone()
    };
    assert_eq!(
        footer_note(&no_worktree, &manual(), "main", 1),
        FooterNote::Plan("Task only, no workspace yet · Claude Code · 1 stage".into())
    );
    let plain = StartForm {
        agent: String::new(),
        ..filled.clone()
    };
    assert_eq!(
        footer_note(&plain, &manual(), "release/1.4", 0),
        FooterNote::Plan("New worktree from release/1.4 · plain terminal · full flow".into())
    );
    let single = StartForm {
        single_agent: true,
        ..filled
    };
    assert_eq!(
        footer_note(&single, &manual(), "main", 5),
        FooterNote::Plan("New worktree from main · Claude Code · one agent".into())
    );
}

#[test]
fn the_branch_help_says_where_its_name_comes_from() {
    assert_eq!(branch_help(true, true), "Your own name");
    assert_eq!(branch_help(false, true), "Follows the title");
    assert_eq!(branch_help(false, false), "Natamux names it");
}

#[test]
fn the_worktree_switch_says_what_on_and_off_make() {
    assert_eq!(
        worktree_help(true),
        "A git worktree on a new branch, apart from your checkout."
    );
    assert_eq!(
        worktree_help(false),
        "Task only. Make the workspace later from the task."
    );
    assert_eq!(worktree_tip(true), "On: a git worktree on a new branch");
    assert_eq!(worktree_tip(false), "Off: the task only, no workspace yet");
}

#[test]
fn the_flow_tooltip_says_what_one_agent_skips() {
    assert!(FLOW_TIP.contains("One agent does the whole task in Terminal 1"));
    assert!(FLOW_TIP.contains("the stages do not run"));
}

#[test]
fn only_the_agent_reasons_show_as_a_red_line_under_their_row() {
    assert_eq!(inline_error(Some(CHOOSE_AGENT)), Some(ErrorRow::Flow));
    assert_eq!(inline_error(Some(PICK_AGENT)), Some(ErrorRow::Skill));
    assert_eq!(inline_error(Some("Enter a title")), None);
    assert_eq!(inline_error(Some("Pick a skill")), None);
    assert_eq!(inline_error(None), None);
}

#[test]
fn the_blocked_reasons_match_the_ones_of_the_rules() {
    let mut single = form("Fix");
    single.single_agent = true;
    single.agent.clear();
    assert_eq!(blocked_reason(&single, &manual()), Some(CHOOSE_AGENT));
    let skill = StartForm {
        skill: "jira".into(),
        reference: "ABC-1".into(),
        agent: String::new(),
        ..form("Fix")
    };
    assert_eq!(
        blocked_reason(&skill, &SourceChoice::AgentSkill),
        Some(PICK_AGENT)
    );
}

#[test]
fn the_branch_from_select_shows_the_base_and_marks_the_default_one() {
    let with_default = list(Some("main"));
    assert_eq!(base_name(Some(&with_default), ""), "main");
    assert!(shows_default(Some(&with_default), ""));
    assert_eq!(base_name(Some(&with_default), "release/1.4"), "release/1.4");
    assert!(!shows_default(Some(&with_default), "release/1.4"));
    // No default branch: the checkout's branch, which is not the default.
    assert_eq!(base_name(Some(&list(None)), ""), "dev");
    assert!(!shows_default(Some(&list(None)), ""));
    // While the branches load.
    assert_eq!(base_name(None, ""), "The default branch");
    assert!(!shows_default(None, ""));
}

#[test]
fn the_start_tooltip_says_why_it_is_off_or_what_it_does() {
    assert_eq!(
        start_tip(Some("Enter a title"), false, false),
        "Enter a title"
    );
    assert_eq!(start_tip(None, true, false), "Starting the task");
    assert_eq!(
        start_tip(None, false, false),
        "A new task with its own workspace: a git worktree on a new branch."
    );
    assert_eq!(
        start_tip(None, false, true),
        "A new task with no workspace yet. Make it later from the task."
    );
}

#[test]
fn the_skill_menu_names_the_agent_that_runs_the_skill() {
    assert_eq!(skill_menu_header("codex"), "Codex · this project");
    assert_eq!(skill_menu_header(""), "Claude Code · this project");
}

#[test]
fn each_context_source_has_its_icon() {
    let source = |kind: &str| {
        SourceChoice::Project(natamux_proto::projects::repo::ContextSource {
            kind: kind.into(),
            name: Some("x".into()),
            skill: None,
            agent: None,
            prompt: None,
        })
    };
    assert_eq!(source_icon(&manual()), "pencil");
    assert_eq!(source_icon(&source("github-issue")), "circle-dot");
    assert_eq!(source_icon(&source("skill")), "sparkles");
    assert_eq!(source_icon(&source("mcp")), "plug");
    assert_eq!(source_icon(&SourceChoice::AgentSkill), "sparkles");
}
