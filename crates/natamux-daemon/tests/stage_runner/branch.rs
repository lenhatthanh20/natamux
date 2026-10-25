//! `describe('a branch')` of `stage-runner.test.ts`.

use std::sync::{Arc, Mutex};

use natamux_daemon::stages::act::StageActionInput;
use natamux_proto::tasks::task::{Task, TaskState};
use natamux_stages::actions::StageAction;
use natamux_stages::checks::Ran;
use natamux_stages::ci::CiResult;
use serde_json::{Value, json};

use crate::Fixture;

const NO_REVIEW: &str = "test -f .no-review";

fn plan() -> Value {
    json!({ "id": "plan", "kind": "agent", "gate": "manual" })
}

fn after_plan(small_enabled: bool, other: bool) -> Value {
    let mut small = json!([
        { "id": "implement", "kind": "agent", "gate": "auto" },
        { "id": "docs", "kind": "agent", "gate": "auto" },
    ]);
    if !small_enabled {
        for stage in small.as_array_mut().unwrap() {
            stage["enabled"] = json!(false);
        }
    }
    let mut paths = vec![
        json!({
            "id": "small",
            "when": { "type": "output", "file": "context.md", "contains": "[small]" },
            "stages": small,
        }),
        json!({
            "id": "pr-review",
            "when": { "type": "command", "run": NO_REVIEW },
            "not": true,
            "stages": [{ "id": "review", "kind": "review", "gate": "manual" }],
        }),
    ];
    if other {
        paths.push(json!({
            "id": "other",
            "stages": [{ "id": "notes", "kind": "agent", "gate": "auto" }],
        }));
    }
    json!({ "id": "after-plan", "paths": paths })
}

// Context, the steps given, the branch, then Release.
fn pipeline(f: &Fixture, before: Vec<Value>) {
    let mut stages = vec![
        json!({ "id": "context", "kind": "context", "gate": "manual", "output": "context.md" }),
    ];
    stages.extend(before);
    stages.push(after_plan(true, false));
    stages.push(json!({ "id": "release", "kind": "release", "gate": "manual" }));
    f.set_stages(Value::Array(stages));
}

// There is no .no-review file, so Review PR holds whenever Small change does not. The context has
// the title, so "[small]" in it picks Small change.
fn fixture() -> Fixture {
    let f = Fixture::new();
    pipeline(&f, vec![plan()]);
    f.set_run_command(|command, _| Ran {
        code: i32::from(command == NO_REVIEW),
        output: String::new(),
    });
    f
}

fn started_stages(f: &Fixture, id: &str) -> Vec<Value> {
    f.task_events(id)
        .into_iter()
        .filter(|event| event.kind == "stage.started")
        .map(|event| event.payload["stage"].clone())
        .collect()
}

fn at_plan_review(f: &Fixture, title: &str) -> Task {
    let task = f.created(json!({ "title": title, "agent": "claude-code" }));
    f.apply(
        &task.id,
        json!({ "source": "human", "type": "state.overridden", "payload": { "to": "review", "stage": "plan" } }),
    );
    f.with_workspace(&task.id)
}

fn act(f: &Fixture, id: &str, what: StageAction) -> Task {
    f.runner
        .run
        .act(id, what, &StageActionInput::default())
        .unwrap()
}

fn act_with(f: &Fixture, id: &str, what: StageAction, input: StageActionInput) -> Task {
    f.runner.run.act(id, what, &input).unwrap()
}

fn reopen_to(to: &str) -> StageActionInput {
    StageActionInput {
        to: Some(to.into()),
        ..Default::default()
    }
}

fn paths(task: &Task) -> Value {
    Value::Object(task.paths.clone())
}

fn payload(f: &Fixture, id: &str, kind: &str) -> Value {
    Value::Object(f.last_of(id, kind).unwrap().payload)
}

fn write_working(task: &Task, name: &str, text: &str) {
    let dir = std::path::Path::new(task.workspace.as_deref().unwrap()).join(".natamux-task");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(name), text).unwrap();
}

fn small() -> Value {
    json!({ "after-plan": "small" })
}

