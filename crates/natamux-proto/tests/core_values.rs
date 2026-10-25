//! The TypeScript in packages/core stays the source of truth until phase 4. This test reads it
//! and fails when a list, limit or rule copied into this crate no longer matches.

// The helpers below are test code too: a source that does not match should fail loudly.
#![allow(clippy::unwrap_used, clippy::panic)]

use natamux_proto::agents::claude_code;
use natamux_proto::agents::start::{LAUNCHABLE_AGENTS, MAX_START_PROMPT_LENGTH};
use natamux_proto::events::kind::KNOWN_KINDS;
use natamux_proto::events::task_event::{EVENT_SOURCES, EVENT_TYPES, RECORD_ONLY_EVENT_TYPES};
use natamux_proto::projects::repo::CONTEXT_SOURCE_TYPES;
use natamux_proto::stages::actions::{MAX_REASON_LENGTH, MAX_STAGE_ID_LENGTH, STAGE_ACTIONS};
use natamux_proto::stages::catalog::catalog;
use natamux_proto::stages::schema::{GATES, STAGE_KINDS};
use natamux_proto::tasks::attachments;
use natamux_proto::tasks::task::{
    CHECK_STATES, EDITABLE_FIELDS, MAX_TITLE_LENGTH, PULL_REQUEST_STATES, TASK_PRIORITIES,
    TASK_STATES,
};
use natamux_proto::tasks::template::{MAX_TEMPLATE_NAME_LENGTH, MAX_TEMPLATE_TITLE_LENGTH};
use natamux_proto::terminals::list::SHELL_PROGRAMS;
use natamux_proto::workspaces::changes::{
    COMMIT_SUBJECT_MAX, ChangeStatus, IMAGE_TYPES, ImageSide, MAX_AGENT_MESSAGE,
    MAX_COMMIT_MESSAGE, MAX_FILE_BYTES, MAX_GRAPH_COMMITS, MAX_IMAGE_BYTES, MAX_PATH_LENGTH,
    MAX_ROLLBACK_PATHS,
};
use natamux_proto::workspaces::workspace::{
    CHANGE_STATUSES, MAX_BASE_LENGTH, MAX_PROMPT_LENGTH, MAX_TRIES,
};

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
const COMMIT_MESSAGE_TS: &str = include_str!("../../../packages/core/src/git/commit-message.ts");
const CHANGE_LABELS_TS: &str = include_str!("../../../packages/web/src/lib/changes.ts");
const FILE_DIFF_TS: &str = include_str!("fixtures/file-diff.ts");
const GRAPH_TS: &str = include_str!("fixtures/graph.ts");
const TERMINAL_LANE_TS: &str = include_str!("../../../packages/core/src/stages/terminal-lane.ts");
const STAGE_SCHEMA_TS: &str = include_str!("../../../packages/core/src/stages/schema.ts");
const STAGE_CATALOG_TS: &str = include_str!("../../../packages/core/src/stages/stage-catalog.ts");
const TEMPLATE_TS: &str = include_str!("../../../packages/core/src/tasks/template.ts");
const STAGE_ACTIONS_TS: &str = include_str!("fixtures/stage-actions.ts");
const STAGE_ROUTES_TS: &str = include_str!("fixtures/stages.ts");
const TASK_INPUTS_TS: &str = include_str!("fixtures/task-inputs.ts");
const FAN_OUT_TS: &str = include_str!("fixtures/fan-out.ts");
const EVENT_BUS_TS: &str = include_str!("fixtures/event-bus.ts");
const AGENT_LAUNCH_TS: &str = include_str!("fixtures/agent-launch.ts");
const WORKSPACE_ROUTES_TS: &str = include_str!("fixtures/workspaces.ts");
const AGENT_ROUTES_TS: &str = include_str!("fixtures/agents.ts");
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

#[test]
fn stage_lists_match_core() {
    same(
        &STAGE_KINDS,
        list(STAGE_SCHEMA_TS, "STAGE_KINDS"),
        "STAGE_KINDS",
    );
    same(&GATES, list(STAGE_SCHEMA_TS, "GATES"), "GATES");
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

/// The text of `key: <value>` in one stage object, without quotes.
fn field<'a>(object: &'a str, key: &str) -> Option<&'a str> {
    let marker = format!("{key}: ");
    let at = object.find(&marker)? + marker.len();
    let rest = &object[at..];
    let end = rest.find([',', '\n', '}']).unwrap_or(rest.len());
    // A list is read whole, up to its closing bracket.
    let end = if rest.starts_with('[') {
        rest.find(']').unwrap() + 1
    } else {
        end
    };
    Some(rest[..end].trim().trim_matches('\''))
}

