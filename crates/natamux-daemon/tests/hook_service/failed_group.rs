//! The group 'AgentHookService, a failed stage of a group': the stage runner's lookup, on the
//! build group and a review by a person after it.

use std::collections::HashSet;

use natamux_core::stages::terminal_lane::stage_of_terminal;
use natamux_proto::stages::catalog::default_pipeline;
use natamux_proto::stages::position::find_step;
use natamux_proto::stages::schema::{Gate, Stage, StageKind, Step};
use natamux_stages::actions::lane_state;

use super::*;

fn pipeline() -> Vec<Step> {
    default_pipeline()
        .into_iter()
        .flat_map(|step| -> Vec<Step> {
            match step.id() {
                "agent-review" => Vec::new(),
                "testing" => vec![
                    Step::Stage(Stage::new("review", StageKind::Review, Gate::Manual)),
                    step,
                ],
                _ => vec![step],
            }
        })
        .collect()
}

/// The lookup of the stage runner, made of the same core functions.
struct Stages {
    tasks: TaskService,
    /// "<task>/<terminal>" of the agents that ran `natamux done` in this turn.
    reported: Arc<Mutex<HashSet<String>>>,
}

impl StageLookup for Stages {
    fn lane_for(&self, task: &Task, terminal: Option<&str>) -> Result<Option<Lane>, String> {
        let steps = self
            .tasks
            .steps_of(task)
            .map_err(|error| error.to_string())?;
        let Some(step) = find_step(&steps, &task.stage) else {
            return Ok(None);
        };
        let pinned = Vec::new();
        let Some(stage) = stage_of_terminal(&steps, step, terminal.unwrap_or("1"), &pinned) else {
            return Ok(None);
        };
        let state = lane_state(task, step, &stage.id);
        Ok(Some((stage.id, state)))
    }

    fn take_report(&self, task_id: &str, terminal: &str) -> bool {
        let key = format!("{task_id}/{terminal}");
        self.reported.lock().unwrap().remove(&key)
    }
}

struct Group {
    fx: Fx,
    service: HookService,
    reported: Arc<Mutex<HashSet<String>>>,
}

fn setup() -> Group {
    let pipeline = pipeline();
    let settings: SettingsOf = Arc::new(move |_| pipeline.clone());
    let fx = Fx::build(Some(settings), None);
    let reported: Arc<Mutex<HashSet<String>>> = Arc::default();
    let stages = Stages {
        tasks: fx.tasks.clone(),
        reported: Arc::clone(&reported),
    };
    let service = fx.service_with(Arc::new(stages), fx.options(false));
    Group {
        fx,
        service,
        reported,
    }
}

impl Group {
    fn send(&self, id: &str, name: &str, terminal: &str) -> HookResult {
        claude(&self.service, id, &hook(name, json!({})), Some(terminal))
    }

    fn log(&self, id: &str, source: &str, kind: &str, stage: &str) {
        let input = json!({ "source": source, "type": kind, "payload": { "stage": stage } });
        self.fx.apply(id, input);
    }

    /// A task at Build, in a workspace, with Build ready.
    fn build_task(&self) -> String {
        let id = self.fx.make("Fix it").id;
        self.fx.workspace(&id, Some(&format!("/work/{id}")));
        let ready = json!({
            "source": "human", "type": "state.overridden",
            "payload": { "to": "ready", "stage": "build" },
        });
        self.fx.apply(&id, ready);
        id
    }

    // Test spec is done; an API error failed the turn of Implement in Shell 1.
    fn failed_implement(&self) -> String {
        let id = self.build_task();
        self.send(&id, "UserPromptSubmit", "1");
        self.send(&id, "UserPromptSubmit", "2");
        self.log(&id, "system", "stage.checked", "test-spec");
        self.log(&id, "human", "stage.approved", "test-spec");
        self.send(&id, "StopFailure", "1");
        let task = self.fx.task(&id);
        assert_eq!(task.stage, "build");
        assert_eq!(task.state, TaskState::Failed);
        assert_eq!(task.lanes["test-spec"], "done");
        assert_eq!(task.lanes["implement"], "failed");
        id
    }
}

#[test]
fn takes_only_implement_up_again_then_moves_on_to_review_after_natamux_done() {
    let g = setup();
    let id = g.failed_implement();

    assert_eq!(
        g.send(&id, "UserPromptSubmit", "1").applied,
        ["agent.resumed"]
    );
    assert_eq!(g.fx.last_payload(&id)["stage"], "implement");
    let task = g.fx.task(&id);
    assert_eq!(task.stage, "build");
    assert_eq!(task.state, TaskState::Running);
    assert_eq!(task.lanes["test-spec"], "done");
    assert_eq!(task.lanes["implement"], "running");

    // `natamux done`, as the stage runner logs it, then the turn end that follows it.
    g.log(&id, "agent", "stage.reported", "implement");
    g.reported.lock().unwrap().insert(format!("{id}/1"));
    g.log(&id, "system", "stage.checked", "implement");
    assert!(g.send(&id, "Stop", "1").applied.is_empty());

    let task = g.fx.task(&id);
    assert_eq!(task.stage, "review");
    assert_eq!(task.state, TaskState::Review);
}

#[test]
fn resumes_nothing_for_a_hook_from_a_shell_no_stage_uses_or_from_a_finished_stage() {
    let g = setup();
    let id = g.failed_implement();
    let events = g.fx.event_count(&id);

    assert!(g.send(&id, "UserPromptSubmit", "7").applied.is_empty());
    assert_eq!(g.service.shell_state(&id, "7"), Some(TaskState::Running));
    assert!(g.send(&id, "UserPromptSubmit", "2").applied.is_empty());

    assert_eq!(g.fx.event_count(&id), events);
    assert_eq!(
        Value::Object(g.fx.task(&id).lanes),
        json!({ "test-spec": "done", "implement": "failed" })
    );
}

#[test]
fn leaves_a_task_cancelled_in_the_group_alone_when_an_agent_reopened_in_it_works() {
    let g = setup();
    let id = g.build_task();
    g.send(&id, "UserPromptSubmit", "1");
    g.fx.apply(&id, json!({ "source": "human", "type": "task.cancelled" }));
    let events = g.fx.event_count(&id);

    for name in [
        "UserPromptSubmit",
        "Stop",
        "UserPromptSubmit",
        "StopFailure",
    ] {
        assert!(g.send(&id, name, "1").applied.is_empty(), "{name}");
    }

    assert_eq!(g.fx.state(&id), TaskState::Cancelled);
    let logged: Vec<String> = g.fx.kinds(&id).split_off(events);
    let others: Vec<&String> = logged
        .iter()
        .filter(|kind| *kind != "agent.active")
        .collect();
    assert!(others.is_empty(), "{others:?}");
    assert_eq!(g.service.shell_state(&id, "1"), Some(TaskState::Failed));
}