#[test]
fn takes_the_path_whose_condition_holds_when_the_stage_before_is_approved() {
    let f = fixture();
    let task = at_plan_review(&f, "Export [small]");

    let moved = act(&f, &task.id, StageAction::Approve);
    f.settle();

    assert_eq!(moved.stage, "implement");
    assert_eq!(paths(&moved), small());
    assert_eq!(
        payload(&f, &task.id, "stage.approved"),
        json!({ "stage": "plan", "path": "small", "because": "context.md contains \"[small]\"" })
    );
    assert!(started_stages(&f, &task.id).contains(&json!("implement")));
    assert!(!started_stages(&f, &task.id).contains(&json!("review")));
}

#[test]
fn takes_a_path_whose_command_fails_with_not_and_runs_the_command_in_the_worktree() {
    let f = fixture();
    let ran: Arc<Mutex<Vec<String>>> = Arc::default();
    let log = Arc::clone(&ran);
    f.set_run_command(move |command, cwd| {
        log.lock()
            .unwrap()
            .push(format!("{command} in {}", cwd.display()));
        Ran {
            code: 1,
            output: String::new(),
        }
    });
    let task = at_plan_review(&f, "Export");

    let moved = act(&f, &task.id, StageAction::Approve);
    f.settle();

    assert_eq!(
        (moved.stage.as_str(), moved.state),
        ("review", TaskState::Review)
    );
    assert_eq!(paths(&moved), json!({ "after-plan": "pr-review" }));
    let approved = payload(&f, &task.id, "stage.approved");
    assert_eq!(approved["path"], "pr-review");
    assert_eq!(approved["because"], "test -f .no-review did not exit 0");
    let workspace = task.workspace.clone().unwrap();
    assert_eq!(
        *ran.lock().unwrap(),
        vec![format!("{NO_REVIEW} in {workspace}")]
    );
    assert!(!started_stages(&f, &task.id).contains(&json!("implement")));
}

#[test]
fn goes_on_after_the_branch_when_no_condition_holds() {
    let f = fixture();
    f.set_run_command(|_, _| Ran {
        code: 0,
        output: String::new(),
    });
    let task = at_plan_review(&f, "Export");

    let moved = act(&f, &task.id, StageAction::Approve);

    assert_eq!(moved.stage, "release");
    assert_eq!(paths(&moved), json!({ "after-plan": "" }));
    assert_eq!(
        payload(&f, &task.id, "stage.approved"),
        json!({ "stage": "plan", "because": "No condition held" })
    );
}

#[test]
fn checks_the_conditions_on_approval_so_an_edit_made_while_the_stage_waited_counts() {
    let f = fixture();
    let task = at_plan_review(&f, "Export");
    write_working(
        &task,
        "context.md",
        "# Export\n\nA [small] change after all.\n",
    );

    let moved = act(&f, &task.id, StageAction::Approve);

    assert_eq!(paths(&moved), small());
}

#[test]
fn enters_the_path_after_pass_of_the_stage_before() {
    enters_after(StageAction::Pass, "stage.passed");
}

#[test]
fn enters_the_path_after_skip_of_the_stage_before() {
    enters_after(StageAction::Skip, "stage.skipped");
}

fn enters_after(what: StageAction, kind: &str) {
    let f = fixture();
    let task = at_plan_review(&f, "Export [small]");

    let moved = act(&f, &task.id, what);

    assert_eq!(moved.stage, "implement");
    assert_eq!(paths(&moved), small());
    let last = payload(&f, &task.id, kind);
    assert_eq!(last["stage"], "plan");
    assert_eq!(last["path"], "small");
}

#[test]
fn enters_the_path_when_the_checks_pass_a_stage_with_an_auto_gate() {
    let f = fixture();
    pipeline(
        &f,
        vec![json!({ "id": "plan", "kind": "agent", "gate": "auto" })],
    );
    let task = f.implementing("plan");

    f.runner
        .run
        .act(&task.id, StageAction::Report, &crate::term_input("1"))
        .unwrap();
    f.settle();

    let now = f.task(&task.id);
    assert_eq!(
        (now.stage.as_str(), now.state),
        ("review", TaskState::Review)
    );
    let checked = payload(&f, &task.id, "stage.checked");
    assert_eq!(checked["stage"], "plan");
    assert_eq!(checked["path"], "pr-review");
}

