//! Pure rules about tasks (`packages/web/src/lib/tasks.ts`): state and stage labels, the actions
//! of a state, the text of the confirmations and the agent button. Live changes are in
//! `task_changes.rs`, the quiet check and filtering in `task_attention.rs`, sorting in
//! `task_sort.rs`, time text in `task_time.rs`.

use natamux_proto::tasks::task::{Task, TaskContext, TaskState};

use super::task_time::{AgeStyle, format_age};
/// `stageName`: one catalog table for the sidebar and the table.
pub use crate::sidebar::sidebar_tree::stage_name;

/// The name of a state in the dashboard. It is not `TaskState::label`: review reads "In review".
pub fn state_label(state: TaskState) -> &'static str {
    match state {
        TaskState::Review => "In review",
        other => other.label(),
    }
}

pub fn state_from_name(name: &str) -> TaskState {
    serde_json::from_value(serde_json::Value::String(name.into())).unwrap_or(TaskState::Unknown)
}

/// Needs-input, review, failed and blocked wait for the user.
pub fn needs_me(state: TaskState) -> bool {
    matches!(
        state,
        TaskState::NeedsInput | TaskState::Review | TaskState::Failed | TaskState::Blocked
    )
}

/// Each stage of a group with its own state, for the tooltip of the task state. None outside a
/// group.
pub fn lane_summary(task: &Task) -> Option<String> {
    if task.lanes.is_empty() {
        return None;
    }
    let parts: Vec<String> = task
        .lanes
        .iter()
        .map(|(id, state)| {
            let state = state_from_name(state.as_str().unwrap_or_default());
            format!("{}: {}", stage_name(id), state_label(state))
        })
        .collect();
    Some(parts.join(" · "))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskAction {
    pub label: &'static str,
    /// The event the daemon gets, such as `task.blocked`.
    pub event_type: &'static str,
    pub hotkey: Option<char>,
    pub confirm: bool,
}

const fn action(label: &'static str, event_type: &'static str) -> TaskAction {
    TaskAction {
        label,
        event_type,
        hotkey: None,
        confirm: false,
    }
}

/// What a task in this state can do, ending with Cancel. Nothing for a finished task.
pub fn actions_for(state: TaskState) -> Vec<TaskAction> {
    let mut actions = match state {
        TaskState::Draft => vec![action("Mark ready", "task.readied")],
        TaskState::Ready | TaskState::Running => vec![action("Block", "task.blocked")],
        TaskState::NeedsInput => vec![action("Resume", "agent.resumed")],
        TaskState::Review => vec![
            TaskAction {
                hotkey: Some('a'),
                ..action("Approve", "review.approved")
            },
            TaskAction {
                hotkey: Some('r'),
                ..action("Request changes", "review.changes-requested")
            },
        ],
        TaskState::Blocked => vec![action("Unblock", "task.unblocked")],
        TaskState::Failed => vec![action("Retry", "task.retried")],
        TaskState::Done | TaskState::Cancelled | TaskState::Unknown => return Vec::new(),
    };
    actions.push(TaskAction {
        confirm: true,
        ..action("Cancel task", "task.cancelled")
    });
    actions
}

/// What the delete confirmation says. The worktree folder and the branch go with the task, also
/// commits that are not merged, and the `uncommitted` files of the worktree.
pub fn delete_task_description(uncommitted: usize) -> String {
    let files = match uncommitted {
        0 => String::new(),
        1 => "1 file in the worktree is not committed. It is lost too. ".into(),
        n => format!("{n} files in the worktree are not committed. They are lost too. "),
    };
    format!(
        "This deletes the task, its notes and its history. The worktree folder and the branch go \
         with it, also commits that are not merged. {files}This cannot be undone."
    )
}

pub fn delete_task_label(uncommitted: usize) -> &'static str {
    if uncommitted == 0 {
        "Delete task"
    } else {
        "Delete anyway"
    }
}

