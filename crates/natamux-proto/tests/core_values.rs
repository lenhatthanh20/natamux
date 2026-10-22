//! The TypeScript in packages/core stays the source of truth until phase 4. This test reads it
//! and fails when a list, limit or rule copied into this crate no longer matches.

// The helpers below are test code too: a source that does not match should fail loudly.
#![allow(clippy::unwrap_used, clippy::panic)]

use natamux_proto::agents::claude_code;
use natamux_proto::agents::start::{LAUNCHABLE_AGENTS, MAX_START_PROMPT_LENGTH};
use natamux_proto::events::kind::KNOWN_KINDS;
use natamux_proto::events::task_event::{EVENT_SOURCES, EVENT_TYPES, RECORD_ONLY_EVENT_TYPES};
use natamux_proto::projects::repo::CONTEXT_SOURCE_TYPES;
use natamux_proto::tasks::attachments;
use natamux_proto::tasks::task::{
    CHECK_STATES, MAX_TITLE_LENGTH, PULL_REQUEST_STATES, TASK_PRIORITIES, TASK_STATES,
};
use natamux_proto::terminals::list::SHELL_PROGRAMS;
use natamux_proto::workspaces::workspace::{CHANGE_STATUSES, MAX_BASE_LENGTH, MAX_PROMPT_LENGTH};

const TASK_STATE_TS: &str = include_str!("../../../packages/core/src/tasks/task-state.ts");
const TASK_TS: &str = include_str!("../../../packages/core/src/tasks/task.ts");
const EVENTS_TS: &str = include_str!("../../../packages/core/src/events/schema.ts");
const PROJECT_SETTINGS_TS: &str =
    include_str!("../../../packages/core/src/projects/project-settings.ts");
const REPO_TS: &str = include_str!("../../../packages/core/src/projects/repo.ts");
const ATTACHMENTS_TS: &str = include_str!("../../../packages/core/src/tasks/attachments.ts");
const TERMINAL_TS: &str = include_str!("../../../packages/core/src/terminals/terminal.ts");
const CHANGES_TS: &str = include_str!("../../../packages/core/src/git/changes.ts");
const GIT_GRAPH_TS: &str = include_str!("../../../packages/core/src/git/git-graph.ts");
const TERMINAL_LANE_TS: &str = include_str!("../../../packages/core/src/stages/terminal-lane.ts");
const EVENT_BUS_TS: &str = include_str!("../../../packages/daemon/src/shared/event-bus.ts");
const AGENT_LAUNCH_TS: &str = include_str!("../../../packages/daemon/src/agents/agent-launch.ts");
const WORKSPACE_ROUTES_TS: &str =
    include_str!("../../../packages/daemon/src/http/routes/workspaces.ts");
const AGENT_ROUTES_TS: &str = include_str!("../../../packages/daemon/src/http/routes/agents.ts");
const HOOKS_TS: &str = include_str!("../../../packages/core/src/agents/claude-code-hooks.ts");

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
    assert_eq!(rust, typescript, "{name} differs from packages/core");
}

#[test]
fn lists_match_core() {
    same(
        &TASK_STATES,
        list(TASK_STATE_TS, "TASK_STATES"),
        "TASK_STATES",
    );
    same(
        &TASK_PRIORITIES,
        list(TASK_TS, "TASK_PRIORITIES"),
        "TASK_PRIORITIES",
    );
    same(
        &EVENT_SOURCES,
        list(EVENTS_TS, "EVENT_SOURCES"),
        "EVENT_SOURCES",
    );
    same(&EVENT_TYPES, list(EVENTS_TS, "EVENT_TYPES"), "EVENT_TYPES");
    same(
        &RECORD_ONLY_EVENT_TYPES,
        list(EVENTS_TS, "RECORD_ONLY_EVENT_TYPES"),
        "RECORD_ONLY_EVENT_TYPES",
    );
    same(
        &CONTEXT_SOURCE_TYPES,
        list(PROJECT_SETTINGS_TS, "CONTEXT_SOURCE_TYPES"),
        "CONTEXT_SOURCE_TYPES",
    );
    same(
        &PULL_REQUEST_STATES,
        list(TASK_TS, "PULL_REQUEST_STATES"),
        "PULL_REQUEST_STATES",
    );
    same(&CHECK_STATES, list(TASK_TS, "CHECK_STATES"), "CHECK_STATES");
    same(
        &CHANGE_STATUSES,
        list(CHANGES_TS, "CHANGE_STATUSES"),
        "CHANGE_STATUSES",
    );
    same(
        &SHELL_PROGRAMS,
        list(TERMINAL_TS, "SHELL_PROGRAMS"),
        "SHELL_PROGRAMS",
    );
    same(
        &LAUNCHABLE_AGENTS,
        list(AGENT_LAUNCH_TS, "LAUNCHABLE_AGENTS"),
        "LAUNCHABLE_AGENTS",
    );
    same(
        &claude_code::CLAUDE_CODE_HOOK_EVENTS,
        list(HOOKS_TS, "CLAUDE_CODE_HOOK_EVENTS"),
        "CLAUDE_CODE_HOOK_EVENTS",
    );
}

