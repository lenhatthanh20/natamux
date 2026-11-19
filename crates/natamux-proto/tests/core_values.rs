//! Lists and limits that the old TypeScript daemon wrote, frozen in `fixtures/`. The tests fail
//! when a value copied into this crate changes by accident.

// The helpers below are test code too: a source that does not match should fail loudly.
#![allow(clippy::unwrap_used, clippy::panic)]

use natamux_proto::agents::start::{LAUNCHABLE_AGENTS, MAX_START_PROMPT_LENGTH};
use natamux_proto::events::kind::KNOWN_KINDS;
use natamux_proto::stages::actions::{MAX_REASON_LENGTH, MAX_STAGE_ID_LENGTH, STAGE_ACTIONS};
use natamux_proto::tasks::task::EDITABLE_FIELDS;
use natamux_proto::workspaces::changes::{
    MAX_AGENT_MESSAGE, MAX_COMMIT_MESSAGE, MAX_FILE_BYTES, MAX_GRAPH_COMMITS, MAX_IMAGE_BYTES,
    MAX_PATH_LENGTH, MAX_ROLLBACK_PATHS,
};
use natamux_proto::workspaces::workspace::{MAX_BASE_LENGTH, MAX_PROMPT_LENGTH, MAX_TRIES};

const FILE_DIFF_TS: &str = include_str!("fixtures/file-diff.ts");
const GRAPH_TS: &str = include_str!("fixtures/graph.ts");
const STAGE_ACTIONS_TS: &str = include_str!("fixtures/stage-actions.ts");
const STAGE_ROUTES_TS: &str = include_str!("fixtures/stages.ts");
const TASK_INPUTS_TS: &str = include_str!("fixtures/task-inputs.ts");
const FAN_OUT_TS: &str = include_str!("fixtures/fan-out.ts");
const EVENT_BUS_TS: &str = include_str!("fixtures/event-bus.ts");
const AGENT_LAUNCH_TS: &str = include_str!("fixtures/agent-launch.ts");
const WORKSPACE_ROUTES_TS: &str = include_str!("fixtures/workspaces.ts");
const AGENT_ROUTES_TS: &str = include_str!("fixtures/agents.ts");

/// The quoted strings of `export const NAME ... = [ ... ]`.
fn list(source: &str, name: &str) -> Vec<String> {
    let start = source
        .find(&format!("export const {name}"))
        .unwrap_or_else(|| panic!("no {name}"));
    let rest = &source[start..];
    let open = rest
        .find("= [")
        .unwrap_or_else(|| panic!("{name} is not a list"))
        + 3;
    let close = open + rest[open..].find(']').unwrap();
    rest[open..close]
        .split('\'')
        .skip(1)
        .step_by(2)
        .map(String::from)
        .collect()
}

/// The right side of `export const NAME = <value>`, up to the end of the line.
fn value<'a>(source: &'a str, name: &str) -> &'a str {
    let marker = format!("export const {name} = ");
    let start = source.find(&marker).unwrap_or_else(|| panic!("no {name}")) + marker.len();
    source[start..].lines().next().unwrap().trim()
}

fn same(rust: &[&str], typescript: Vec<String>, name: &str) {
    assert_eq!(rust, typescript, "{name} differs from the fixture");
}

/// A number written like `1_000_000` in the right side of `export const NAME = `.
fn number(source: &str, name: &str) -> u64 {
    value(source, name).replace('_', "").parse().unwrap()
}

#[test]
fn lists_match_the_fixtures() {
    same(
        &LAUNCHABLE_AGENTS,
        list(AGENT_LAUNCH_TS, "LAUNCHABLE_AGENTS"),
        "LAUNCHABLE_AGENTS",
    );
    same(
        &STAGE_ACTIONS,
        list(STAGE_ACTIONS_TS, "STAGE_ACTIONS"),
        "STAGE_ACTIONS",
    );
    same(
        &EDITABLE_FIELDS,
        list(TASK_INPUTS_TS, "EDITABLE_FIELDS"),
        "EDITABLE_FIELDS",
    );
}

