use natamux_proto::tasks::task::TaskState;

use super::*;

const SHELL: &str = "T-0001/1";

fn lane(stage: &str) -> Lane {
    (stage.to_string(), TaskState::Running)
}

fn hook<'a>(lane: Option<&'a Lane>, pending: Option<&str>, reported: bool) -> TurnStage<'a> {
    TurnStage {
        lane,
        pending: pending.map(str::to_string),
        reported,
    }
}

#[test]
fn a_hook_counts_for_the_lane_of_its_terminal() {
    let mut usage = UsageStages::default();
    let implement = lane("implement");

    let stage = usage.note(SHELL, hook(Some(&implement), None, false), None);

    assert_eq!(stage.as_deref(), Some("implement"));
}

#[test]
fn the_hooks_after_a_report_count_for_the_reported_stage_when_the_lane_is_gone() {
    let mut usage = UsageStages::default();
    let implement = lane("implement");
    usage.note(SHELL, hook(Some(&implement), None, false), None);

    // The tool hook of `natamux done` itself: the next step runs in another terminal.
    let tool = usage.note(SHELL, hook(None, Some("implement"), false), None);
    let end = usage.note(SHELL, hook(None, Some("implement"), true), None);

    assert_eq!(tool.as_deref(), Some("implement"));
    assert_eq!(end.as_deref(), Some("implement"));
}

#[test]
fn the_hooks_after_a_report_count_for_the_reported_stage_when_the_next_stage_runs_there() {
    let mut usage = UsageStages::default();
    let (spec, implement) = (lane("spec"), lane("implement"));
    usage.note(SHELL, hook(Some(&spec), None, false), None);

    let tool = usage.note(SHELL, hook(Some(&implement), Some("spec"), false), None);
    let end = usage.note(SHELL, hook(Some(&implement), Some("spec"), true), None);
    let next = usage.note(SHELL, hook(Some(&implement), None, false), None);

    assert_eq!(tool.as_deref(), Some("spec"));
    assert_eq!(end.as_deref(), Some("spec"));
    assert_eq!(next.as_deref(), Some("implement"));
}

#[test]
fn a_turn_end_with_a_report_and_no_known_stage_counts_for_the_stage_before() {
    let mut usage = UsageStages::default();
    let (implement, review) = (lane("implement"), lane("review"));
    usage.note(SHELL, hook(Some(&implement), None, false), None);

    let end = usage.note(SHELL, hook(Some(&review), None, true), None);

    assert_eq!(end.as_deref(), Some("implement"));
}

#[test]
fn an_agent_whose_stage_passed_still_works_for_that_stage() {
    let mut usage = UsageStages::default();
    let implement = lane("implement");
    usage.note(SHELL, hook(Some(&implement), None, false), None);

    let woken = usage.note(SHELL, hook(None, None, false), None);

    assert_eq!(woken.as_deref(), Some("implement"));
}

#[test]
fn a_terminal_that_never_had_a_stage_counts_for_none() {
    let mut usage = UsageStages::default();

    assert_eq!(usage.note("T-0001/2", hook(None, None, false), None), None);
    assert_eq!(usage.note("T-0001/2", hook(None, None, true), None), None);
}

#[test]
fn an_ended_shell_gives_its_last_stage_and_file_once() {
    let mut usage = UsageStages::default();
    let ci = lane("ci-cd-watch");
    let file = Some(("claude-code", "/home/me/s.jsonl"));
    usage.note(SHELL, hook(Some(&ci), None, false), file);

    let (stage, named) = usage.take(SHELL);

    assert_eq!(stage.as_deref(), Some("ci-cd-watch"));
    let expected = ("claude-code".to_string(), "/home/me/s.jsonl".to_string());
    assert_eq!(named, Some(expected));
    assert_eq!(usage.take(SHELL), (None, None));
    // The next agent in this shell starts with no stage.
    assert_eq!(usage.note(SHELL, hook(None, None, false), None), None);
}