#[test]
fn hook_names_match_core() {
    let quoted = |text: &str| format!("'{text}'");
    assert_eq!(
        value(HOOKS_TS, "TASK_ID_HEADER"),
        quoted(claude_code::TASK_ID_HEADER)
    );
    assert_eq!(
        value(HOOKS_TS, "TERMINAL_HEADER"),
        quoted(claude_code::TERMINAL_HEADER)
    );
    assert_eq!(
        value(HOOKS_TS, "TOKEN_ENV_VAR"),
        quoted(claude_code::TOKEN_ENV_VAR)
    );
    assert_eq!(
        value(HOOKS_TS, "TASK_ENV_VAR"),
        quoted(claude_code::TASK_ENV_VAR)
    );
    assert_eq!(
        value(HOOKS_TS, "TERMINAL_ENV_VAR"),
        quoted(claude_code::TERMINAL_ENV_VAR)
    );
}

#[test]
fn attachment_limits_match_core() {
    let number = |name: &str| {
        value(ATTACHMENTS_TS, name)
            .replace('_', "")
            .parse::<u64>()
            .unwrap()
    };
    assert_eq!(
        number("MAX_ATTACHMENT_COUNT"),
        attachments::MAX_ATTACHMENT_COUNT as u64
    );
    assert_eq!(
        number("MAX_ATTACHMENT_SIZE"),
        attachments::MAX_ATTACHMENT_SIZE
    );
    assert_eq!(
        number("MAX_TASK_ATTACHMENT_SIZE"),
        attachments::MAX_TASK_ATTACHMENT_SIZE
    );
}

// is_task_id and is_repo_id are written by hand from these rules.
#[test]
fn id_rules_match_core() {
    assert!(TASK_TS.contains(".max(32)\n  .regex(/^[A-Z]+-\\d+$/"));
    assert!(REPO_TS.contains(".regex(/^[a-z0-9]+(-[a-z0-9]+)*$/"));
    assert!(REPO_TS.contains(".max(60)"));
}

#[test]
fn input_limits_match_core() {
    assert!(TASK_TS.contains(&format!(
        "title: z.string().trim().min(1).max({MAX_TITLE_LENGTH})"
    )));
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
fn preparing_steps_and_push_kinds_match_core() {
    assert!(EVENT_BUS_TS.contains("export type PreparingStep = 'fetch' | 'checkout'"));
    for remote in ["none", "new", "tracked"] {
        assert!(
            GIT_GRAPH_TS.contains(&format!("remote: z.literal('{remote}')")),
            "{remote}"
        );
    }
}

// A lane without `agent` counts as an agent, as in the dashboard (`isAgentShell`).
#[test]
fn lane_agent_is_null_or_missing_in_core() {
    assert!(TERMINAL_LANE_TS.contains("agent: z.string().min(1).nullable().optional()"));
}

// The ended shell fields of `EndedShell`, in the order of the daemon's schema.
#[test]
fn ended_shell_fields_match_core() {
    let start = TERMINAL_TS.find("export const endedShellSchema").unwrap();
    let schema = &TERMINAL_TS[start..start + TERMINAL_TS[start..].find("})").unwrap()];
    let fields: Vec<&str> = schema
        .lines()
        .skip(1)
        .filter_map(|line| line.trim().split(':').next())
        .collect();
    assert_eq!(
        fields,
        ["name", "agent", "stage", "session", "endedAt", "text"]
    );
}