#[test]
fn input_limits_match_the_fixtures() {
    assert!(
        WORKSPACE_ROUTES_TS.contains(&format!("base: z.string().min(1).max({MAX_BASE_LENGTH})"))
    );
    // 20_000 is written with a separator in TypeScript.
    let with_separator = MAX_PROMPT_LENGTH.to_string().replace("20000", "20_000");
    assert!(
        WORKSPACE_ROUTES_TS.contains(&format!("prompt: z.string().trim().max({with_separator})"))
    );
    let start = MAX_START_PROMPT_LENGTH
        .to_string()
        .replace("20000", "20_000");
    assert!(AGENT_ROUTES_TS.contains(&format!("prompt: z.string().max({start}).optional()")));
}

// The kinds the app reads are all in DaemonMessage, with the names written here.
#[test]
fn event_kinds_match_the_daemon() {
    for kind in KNOWN_KINDS {
        assert!(
            EVENT_BUS_TS.contains(&format!("kind: '{kind}'")),
            "{kind} is not in DaemonMessage"
        );
    }
}

#[test]
fn preparing_steps_match_the_fixture() {
    assert!(EVENT_BUS_TS.contains("export type PreparingStep = 'fetch' | 'checkout'"));
}

#[test]
fn stage_input_limits_match_the_fixture() {
    let limit = MAX_STAGE_ID_LENGTH;
    assert!(STAGE_ROUTES_TS.contains(&format!("stage: z.string().min(1).max({limit}).optional()")));
    assert!(STAGE_ROUTES_TS.contains(&format!("to: z.string().min(1).max({limit}).optional()")));
    assert!(STAGE_ROUTES_TS.contains(&format!(
        "const workflowBodySchema = z.object({{ stage: z.string().min(1).max({limit}), on: z.boolean() }})"
    )));
    let reason = MAX_REASON_LENGTH.to_string().replace("20000", "20_000");
    assert!(STAGE_ROUTES_TS.contains(&format!(
        "reason: z.string().trim().min(1).max({reason}).optional()"
    )));
}

#[test]
fn fan_out_limits_match_the_fixtures() {
    assert_eq!(value(FAN_OUT_TS, "MAX_TRIES"), MAX_TRIES.to_string());
    assert!(WORKSPACE_ROUTES_TS.contains("count: z.number().int().min(1).max(MAX_TRIES)"));
}

#[test]
fn git_limits_match_the_daemon() {
    assert_eq!(
        number(GRAPH_TS, "MAX_GRAPH_COMMITS"),
        MAX_GRAPH_COMMITS as u64
    );
    assert_eq!(number(FILE_DIFF_TS, "MAX_IMAGE_BYTES"), MAX_IMAGE_BYTES);
    assert_eq!(number(FILE_DIFF_TS, "MAX_FILE_BYTES"), MAX_FILE_BYTES);
}

#[test]
fn commit_rollback_and_message_limits_match_the_daemon() {
    let commit = MAX_COMMIT_MESSAGE.to_string().replace("10000", "10_000");
    assert!(
        WORKSPACE_ROUTES_TS.contains(&format!("message: z.string().trim().min(1).max({commit})"))
    );
    assert!(WORKSPACE_ROUTES_TS.contains(&format!(
        "paths: z.array(changedFileQuerySchema.shape.path).min(1).max({MAX_ROLLBACK_PATHS})"
    )));
    assert!(
        WORKSPACE_ROUTES_TS.contains(&format!("path: z.string().min(1).max({MAX_PATH_LENGTH})"))
    );
    let text = MAX_AGENT_MESSAGE.to_string().replace("20000", "20_000");
    assert!(AGENT_ROUTES_TS.contains(&format!("text: z.string().trim().min(1).max({text})")));
}
