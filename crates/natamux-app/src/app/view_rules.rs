//! What the app remembers about the user's views, as pure rules (`lib/view-store/helpers.ts` and
//! `togglePin`). Reading and writing the file is `app/view_state.rs`. A remembered map keeps its
//! insertion order, so "the newest" is the last entry.

// Ported with their tests (spec D11); the details and tab rules serve screens 2 and 3.
#![cfg_attr(not(test), allow(dead_code))]

use natamux_proto::tasks::task::{Task, TaskState};

/// A task id with a remembered value, oldest first.
pub type Entries<T> = Vec<(String, T)>;

// Old tasks do not need a remembered answer forever.
const MAX_REMEMBERED_DETAILS: usize = 200;
const MAX_REMEMBERED_SEEN: usize = 1000;
pub const MAX_RECENT: usize = 20;

/// `editor` shows only while a file of the worktree is open.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceTab {
    Changes,
    History,
    Terminal,
    Docs,
    Editor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailsView {
    Progress,
    Files,
    Details,
}

impl DetailsView {
    /// The names of `natamux.details-view`.
    pub fn name(self) -> &'static str {
        match self {
            DetailsView::Progress => "progress",
            DetailsView::Files => "files",
            DetailsView::Details => "details",
        }
    }

    pub fn parse(text: &str) -> Option<DetailsView> {
        [
            DetailsView::Progress,
            DetailsView::Files,
            DetailsView::Details,
        ]
        .into_iter()
        .find(|view| view.name() == text)
    }
}

/// Until the user picks a view, the details show the progress beside a workspace and the task
/// itself beside the list.
pub fn details_view_for(picked: Option<DetailsView>, in_workspace: bool) -> DetailsView {
    picked.unwrap_or(if in_workspace {
        DetailsView::Progress
    } else {
        DetailsView::Details
    })
}

/// Until the user opens or closes them, the details are open where they dock beside a workspace.
pub fn details_open_for(choice: Option<bool>, docked_beside_workspace: bool) -> bool {
    choice.unwrap_or(docked_beside_workspace)
}

/// The workspaces that were opened, newest first, each once.
pub fn with_recent(current: &[String], task_id: &str) -> Vec<String> {
    std::iter::once(task_id.to_owned())
        .chain(current.iter().filter(|id| id.as_str() != task_id).cloned())
        .take(MAX_RECENT)
        .collect()
}

/// Sets one task's entry, moved to the end. The newest answers stay when there are too many.
fn with_entry<T: Clone>(
    current: &[(String, T)],
    task_id: &str,
    value: T,
    max: usize,
) -> Entries<T> {
    let mut next: Entries<T> = current
        .iter()
        .filter(|(id, _)| id != task_id)
        .cloned()
        .collect();
    next.push((task_id.to_owned(), value));
    let extra = next.len().saturating_sub(max);
    next.drain(..extra);
    next
}

pub fn with_detail_open(current: &[(String, bool)], task_id: &str, open: bool) -> Entries<bool> {
    with_entry(current, task_id, open, MAX_REMEMBERED_DETAILS)
}

pub fn with_tab(
    current: &[(String, WorkspaceTab)],
    task_id: &str,
    tab: WorkspaceTab,
) -> Entries<WorkspaceTab> {
    with_entry(current, task_id, tab, MAX_REMEMBERED_DETAILS)
}

fn find<'a, T>(entries: &'a [(String, T)], task_id: &str) -> Option<&'a T> {
    entries
        .iter()
        .find(|(id, _)| id == task_id)
        .map(|(_, value)| value)
}

/// A task is unseen when its state is not the one it had when the user last looked at it. A task
/// that was never recorded counts as seen: the first visit must not mark everything.
pub fn is_unseen(seen: &[(String, TaskState)], id: &str, state: TaskState) -> bool {
    find(seen, id).is_some_and(|recorded| *recorded != state)
}

/// Records the state the user has seen. Tasks that are recorded already keep their entry unless
/// `overwrite` is set (the user opened that task just now).
pub fn with_seen(
    current: &[(String, TaskState)],
    tasks: &[(&str, TaskState)],
    overwrite: bool,
) -> Entries<TaskState> {
    let mut next = current.to_vec();
    for &(id, state) in tasks {
        if overwrite || find(&next, id).is_none() {
            next = with_entry(&next, id, state, MAX_REMEMBERED_SEEN);
        }
    }
    next
}

/// The seen states after a new task list (the two `markSeen` effects of `Dashboard.tsx`). A task
/// with a workspace seen for the first time is the baseline, so nothing starts out unseen; a
/// backlog task is not recorded, or it would start unseen once it runs. The open task is being
/// looked at, so its new state is seen whatever it does.
pub fn seen_after_list(
    current: &[(String, TaskState)],
    tasks: &[Task],
    open: Option<&str>,
) -> Entries<TaskState> {
    let baseline: Vec<(&str, TaskState)> = tasks
        .iter()
        .filter(|task| task.workspace.is_some())
        .map(|task| (task.id.as_str(), task.state))
        .collect();
    let next = with_seen(current, &baseline, false);
    match open.and_then(|id| tasks.iter().find(|task| task.id == id)) {
        Some(task) => with_seen(&next, &[(task.id.as_str(), task.state)], true),
        None => next,
    }
}

