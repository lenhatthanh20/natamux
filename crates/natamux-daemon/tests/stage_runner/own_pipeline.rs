//! `describe('a pipeline of its own')` of `stage-runner.test.ts`.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use natamux_daemon::stages::act::StageActionInput;
use natamux_proto::stages::schema::{Gate, Stage, StageKind};
use natamux_proto::tasks::task::{Task, TaskState};
use natamux_stages::actions::StageAction;
use natamux_stages::checks::Ran;
use serde_json::{Value, json};

use crate::context::{
    Blocked, act_in_background, act_to, from_terminal, payloads_of, report, set_stages,
};
use crate::{Fixture, run_git};

fn implement_and_review(check: Option<&str>) -> Vec<Stage> {
    vec![
        Stage {
            check: check.map(Into::into),
            ..Stage::new("implement", StageKind::Agent, Gate::Auto)
        },
        Stage::new("review", StageKind::Review, Gate::Manual),
    ]
}

/// The `beforeEach` of the group.
fn fixture() -> Fixture {
    let f = Fixture::new();
    set_stages(&f, implement_and_review(Some("make test")));
    f
}

fn started(f: &Fixture, extra: Value) -> Task {
    let mut input = json!({ "title": "Export", "agent": "claude-code" });
    input
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    let task = f.created(input);
    f.with_workspace(&task.id);
    f.runner
        .run
        .act(&task.id, StageAction::Start, &StageActionInput::default())
        .unwrap()
}

#[test]
fn runs_the_check_command_at_the_root_of_the_worktree() {
    let f = fixture();
    let cwds: Arc<Mutex<Vec<PathBuf>>> = Arc::default();
    let seen = Arc::clone(&cwds);
    f.set_run_command(move |_, cwd| {
        seen.lock().unwrap().push(cwd.to_path_buf());
        Ran {
            code: 0,
            output: String::new(),
        }
    });
    let task = started(&f, json!({}));

    report(&f, &task.id, "1");
    f.settle();

    let expected = vec![PathBuf::from(task.workspace.as_deref().unwrap())];
    assert_eq!(*cwds.lock().unwrap(), expected);
    assert_eq!(f.task(&task.id).stage, "review");
}

#[test]
fn asks_for_a_commit_and_checks_for_it_when_the_settings_turn_it_on() {
    let f = Fixture::new();
    let mut stages = implement_and_review(None);
    stages[0].require_commit = Some(true);
    stages[0].require_clean = Some(true);
    set_stages(&f, stages);
    let task = started(&f, json!({}));
    assert!(
        f.launch_script(&task.id, "1")
            .contains("Commit your work when it is done.")
    );
    let workspace = PathBuf::from(task.workspace.as_deref().unwrap());
    std::fs::write(workspace.join("export.ts"), "export {}\n").unwrap();

    report(&f, &task.id, "1");
    let (_, payload) = f.events(&task.id).pop().unwrap();
    assert_eq!(
        payload["reasons"],
        json!([
            "There is no new commit since the stage started",
            "1 file is not committed"
        ])
    );

    run_git(&workspace, &["add", "."]);
    run_git(&workspace, &["commit", "-m", "Add the export"]);
    report(&f, &task.id, "1");
    f.settle();
    assert_eq!(f.task(&task.id).stage, "review");
}

#[test]
fn drops_the_result_of_checks_that_ran_while_the_stage_started_again() {
    let f = fixture();
    let blocked = Blocked::new(&f);
    let task = started(&f, json!({}));

    let reporting = act_in_background(&f, &task.id, StageAction::Report, from_terminal("1"));
    blocked.wait_for_calls(1);
    act_to(&f, &task.id, StageAction::Reopen, Some("implement"), None);
    blocked.finish(1);
    reporting.join().unwrap();

    assert!(payloads_of(&f, &task.id, "stage.check-failed").is_empty());
    assert_eq!(f.task(&task.id).state, TaskState::Running);
}

#[test]
fn waits_for_the_tasks_it_depends_on_then_starts() {
    let f = fixture();
    let first = f.created(json!({ "title": "Schema" }));
    let task = started(&f, json!({ "dependsOn": [first.id] }));
    assert_eq!(task.state, TaskState::Draft);
    assert!(f.ptys().is_empty());

    f.apply(
        &first.id,
        json!({ "source": "human", "type": "state.overridden", "payload": { "to": "done" } }),
    );

    assert_eq!(f.task(&task.id).state, TaskState::Running);
    assert!(f.launch_script(&task.id, "1").contains("Task: Export"));
}

#[test]
fn runs_a_stage_command_as_one_unit_in_the_same_terminal_each_time() {
    let f = fixture();
    let testing = Stage {
        command: Some("make test; make lint".into()),
        ..Stage::new("testing", StageKind::Testing, Gate::Auto)
    };
    set_stages(
        &f,
        vec![
            testing,
            Stage::new("release", StageKind::Release, Gate::Manual),
        ],
    );
    let task = started(&f, json!({}));
    let script = f.data_dir.join(format!("stage-command-{}.sh", task.id));

    assert_eq!(f.terminals.list(&task.id), ["2"]);
    assert_eq!(f.pty(0).written(), [format!("sh '{}'\r", script.display())]);
    let text = std::fs::read_to_string(&script).unwrap();
    let ticket = text
        .split("NATAMUX_STAGE_TICKET=")
        .nth(1)
        .and_then(|rest| rest.split(' ').next())
        .unwrap()
        .to_string();
    let expected = [
        "if (",
        "make test; make lint",
        "); then",
        &format!(
            "  NATAMUX_STAGE_TICKET={ticket} natamux stage pass {} --stage testing",
            task.id
        ),
        "else",
        &format!(
            "  NATAMUX_STAGE_TICKET={ticket} natamux stage fail {} --stage testing",
            task.id
        ),
        "fi",
        "",
    ]
    .join("\n");
    assert_eq!(text, expected);
    // Security T-0224 (S-01): the script's one-time ticket lets this stage's pass through once.
    let tickets = &f.runner.run.options.stage_tickets;
    assert!(tickets.redeem(&task.id, "testing", &ticket));
    assert!(!tickets.redeem(&task.id, "testing", &ticket));

    act_to(&f, &task.id, StageAction::Reopen, Some("testing"), None);

    assert!(f.pty(0).killed());
    assert_eq!(f.ptys().len(), 2);
    assert_eq!(f.terminals.list(&task.id), ["2"]);
}
