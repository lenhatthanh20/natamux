//! The stage that the usage of a turn counts for, around a report and after a shell ended.

use super::*;

/// A stage runner whose terminal 1 ran `implement`; `report` moves the task on, as `natamux done`
/// does, while its turn has not ended yet.
#[derive(Default)]
struct Runner {
    lane: Mutex<Option<String>>,
    pending: Mutex<Option<String>>,
}

impl Runner {
    fn report(&self, stage: &str, next_lane: Option<&str>) {
        *self.pending.lock().unwrap() = Some(stage.to_string());
        *self.lane.lock().unwrap() = next_lane.map(String::from);
    }
}

impl StageLookup for Runner {
    fn lane_for(&self, task: &Task, _terminal: Option<&str>) -> Result<Option<Lane>, String> {
        let lane = self.lane.lock().unwrap().clone();
        Ok(lane.map(|stage| (stage, task.state)))
    }

    fn take_report(&self, _task_id: &str, _terminal: &str) -> bool {
        self.pending.lock().unwrap().take().is_some()
    }

    fn report_of(&self, _task_id: &str, _terminal: &str) -> Option<String> {
        self.pending.lock().unwrap().clone()
    }
}

type Calls = Arc<Mutex<Vec<(Option<String>, String)>>>;

struct Turns {
    /// Keeps the task's folder.
    _fx: Fx,
    runner: Arc<Runner>,
    service: HookService,
    calls: Calls,
    task: String,
}

const FILE: &str = "/home/me/.claude/projects/w/s-1.jsonl";

fn turns() -> Turns {
    let fx = Fx::new();
    let runner = Arc::new(Runner::default());
    *runner.lane.lock().unwrap() = Some("implement".into());
    let calls: Calls = Arc::default();
    let record = Arc::clone(&calls);
    let mut options = fx.options(false);
    options.usage_seen = Some(Arc::new(
        move |_: &str, stage: Option<&str>, _: &str, file: &str| {
            record
                .lock()
                .unwrap()
                .push((stage.map(String::from), file.to_string()));
        },
    ));
    let stages: Arc<dyn StageLookup> = runner.clone();
    let service = fx.service_with(stages, options);
    let task = fx.make("Fix it").id;
    fx.workspace(&task, Some("/work/T-0001"));
    Turns {
        _fx: fx,
        runner,
        service,
        calls,
        task,
    }
}

impl Turns {
    fn send(&self, name: &str) {
        let raw = hook(name, json!({ "transcript_path": FILE, "tool_name": "Bash" }));
        claude(&self.service, &self.task, &raw, Some("1"));
    }

    fn stages(&self) -> Vec<Option<String>> {
        let calls = self.calls.lock().unwrap();
        calls.iter().map(|(stage, _)| stage.clone()).collect()
    }
}

fn stage(name: &str) -> Option<String> {
    Some(name.to_string())
}

#[test]
fn a_turn_that_reports_counts_for_its_stage_though_the_next_step_runs_elsewhere() {
    let t = turns();
    t.send("PreToolUse");
    // `natamux done` ran: the next step works in another terminal.
    t.runner.report("implement", None);
    t.send("PostToolUse");
    t.send("Stop");

    assert_eq!(t.stages(), [stage("implement")]);
}

#[test]
fn a_turn_that_reports_counts_for_its_stage_though_the_next_stage_runs_in_its_terminal() {
    let t = turns();
    *t.runner.lane.lock().unwrap() = Some("spec".into());
    t.send("PreToolUse");
    t.runner.report("spec", Some("implement"));
    t.send("PostToolUse");
    t.send("Stop");
    t.send("UserPromptSubmit");
    t.send("Stop");

    assert_eq!(t.stages(), [stage("spec"), stage("implement")]);
}

#[test]
fn a_turn_after_the_stage_of_its_terminal_passed_counts_for_that_stage() {
    let t = turns();
    t.send("Stop");
    *t.runner.lane.lock().unwrap() = None;
    // Background work woke the agent while another terminal runs the next step.
    t.send("Stop");

    assert_eq!(t.stages(), [stage("implement"), stage("implement")]);
}

#[test]
fn an_ended_shell_has_its_last_file_read_once_more_for_its_last_stage() {
    let t = turns();
    t.send("PreToolUse");
    t.runner.report("implement", None);
    t.send("PostToolUse");

    // The task finished before the turn ended: its shell was killed.
    t.service.shell_ended(&t.task, "1");
    t.service.shell_ended(&t.task, "1");

    let calls = t.calls.lock().unwrap().clone();
    assert_eq!(calls, [(stage("implement"), FILE.to_string())]);
}

#[test]
fn the_next_agent_in_an_ended_shell_starts_with_no_stage() {
    let t = turns();
    t.send("Stop");
    t.service.shell_ended(&t.task, "1");
    *t.runner.lane.lock().unwrap() = None;

    t.send("Stop");

    assert_eq!(t.stages(), [stage("implement"), stage("implement"), None]);
}
