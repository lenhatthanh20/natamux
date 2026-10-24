//! Ports `packages/daemon/src/stages/__tests__/stage-prompts.test.ts`.

use natamux_proto::stages::catalog::default_pipeline;
use natamux_proto::stages::prompt::default_prompt;
use natamux_proto::stages::schema::{Gate, StageKind};
use serde_json::{Value, json};

use super::*;

fn task_of(extra: Value) -> Task {
    let mut value = json!({"id": "T-1", "title": "Export tasks", "description": "", "createdAt": "2026-01-01T00:00:00.000Z", "updatedAt": "2026-01-01T00:00:00.000Z"});
    value
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    serde_json::from_value(value).unwrap()
}

fn task() -> Task {
    task_of(json!({}))
}

fn default_stage(id: &str) -> Stage {
    default_pipeline()
        .iter()
        .flat_map(|step| step.stages())
        .find(|stage| stage.id == id)
        .cloned()
        .unwrap_or_else(|| Stage::new(id, StageKind::Agent, Gate::Auto))
}

fn with_prompt(id: &str, prompt: &str) -> Stage {
    Stage {
        prompt: Some(prompt.into()),
        ..default_stage(id)
    }
}

#[test]
fn tells_the_implement_agent_not_to_commit_by_default() {
    let prompt = stage_prompt(&default_stage("implement"), &task(), false);
    assert!(prompt.contains("Do not commit"));
    assert!(!prompt.contains("Commit your work"));
    assert!(prompt.ends_with(DONE));
}

#[test]
fn tells_the_testing_agent_not_to_commit_by_default() {
    let prompt = stage_prompt(&default_stage("testing"), &task(), false);
    assert!(prompt.contains("Do not commit"));
    assert!(!prompt.contains("Commit your work"));
}

#[test]
fn asks_the_agent_to_commit_when_the_stage_needs_a_commit() {
    let stage = Stage {
        require_commit: Some(true),
        ..default_stage("implement")
    };
    let prompt = stage_prompt(&stage, &task(), false);
    assert!(prompt.contains("Commit your work when it is done."));
    assert!(!prompt.contains("Do not commit"));
}

#[test]
fn keeps_a_prompt_of_your_own_as_it_is() {
    let stage = with_prompt("implement", "Fix {title}. {commit}");
    assert_eq!(
        stage_prompt(&stage, &task(), false),
        format!("Fix Export tasks. {{commit}}\n{DONE}")
    );
}

#[test]
fn asks_for_a_workflow_once_just_before_the_done_line_when_the_stage_runs_one() {
    let plain = stage_prompt(&default_stage("implement"), &task(), false);
    let prompt = stage_prompt(&default_stage("implement"), &task(), true);
    assert!(WORKFLOW_LINE.contains("ultracode"));
    assert_eq!(prompt.split(WORKFLOW_LINE).count(), 2);
    assert!(prompt.ends_with(&format!("{WORKFLOW_LINE}\n{DONE}")));
    assert_eq!(prompt.replacen(&format!("{WORKFLOW_LINE}\n"), "", 1), plain);
}

#[test]
fn leaves_the_workflow_out_when_the_stage_does_not_run_one() {
    for id in ["spec", "plan", "implement"] {
        let prompt = stage_prompt(&default_stage(id), &task(), false);
        assert!(!prompt.contains("ultracode"));
    }
}

#[test]
fn asks_for_a_workflow_in_a_prompt_of_your_own_too() {
    let stage = with_prompt("implement", "Fix {title}.");
    assert_eq!(
        stage_prompt(&stage, &task(), true),
        format!("Fix Export tasks.\n{WORKFLOW_LINE}\n{DONE}")
    );
}

#[test]
fn adds_the_workflow_line_once_also_when_your_prompt_names_ultracode_itself() {
    let stage = with_prompt("implement", "ultracode: fix {title}.");
    let prompt = stage_prompt(&stage, &task(), true);
    assert_eq!(prompt.split(WORKFLOW_LINE).count(), 2);
    assert!(prompt.starts_with("ultracode: fix Export tasks.\n"));
}

#[test]
fn says_nothing_about_commits_in_the_spec_prompt() {
    let prompt = stage_prompt(&default_stage("spec"), &task(), false);
    assert!(prompt.contains("Do not change any code."));
    assert!(!prompt.to_lowercase().contains("commit"));
}

#[test]
fn takes_the_prompt_of_a_catalog_stage_that_is_not_in_the_default_pipeline() {
    let stage = Stage::new("security", StageKind::Agent, Gate::Manual);
    assert!(stage_prompt(&stage, &task(), false).contains(".natamux-task/security.md"));
}

#[test]
fn keeps_unknown_placeholders_long_text_and_other_languages_as_they_are() {
    let long = "x".repeat(12_000);
    let stage = Stage {
        prompt: Some(format!("Kiểm thử {{foo}} 🚀 {long}")),
        ..Stage::new("plan", StageKind::Agent, Gate::Manual)
    };
    assert_eq!(
        stage_prompt(&stage, &task(), false),
        format!("Kiểm thử {{foo}} 🚀 {long}\n{DONE}")
    );
}