// The stage objects of STAGE_CATALOG hold no braces inside, so the first `}` ends one.
fn catalog_objects() -> Vec<&'static str> {
    STAGE_CATALOG_TS
        .split("stage: {")
        .skip(1)
        .map(|part| &part[..part.find('}').unwrap()])
        .collect()
}

#[test]
fn the_stage_catalog_matches_core() {
    let objects = catalog_objects();
    let stages = catalog();
    assert_eq!(objects.len(), stages.len(), "the number of catalog stages");
    for (object, stage) in objects.iter().zip(stages) {
        let id = field(object, "id").unwrap();
        assert_eq!(id, stage.id, "the order of the catalog");
        let name = |value: &serde_json::Value| value.as_str().unwrap().to_owned();
        let kind = name(&serde_json::to_value(stage.kind).unwrap());
        let gate = name(&serde_json::to_value(stage.gate).unwrap());
        assert_eq!(field(object, "kind"), Some(kind.as_str()), "{id} kind");
        assert_eq!(field(object, "gate"), Some(gate.as_str()), "{id} gate");
        assert_eq!(
            field(object, "output"),
            stage.output.as_deref(),
            "{id} output"
        );
        assert_eq!(field(object, "name"), stage.name.as_deref(), "{id} name");
        for (key, value) in [
            ("workflow", stage.workflow),
            ("ci", stage.ci),
            ("ownShell", stage.own_shell),
        ] {
            assert_eq!(
                field(object, key) == Some("true"),
                value == Some(true),
                "{id} {key}"
            );
        }
        let written: Option<Vec<String>> = field(object, "sections").map(|text| {
            let inner = text.trim_start_matches('[').trim_end_matches(']');
            inner
                .split(',')
                .map(|name| name.trim().trim_matches('\'').to_owned())
                .collect()
        });
        assert_eq!(written, stage.sections, "{id} sections");
    }
}

#[test]
fn stage_input_limits_match_core() {
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
fn template_and_fan_out_limits_match_core() {
    assert!(TEMPLATE_TS.contains(&format!(
        "name: z.string().trim().min(1).max({MAX_TEMPLATE_NAME_LENGTH})"
    )));
    assert!(TEMPLATE_TS.contains(&format!(
        "title: z.string().trim().min(1).max({MAX_TEMPLATE_TITLE_LENGTH})"
    )));
    assert_eq!(value(FAN_OUT_TS, "MAX_TRIES"), MAX_TRIES.to_string());
    assert!(WORKSPACE_ROUTES_TS.contains("count: z.number().int().min(1).max(MAX_TRIES)"));
}

/// A number written like `1_000_000` in the right side of `export const NAME = `.
fn number(source: &str, name: &str) -> u64 {
    value(source, name).replace('_', "").parse().unwrap()
}

#[test]
fn change_labels_match_the_web_dashboard() {
    let start = CHANGE_LABELS_TS.find("export const CHANGE_LABEL").unwrap();
    let object = &CHANGE_LABELS_TS[start..];
    let object = &object[..object.find("\n}").unwrap()];
    let written: Vec<(&str, &str)> = object
        .lines()
        .skip(1)
        .map(|line| {
            let (letter, label) = line.trim().split_once(": ").unwrap();
            (letter, label.trim_end_matches(',').trim_matches('\''))
        })
        .collect();
    let rust: Vec<(&str, &str)> = ChangeStatus::ALL
        .iter()
        .map(|status| (status.letter(), status.label()))
        .collect();
    assert_eq!(rust, written);
}

#[test]
fn image_types_and_sides_match_core() {
    let start = CHANGES_TS.find("export const IMAGE_TYPES").unwrap();
    let object = &CHANGES_TS[start..];
    let object = &object[..object.find("} as const").unwrap()];
    let written: Vec<(&str, &str)> = object
        .lines()
        .skip(1)
        .map(|line| {
            let (extension, mime) = line.trim().split_once(": ").unwrap();
            (extension, mime.trim_end_matches(',').trim_matches('\''))
        })
        .collect();
    assert_eq!(IMAGE_TYPES.to_vec(), written);
    // The route reads `side` as one of these two words.
    assert_eq!(ImageSide::Original.name(), "original");
    assert_eq!(ImageSide::Modified.name(), "modified");
    assert_eq!(list(CHANGES_TS, "IMAGE_SIDES"), ["original", "modified"]);
}

#[test]
fn git_limits_match_the_daemon() {
    assert_eq!(
        number(COMMIT_MESSAGE_TS, "COMMIT_SUBJECT_MAX"),
        COMMIT_SUBJECT_MAX as u64
    );
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