/// What a cancel stops and what it keeps.
pub fn cancel_task_description(has_workspace: bool) -> String {
    let last = "A cancelled task cannot be started again.";
    if has_workspace {
        format!(
            "The agent and its terminals stop now. \
             The commits, the branch and the workspace stay. {last}"
        )
    } else {
        last.into()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentAction {
    Start,
    Restart,
}

/// What the agent button does. Nothing while the agent works or waits, and nothing for a finished
/// task. After it ended or failed, starting it again is a restart.
pub fn agent_action(state: TaskState, agent: Option<&str>) -> Option<AgentAction> {
    match state {
        TaskState::Running | TaskState::NeedsInput | TaskState::Done | TaskState::Cancelled => None,
        _ if agent.is_none() => Some(AgentAction::Start),
        _ => Some(AgentAction::Restart),
    }
}

/// The stage a task is in, such as "Plan". None while it has none: a draft, or a finished task.
pub fn stage_label(task: &Task) -> Option<String> {
    match task.state {
        TaskState::Draft | TaskState::Done | TaskState::Cancelled => None,
        _ => Some(stage_name(&task.stage)),
    }
}

/// "Plan · Needs input": the stage and its state in one text.
pub fn stage_state_label(task: &Task) -> String {
    match stage_label(task) {
        Some(stage) => format!("{stage} · {}", state_label(task.state)),
        None => state_label(task.state).into(),
    }
}

// Ported with its tests (spec D11); the details panel uses it.
#[cfg_attr(not(test), allow(dead_code))]
/// What a screen reader says for a row: "Fix token refresh race, Needs input, T-0008, 34m ago".
pub fn row_label(task: &Task, now_ms: i64, offset_minutes: i32) -> String {
    format!(
        "{}, {}, {}, {}",
        task.title,
        state_label(task.state),
        task.id,
        format_age(&task.updated_at, now_ms, AgeStyle::Long, offset_minutes)
    )
}

#[cfg_attr(not(test), allow(dead_code))]
/// "linear (skill) LIN-7": the source by its name when it has one, then the reference.
pub fn context_text(context: &TaskContext) -> String {
    let kind = match context.source.as_str() {
        "manual" => "My notes",
        "github-issue" => "GitHub issue",
        "skill" => "skill",
        "mcp" => "MCP server",
        other => other,
    };
    let source = match context.skill.as_ref().or(context.name.as_ref()) {
        Some(name) => format!("{name} ({kind})"),
        None => kind.into(),
    };
    if context.reference.is_empty() {
        source
    } else {
        format!("{source} {}", context.reference)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::tasks::task_time::parse_iso_ms;

    fn make(overrides: impl FnOnce(&mut Task)) -> Task {
        let mut task = Task {
            id: "T-0001".into(),
            title: "Example".into(),
            state: TaskState::Draft,
            stage: "implement".into(),
            created_at: "2026-10-08T09:00:00.000Z".into(),
            updated_at: "2026-10-08T09:00:00.000Z".into(),
            ..Task::default()
        };
        overrides(&mut task);
        task
    }

    fn at_ms(time: &str) -> i64 {
        parse_iso_ms(time).unwrap()
    }

    const ALL_STATES: [TaskState; 9] = [
        TaskState::Draft,
        TaskState::Ready,
        TaskState::Running,
        TaskState::NeedsInput,
        TaskState::Review,
        TaskState::Blocked,
        TaskState::Failed,
        TaskState::Done,
        TaskState::Cancelled,
    ];

    #[test]
    fn offers_approve_and_request_changes_for_a_task_in_review() {
        let actions = actions_for(TaskState::Review);
        let labels: Vec<_> = actions.iter().map(|action| action.label).collect();
        assert_eq!(labels, ["Approve", "Request changes", "Cancel task"]);
        let keys: Vec<_> = actions.iter().map(|action| action.hotkey).collect();
        assert_eq!(keys, [Some('a'), Some('r'), None]);
    }

    #[test]
    fn offers_nothing_for_a_finished_task() {
        assert!(actions_for(TaskState::Done).is_empty());
        assert!(actions_for(TaskState::Cancelled).is_empty());
    }

    #[test]
    fn asks_to_confirm_only_the_cancel_action() {
        for state in ALL_STATES {
            for action in actions_for(state) {
                assert_eq!(action.confirm, action.event_type == "task.cancelled");
            }
        }
    }

    #[test]
    fn names_the_stage_of_a_task_that_is_in_progress() {
        let task = make(|t| {
            t.stage = "test-spec".into();
            t.state = TaskState::NeedsInput;
        });
        assert_eq!(stage_label(&task).as_deref(), Some("Test spec"));
        assert_eq!(stage_state_label(&task), "Test spec · Needs input");
    }

    #[test]
    fn names_the_waiting_state_the_same_way_for_screen_readers() {
        let task = make(|t| t.state = TaskState::NeedsInput);
        let now = at_ms("2026-10-08T09:00:00.000Z");
        assert!(row_label(&task, now, 0).contains(", Needs input, "));
        assert_eq!(
            row_label(&task, now, 0),
            "Example, Needs input, T-0001, just now"
        );
    }

    #[test]
    fn has_no_stage_for_a_draft_or_a_finished_task() {
        for state in [TaskState::Draft, TaskState::Done, TaskState::Cancelled] {
            let task = make(|t| t.state = state);
            assert_eq!(stage_label(&task), None);
            assert_eq!(stage_state_label(&task), state_label(state));
        }
    }

    #[test]
    fn names_stages_from_the_catalog_and_the_id() {
        assert_eq!(stage_name("ux-design"), "UX design");
        assert_eq!(stage_name("agent-review"), "Review");
        assert_eq!(stage_name("implement"), "Implement");
        assert_eq!(stage_name(""), "");
        assert_eq!(stage_name("staging"), "Staging check");
    }

    #[test]
    fn lists_each_stage_of_a_group_with_its_state() {
        let task = make(|t| {
            t.stage = "build".into();
            t.state = TaskState::Review;
            t.lanes.insert("test-spec".into(), json!("review"));
            t.lanes.insert("implement".into(), json!("running"));
        });
        assert_eq!(
            lane_summary(&task).as_deref(),
            Some("Test spec: In review · Implement: Running")
        );
    }

    #[test]
    fn is_empty_outside_a_group() {
        assert_eq!(lane_summary(&make(|t| t.state = TaskState::Running)), None);
    }

    #[test]
    fn starts_an_agent_when_none_was_started() {
        assert_eq!(
            agent_action(TaskState::Ready, None),
            Some(AgentAction::Start)
        );
        assert_eq!(
            agent_action(TaskState::Draft, None),
            Some(AgentAction::Start)
        );
    }

    #[test]
    fn has_nothing_to_offer_while_the_agent_works_or_waits() {
        assert_eq!(agent_action(TaskState::Running, Some("claude-code")), None);
        assert_eq!(
            agent_action(TaskState::NeedsInput, Some("claude-code")),
            None
        );
    }

    #[test]
    fn restarts_an_agent_that_ended_or_failed() {
        assert_eq!(
            agent_action(TaskState::Failed, Some("claude-code")),
            Some(AgentAction::Restart)
        );
        assert_eq!(
            agent_action(TaskState::Review, Some("claude-code")),
            Some(AgentAction::Restart)
        );
    }

    #[test]
    fn has_nothing_to_offer_for_a_finished_task_which_no_agent_can_move() {
        assert_eq!(agent_action(TaskState::Done, Some("claude-code")), None);
        assert_eq!(
            agent_action(TaskState::Cancelled, Some("claude-code")),
            None
        );
        assert_eq!(agent_action(TaskState::Cancelled, None), None);
    }

    fn context(
        source: &str,
        name: Option<&str>,
        skill: Option<&str>,
        reference: &str,
    ) -> TaskContext {
        TaskContext {
            source: source.into(),
            name: name.map(Into::into),
            skill: skill.map(Into::into),
            reference: reference.into(),
        }
    }

    #[test]
    fn names_the_source_when_it_has_a_name_then_the_reference() {
        assert_eq!(
            context_text(&context("skill", Some("linear"), None, "LIN-7")),
            "linear (skill) LIN-7"
        );
        assert_eq!(
            context_text(&context("mcp", Some("jira"), None, "")),
            "jira (MCP server)"
        );
        assert_eq!(
            context_text(&context("skill", None, None, "LIN-7")),
            "skill LIN-7"
        );
        assert_eq!(
            context_text(&context("github-issue", None, None, "#42")),
            "GitHub issue #42"
        );
    }

    #[test]
    fn names_a_skill_picked_for_the_task() {
        assert_eq!(
            context_text(&context("skill", None, Some("jira-ticket"), "ABC-1")),
            "jira-ticket (skill) ABC-1"
        );
        assert_eq!(
            context_text(&context("skill", Some("Ticket"), Some("jira-ticket"), "")),
            "jira-ticket (skill)"
        );
    }

    #[test]
    fn says_what_stops_and_what_stays() {
        assert_eq!(
            cancel_task_description(true),
            "The agent and its terminals stop now. The commits, the branch and the workspace stay. \
             A cancelled task cannot be started again."
        );
        assert_eq!(
            cancel_task_description(false),
            "A cancelled task cannot be started again."
        );
    }

    #[test]
    fn says_what_a_delete_removes() {
        let text = delete_task_description(0);
        assert!(text.contains("The worktree folder and the branch go with it"));
        assert!(text.contains("not merged. This cannot be undone."));
        assert!(!text.contains("not committed"));
        assert_eq!(delete_task_label(0), "Delete task");
    }

    #[test]
    fn names_the_files_that_are_not_committed_and_deletes_anyway() {
        assert!(delete_task_description(1).contains(
            "not merged. 1 file in the worktree is not committed. It is lost too. This cannot"
        ));
        assert!(
            delete_task_description(3)
                .contains("3 files in the worktree are not committed. They are lost too.")
        );
        assert_eq!(delete_task_label(3), "Delete anyway");
    }
}