#[test]
fn enters_the_path_once_ci_passes_the_stage_before() {
    let f = fixture();
    pipeline(
        &f,
        vec![json!({ "id": "plan", "kind": "agent", "gate": "auto", "ci": true })],
    );
    let task = f.implementing("plan");
    f.set_ci(&task.id, "plan", true);

    f.runner
        .run
        .act(&task.id, StageAction::Report, &crate::term_input("1"))
        .unwrap();
    assert_eq!(f.task(&task.id).stage, "plan");
    f.ci.finish(CiResult {
        passed: true,
        passes: vec!["CI passed: CI u".into()],
        ..CiResult::default()
    });
    f.settle();

    let now = f.task(&task.id);
    assert_eq!(
        (now.stage.as_str(), now.state),
        ("review", TaskState::Review)
    );
    assert_eq!(payload(&f, &task.id, "stage.checked")["path"], "pr-review");
}

#[test]
fn enters_the_path_when_the_context_stage_passes_on_its_own() {
    let f = fixture();
    pipeline(&f, vec![]);

    let task = f.created(json!({ "title": "Export [small]" }));

    assert_eq!(task.stage, "implement");
    assert_eq!(paths(&task), small());
    let passed = payload(&f, &task.id, "stage.passed");
    assert_eq!(passed["stage"], "context");
    assert_eq!(passed["path"], "small");
}

#[test]
fn enters_the_path_when_the_last_stage_of_a_group_ends() {
    let f = fixture();
    pipeline(
        &f,
        vec![json!({
            "id": "build",
            "parallel": [
                { "id": "code", "kind": "agent", "gate": "auto" },
                { "id": "notes", "kind": "agent", "gate": "auto" },
            ],
        })],
    );
    let task = f.created(json!({ "title": "Export [small]" }));
    f.apply(
        &task.id,
        json!({ "source": "human", "type": "state.overridden", "payload": { "to": "review", "stage": "build" } }),
    );
    let named = |stage: &str| StageActionInput {
        stage: Some(stage.into()),
        ..Default::default()
    };

    act_with(&f, &task.id, StageAction::Approve, named("code"));
    assert_eq!(paths(&f.task(&task.id)), json!({}));
    let moved = act_with(&f, &task.id, StageAction::Approve, named("notes"));

    assert_eq!(moved.stage, "implement");
    assert_eq!(paths(&moved), small());
    let approved = payload(&f, &task.id, "stage.approved");
    assert_eq!(approved["stage"], "notes");
    assert_eq!(approved["path"], "small");
}

#[test]
fn enters_the_path_when_a_stage_marked_to_skip_before_the_branch_is_skipped() {
    let f = fixture();
    let task = f
        .tasks
        .create_task(
            Some(&json!({ "title": "Export [small]", "agent": "claude-code" })),
            &[],
            true,
        )
        .unwrap();
    f.set_skip(&task.id, "plan", true);

    let now = f.task(&task.id);
    assert_eq!(now.stage, "implement");
    assert_eq!(paths(&now), small());
    let skipped = payload(&f, &task.id, "stage.skipped");
    assert_eq!(skipped["stage"], "plan");
    assert_eq!(skipped["planned"], true);
    assert_eq!(skipped["path"], "small");
}

#[test]
fn goes_from_stage_to_stage_in_the_path_then_to_the_step_after_the_branch() {
    let f = fixture();
    let task = at_plan_review(&f, "Export [small]");
    act(&f, &task.id, StageAction::Approve);

    let docs = act(&f, &task.id, StageAction::Pass);
    assert_eq!(
        (docs.stage.as_str(), docs.state),
        ("docs", TaskState::Running)
    );
    let release = act(&f, &task.id, StageAction::Pass);
    assert_eq!(release.stage, "release");
    assert_eq!(paths(&release), small());
    assert!(!started_stages(&f, &task.id).contains(&json!("review")));
}

#[path = "branch_later.rs"]
mod later;
