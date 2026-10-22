//! Port of `createBlockedReason` and `tabToOpen` in `packages/web/src/lib/workspace.ts`.

use natamux_proto::tasks::task::TaskState;

use crate::app::view_rules::WorkspaceTab;

/// Why Start task is off in the Start a task dialog. A source other than manual needs its
/// reference, or the task would start with no context. `skill` is the skill picked in the dialog,
/// None for a source from the project settings. `agent` is None where the dialog has no agent pick.
pub fn create_blocked_reason(
    title: &str,
    source: &str,
    reference: &str,
    skill: Option<&str>,
    agent: Option<&str>,
) -> Option<&'static str> {
    if title.trim().is_empty() {
        return Some("Enter a title");
    }
    if skill == Some("") {
        return Some("Pick a skill");
    }
    if source != "manual" && reference.trim().is_empty() {
        return Some("Enter the reference of the context");
    }
    // A plain shell cannot run a skill.
    (skill.is_some() && agent == Some("")).then_some("Pick an agent to run the skill")
}

#[cfg_attr(not(test), allow(dead_code))]
/// A task whose agent works or waits opens on its terminal. Another task opens on the tab that
/// the user used last in it, and a new one opens on its terminal.
pub fn tab_to_open(state: TaskState, saved: Option<WorkspaceTab>) -> WorkspaceTab {
    if matches!(state, TaskState::Running | TaskState::NeedsInput) {
        return WorkspaceTab::Terminal;
    }
    saved.unwrap_or(WorkspaceTab::Terminal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needs_a_title_and_a_reference_for_a_source_other_than_manual() {
        assert_eq!(
            create_blocked_reason(" ", "manual", "", None, None),
            Some("Enter a title")
        );
        assert_eq!(create_blocked_reason("Fix", "manual", "", None, None), None);
        assert_eq!(
            create_blocked_reason("Fix", "github-issue", " ", None, None),
            Some("Enter the reference of the context")
        );
        assert_eq!(
            create_blocked_reason("Fix", "github-issue", "#4", None, None),
            None
        );
    }

    #[test]
    fn needs_a_skill_a_reference_and_an_agent_for_a_skill_picked_in_the_dialog() {
        let reason = |r, s, a| create_blocked_reason("Fix", "skill", r, s, a);
        assert_eq!(
            reason("", Some(""), Some("claude-code")),
            Some("Pick a skill")
        );
        assert_eq!(
            reason(" ", Some("jira-ticket"), Some("claude-code")),
            Some("Enter the reference of the context")
        );
        assert_eq!(
            reason("ABC-1", Some("jira-ticket"), Some("")),
            Some("Pick an agent to run the skill")
        );
        assert_eq!(reason("ABC-1", Some("jira-ticket"), Some("codex")), None);
        // A skill from the project settings needs no pick.
        assert_eq!(reason("ABC-1", None, Some("codex")), None);
    }

    #[test]
    fn gives_one_reason_at_a_time_title_skill_reference_then_agent() {
        assert_eq!(
            create_blocked_reason(" ", "skill", "", Some(""), Some("")),
            Some("Enter a title")
        );
        assert_eq!(
            create_blocked_reason("Fix", "skill", "", Some(""), Some("")),
            Some("Pick a skill")
        );
        assert_eq!(
            create_blocked_reason("Fix", "skill", "  ", Some("jira-ticket"), Some("")),
            Some("Enter the reference of the context")
        );
    }

    #[test]
    fn needs_no_agent_for_my_notes_or_for_a_skill_from_the_settings() {
        assert_eq!(
            create_blocked_reason("Fix", "manual", "", None, Some("")),
            None
        );
        assert_eq!(
            create_blocked_reason("Fix", "skill", "ABC-1", None, Some("")),
            None
        );
    }

    #[test]
    fn opens_the_terminal_while_the_agent_works_or_waits_whatever_was_used_last() {
        assert_eq!(
            tab_to_open(TaskState::Running, Some(WorkspaceTab::Changes)),
            WorkspaceTab::Terminal
        );
        assert_eq!(
            tab_to_open(TaskState::NeedsInput, Some(WorkspaceTab::History)),
            WorkspaceTab::Terminal
        );
    }

    #[test]
    fn opens_the_last_tab_of_that_workspace_and_the_terminal_for_a_new_one() {
        assert_eq!(
            tab_to_open(TaskState::Review, Some(WorkspaceTab::Changes)),
            WorkspaceTab::Changes
        );
        assert_eq!(
            tab_to_open(TaskState::Ready, Some(WorkspaceTab::History)),
            WorkspaceTab::History
        );
        assert_eq!(tab_to_open(TaskState::Ready, None), WorkspaceTab::Terminal);
    }
}