#[test]
fn still_adds_the_done_line_to_a_prompt_that_asks_for_it_already() {
    let stage = Stage {
        prompt: Some("Plan it. Then run `natamux done`.".into()),
        ..Stage::new("plan", StageKind::Agent, Gate::Manual)
    };
    assert!(stage_prompt(&stage, &task(), false).ends_with(&format!("\n{DONE}")));
}

#[test]
fn gives_the_web_app_and_the_agent_the_same_default_prompt() {
    let plan = Stage {
        output: Some("plan.md".into()),
        ..Stage::new("plan", StageKind::Agent, Gate::Manual)
    };
    let expected = default_prompt(&plan)
        .replacen("{title}", "Export tasks", 1)
        .replace("{dir}", ".natamux-task");
    assert_eq!(
        stage_prompt(&plan, &task(), false),
        format!("{expected}\n{DONE}")
    );
}

#[test]
fn leaves_skill_placeholders_for_the_agent_to_fill() {
    let stage = Stage {
        prompt: Some("Use {skill:plan}".into()),
        ..Stage::new("plan", StageKind::Agent, Gate::Manual)
    };
    assert!(stage_prompt(&stage, &task(), false).contains("Use {skill:plan}"));
}

fn skill_source() -> ContextSource {
    ContextSource {
        kind: "skill".into(),
        name: Some("Ticket".into()),
        skill: Some("fetch-ticket".into()),
        agent: None,
        prompt: None,
    }
}

fn with_ref() -> Task {
    task_of(json!({"context": {"source": "skill", "ref": "ABC-1"}}))
}

fn first_line(text: String) -> String {
    text.split('\n').next().unwrap_or_default().to_string()
}

#[test]
fn calls_the_skill_with_the_reference() {
    let prompt = context_prompt(&skill_source(), &with_ref(), &[], None);
    assert!(prompt.starts_with("/fetch-ticket ABC-1\n"));
    let call = SkillResolution::Call("$fetch-ticket".into());
    let prompt = context_prompt(&skill_source(), &with_ref(), &[], Some(&call));
    assert_eq!(first_line(prompt), "$fetch-ticket ABC-1");
}

#[test]
fn gives_the_skill_file_to_an_agent_that_cannot_call_it_and_names_a_missing_skill() {
    let file = SkillResolution::File(".claude/skills/fetch-ticket/SKILL.md".into());
    let prompt = context_prompt(&skill_source(), &with_ref(), &[], Some(&file));
    assert_eq!(
        first_line(prompt),
        "Read .claude/skills/fetch-ticket/SKILL.md and follow it for ABC-1."
    );
    let prompt = context_prompt(
        &skill_source(),
        &with_ref(),
        &[],
        Some(&SkillResolution::Missing),
    );
    assert_eq!(first_line(prompt), "Use the fetch-ticket skill for ABC-1.");
}

#[test]
fn writes_the_title_and_the_notes_keeping_their_line_breaks() {
    let task = task_of(json!({
        "title": "Fix the login",
        "description": "  Keep the old URL.\nAsk before a redirect.  "
    }));
    assert_eq!(
        manual_context(&task),
        "# Fix the login\n\nKeep the old URL.\nAsk before a redirect.\n"
    );
}

#[test]
fn says_there_are_no_notes_when_the_notes_are_empty_or_only_spaces() {
    for description in ["", "  \n\t\n"] {
        let task = task_of(json!({"title": "Fix the login", "description": description}));
        assert_eq!(manual_context(&task), "# Fix the login\n\n(No notes.)\n");
    }
}

fn parent(description: &str) -> Task {
    task_of(json!({"id": "T-0001", "title": "Export", "description": description}))
}

#[test]
fn adds_the_parent_its_notes_and_its_stage_folder_after_the_context() {
    assert_eq!(
        with_parent_section(
            "# Part A\n\n(No notes.)\n",
            &parent(" Use CSV. "),
            "/w/.natamux-task"
        ),
        "# Part A\n\n(No notes.)\n\n## Parent task\n\nT-0001: Export\n\nUse CSV.\n\n\
         Stage files of the parent: /w/.natamux-task\n"
    );
}

#[test]
fn says_there_are_no_notes_and_keeps_a_blank_line_after_a_context_with_no_line_end() {
    assert_eq!(
        with_parent_section("# Part A", &parent(""), "/d"),
        "# Part A\n\n## Parent task\n\nT-0001: Export\n\n(No notes.)\n\nStage files of the parent: /d\n"
    );
}

// Cases of my own.

#[test]
fn lists_the_attachments_in_a_context_prompt() {
    let attachments = [Attachment {
        name: "a.png".into(),
        size: 1500,
        kind: None,
    }];
    let prompt = context_prompt(&skill_source(), &with_ref(), &attachments, None);
    assert!(
        prompt.contains(".natamux-task/attachments/a.png (1.5 KB)"),
        "{prompt}"
    );
}

#[test]
fn fills_words_by_hand() {
    assert_eq!(
        fill(
            "{a}{b} {a_1} {} {a b} {{a}} {a",
            &[("a", "1"), ("a_1", "2")]
        ),
        "1{b} 2 {} {a b} {1} {a"
    );
}