/// The terminal a workspace should show and focus once its terminals are read (`shellRequest`).
/// `name` None: the first agent terminal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellRequest {
    pub task_id: String,
    pub name: Option<String>,
}

/// `openChanges(taskId, 'terminal')`: another task asks for its first agent terminal; the same
/// task keeps the request it has, so a terminal row's ask is not lost.
pub fn request_on_open(
    open: Option<&str>,
    pending: Option<&ShellRequest>,
    task_id: &str,
) -> Option<ShellRequest> {
    if open == Some(task_id) {
        return pending.cloned();
    }
    Some(ShellRequest {
        task_id: task_id.to_owned(),
        name: None,
    })
}

/// Pins the task, or unpins it when it is pinned.
pub fn toggle_pin(pinned: &[String], id: &str) -> Vec<String> {
    if pinned.iter().any(|item| item == id) {
        pinned
            .iter()
            .filter(|item| item.as_str() != id)
            .cloned()
            .collect()
    } else {
        pinned
            .iter()
            .cloned()
            .chain(std::iter::once(id.to_owned()))
            .collect()
    }
}

/// The saved pinned ids: strings of a JSON array; anything else is nothing pinned.
pub fn parse_pinned(raw: Option<&str>) -> Vec<String> {
    let value: serde_json::Value = serde_json::from_str(raw.unwrap_or("[]")).unwrap_or_default();
    match value {
        serde_json::Value::Array(items) => items
            .into_iter()
            .filter_map(|item| item.as_str().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flags(entries: &[(&str, bool)]) -> Entries<bool> {
        entries
            .iter()
            .map(|&(id, open)| (id.to_owned(), open))
            .collect()
    }

    #[test]
    fn with_detail_open_sets_and_changes_the_flag_of_one_task() {
        let opened = with_detail_open(&[], "T-0001", true);
        assert_eq!(opened, flags(&[("T-0001", true)]));
        assert_eq!(
            with_detail_open(&opened, "T-0001", false),
            flags(&[("T-0001", false)])
        );
    }

    #[test]
    fn with_detail_open_keeps_the_other_tasks_as_they_were() {
        let next = with_detail_open(&flags(&[("T-0001", true)]), "T-0002", false);
        assert_eq!(next, flags(&[("T-0001", true), ("T-0002", false)]));
    }

    #[test]
    fn with_detail_open_forgets_the_oldest_answers_when_there_are_too_many() {
        let mut current: Entries<bool> = Vec::new();
        for index in 0..205 {
            current = with_detail_open(&current, &format!("T-{index}"), true);
        }
        assert_eq!(current.len(), 200);
        assert!(find(&current, "T-0").is_none());
        assert_eq!(find(&current, "T-204"), Some(&true));
    }

    #[test]
    fn details_open_for_follows_the_choice_of_the_user() {
        assert!(details_open_for(Some(true), false));
        assert!(details_open_for(Some(true), true));
        assert!(!details_open_for(Some(false), true));
        assert!(!details_open_for(Some(false), false));
    }

    #[test]
    fn details_open_for_opens_the_details_docked_beside_a_workspace_until_the_user_chooses() {
        assert!(details_open_for(None, true));
        assert!(!details_open_for(None, false));
    }

    #[test]
    fn details_view_for_shows_progress_in_a_workspace_and_the_task_beside_the_list() {
        assert_eq!(details_view_for(None, true), DetailsView::Progress);
        assert_eq!(details_view_for(None, false), DetailsView::Details);
        assert_eq!(
            details_view_for(Some(DetailsView::Files), true),
            DetailsView::Files
        );
    }

    #[test]
    fn with_tab_remembers_the_last_tab_of_each_workspace() {
        let first = with_tab(&[], "T-0001", WorkspaceTab::Changes);
        let both = with_tab(&first, "T-0002", WorkspaceTab::History);
        assert_eq!(
            both,
            vec![
                ("T-0001".to_owned(), WorkspaceTab::Changes),
                ("T-0002".to_owned(), WorkspaceTab::History)
            ]
        );
        let again = with_tab(&first, "T-0001", WorkspaceTab::Terminal);
        assert_eq!(again, vec![("T-0001".to_owned(), WorkspaceTab::Terminal)]);
    }

    fn seen(entries: &[(&str, TaskState)]) -> Entries<TaskState> {
        entries
            .iter()
            .map(|&(id, state)| (id.to_owned(), state))
            .collect()
    }

    #[test]
    fn counts_a_task_as_unseen_only_when_its_state_differs_from_the_recorded_one() {
        let recorded = seen(&[("T-0001", TaskState::Running)]);
        assert!(is_unseen(&recorded, "T-0001", TaskState::Review));
        assert!(!is_unseen(&recorded, "T-0001", TaskState::Running));
    }

    #[test]
    fn does_not_mark_a_task_that_was_never_recorded() {
        assert!(!is_unseen(&[], "T-0001", TaskState::Review));
    }

    #[test]
    fn records_a_baseline_without_changing_what_was_recorded() {
        let next = with_seen(
            &seen(&[("T-0001", TaskState::Running)]),
            &[("T-0001", TaskState::Review), ("T-0002", TaskState::Ready)],
            false,
        );
        assert_eq!(
            next,
            seen(&[("T-0001", TaskState::Running), ("T-0002", TaskState::Ready)])
        );
    }

    #[test]
    fn overwrites_the_record_when_the_user_looked_at_the_task() {
        let next = with_seen(
            &seen(&[("T-0001", TaskState::Running)]),
            &[("T-0001", TaskState::Review)],
            true,
        );
        assert_eq!(next, seen(&[("T-0001", TaskState::Review)]));
    }

    #[test]
    fn with_seen_keeps_the_newest_1000() {
        let tasks: Vec<(String, TaskState)> = (0..1005)
            .map(|index| (format!("T-{index}"), TaskState::Ready))
            .collect();
        let refs: Vec<(&str, TaskState)> = tasks
            .iter()
            .map(|(id, state)| (id.as_str(), *state))
            .collect();
        let next = with_seen(&[], &refs, false);
        assert_eq!(next.len(), 1000);
        assert!(find(&next, "T-4").is_none());
        assert!(find(&next, "T-1004").is_some());
    }

    fn task(id: &str, state: TaskState, workspace: bool) -> Task {
        Task {
            id: id.into(),
            state,
            workspace: workspace.then(|| format!("/work/{id}")),
            ..Task::default()
        }
    }

    #[test]
    fn the_open_task_is_seen_again_when_it_changes() {
        let seen = vec![("T-0001".to_owned(), TaskState::Running)];
        let tasks = [task("T-0001", TaskState::Review, true)];
        let next = seen_after_list(&seen, &tasks, Some("T-0001"));
        assert!(!is_unseen(&next, "T-0001", TaskState::Review));
        // Not open: the change shows as unseen.
        let next = seen_after_list(&seen, &tasks, None);
        assert!(is_unseen(&next, "T-0001", TaskState::Review));
    }

    #[test]
    fn the_baseline_records_only_tasks_with_a_workspace() {
        let tasks = [
            task("T-0001", TaskState::Draft, false),
            task("T-0002", TaskState::Running, true),
        ];
        let next = seen_after_list(&[], &tasks, None);
        assert_eq!(next, vec![("T-0002".to_owned(), TaskState::Running)]);
        // The backlog task gets a workspace and runs: it starts out seen.
        let running = [task("T-0001", TaskState::Running, true)];
        let next = seen_after_list(&next, &running, None);
        assert!(!is_unseen(&next, "T-0001", TaskState::Running));
    }

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|id| (*id).to_owned()).collect()
    }

    #[test]
    fn puts_the_newest_first_and_lists_each_workspace_once() {
        assert_eq!(
            with_recent(&ids(&["T-0002", "T-0001"]), "T-0001"),
            ids(&["T-0001", "T-0002"])
        );
        assert_eq!(with_recent(&[], "T-0003"), ids(&["T-0003"]));
    }

    #[test]
    fn keeps_the_newest_20() {
        let mut recent: Vec<String> = Vec::new();
        for index in 0..25 {
            recent = with_recent(&recent, &format!("T-{index}"));
        }
        assert_eq!(recent.len(), 20);
        assert_eq!(recent[0], "T-24");
        assert!(!recent.contains(&"T-0".to_owned()));
    }

    #[test]
    fn opening_another_task_asks_for_its_first_agent_terminal() {
        let pending = ShellRequest {
            task_id: "T-1".into(),
            name: Some("5".into()),
        };
        let other = request_on_open(Some("T-1"), Some(&pending), "T-2");
        assert_eq!(
            other,
            Some(ShellRequest {
                task_id: "T-2".into(),
                name: None
            })
        );
        assert_eq!(
            request_on_open(None, None, "T-2").map(|request| request.name),
            Some(None)
        );
    }

    #[test]
    fn opening_the_same_task_again_keeps_its_request() {
        let pending = ShellRequest {
            task_id: "T-1".into(),
            name: Some("5".into()),
        };
        assert_eq!(
            request_on_open(Some("T-1"), Some(&pending), "T-1"),
            Some(pending)
        );
        assert_eq!(request_on_open(Some("T-1"), None, "T-1"), None);
    }

    #[test]
    fn toggle_pin_pins_and_unpins() {
        let pinned = toggle_pin(&ids(&["T-0001"]), "T-0002");
        assert_eq!(pinned, ids(&["T-0001", "T-0002"]));
        assert_eq!(toggle_pin(&pinned, "T-0001"), ids(&["T-0002"]));
    }

    #[test]
    fn parse_pinned_keeps_strings_and_drops_the_rest() {
        assert_eq!(
            parse_pinned(Some(r#"["T-1", 2, "T-3"]"#)),
            ids(&["T-1", "T-3"])
        );
        assert!(parse_pinned(Some(r#"{"a":1}"#)).is_empty());
        assert!(parse_pinned(Some("not json")).is_empty());
        assert!(parse_pinned(None).is_empty());
    }
}
