//! Pure rules of the All tasks view (`Dashboard.tsx`, `StateFilter.tsx`, `TaskBoard.tsx`): what
//! main shows, the states of the filter, the board's columns and folds, and when a task can be added.

use natamux_proto::projects::repo::Repo;
use natamux_proto::tasks::task::TaskState;

use super::task_rules::{state_label, uncommitted_sentence};
use super::task_sort::STATE_ORDER;

/// Under this main width the state chips fold into one State button.
pub const MENU_BELOW: f32 = 900.0;
/// The "+N" button, wide enough for "+9".
pub const MORE_WIDTH: f32 = 40.0;
/// Under this main width the layout switch shows icons only and the board's toggle is short.
pub const NARROW_BELOW: f32 = 560.0;

/// What the main area shows under the view header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MainContent {
    /// The task list has not come yet.
    Loading,
    /// The task list never came. `refused`: the daemon rejected the token.
    Error {
        reason: String,
        refused: bool,
    },
    NoTasks,
    /// Tasks exist, but the search or the state filter hides all of them.
    FilterMiss {
        title: String,
        body: &'static str,
    },
    Tasks,
}

/// `down`: the link is down, with its reason and whether the client retries by itself.
pub fn main_content(
    loaded: bool,
    down: Option<(&str, bool)>,
    total: usize,
    visible: usize,
    search: &str,
    state_filter: Option<TaskState>,
) -> MainContent {
    if !loaded {
        return match down {
            Some((reason, retrying)) => MainContent::Error {
                reason: reason.into(),
                refused: !retrying,
            },
            None => MainContent::Loading,
        };
    }
    if total == 0 {
        return MainContent::NoTasks;
    }
    if visible > 0 {
        return MainContent::Tasks;
    }
    let search = search.trim();
    if search.is_empty() {
        let state = state_label(state_filter.unwrap_or(TaskState::Draft)).to_lowercase();
        MainContent::FilterMiss {
            title: format!("No tasks are {state}"),
            body: "Clear the filter to see every task.",
        }
    } else {
        MainContent::FilterMiss {
            title: format!("No tasks match \u{201c}{search}\u{201d}"),
            body: "Check the spelling or clear the filter.",
        }
    }
}

/// The states the filter offers, with their counts: the ones that have tasks, and the active
/// one. The states that need the user come first.
pub fn filter_states(
    counts: &[(TaskState, usize)],
    active: Option<TaskState>,
) -> Vec<(TaskState, usize)> {
    let count_of = |state| {
        counts
            .iter()
            .find(|(known, _)| *known == state)
            .map_or(0, |(_, count)| *count)
    };
    STATE_ORDER
        .iter()
        .map(|state| (*state, count_of(*state)))
        .filter(|(state, count)| *count > 0 || active == Some(*state))
        .collect()
}

/// "Running · 3": a state in the filter menu and on the State button.
pub fn state_count_label(state: TaskState, count: usize) -> String {
    format!("{} \u{b7} {count}", state_label(state))
}

/// The text of the State button: the active state with its count, else "State".
pub fn state_button_text(states: &[(TaskState, usize)], active: Option<TaskState>) -> String {
    active
        .map(|state| {
            let count = states
                .iter()
                .find(|(known, _)| *known == state)
                .map_or(0, |(_, count)| *count);
            state_count_label(state, count)
        })
        .unwrap_or_else(|| "State".into())
}

/// The board's columns: the filtered state alone, else every state, with done and cancelled
/// only when asked for.
pub fn board_states(state_filter: Option<TaskState>, show_finished: bool) -> Vec<TaskState> {
    STATE_ORDER
        .iter()
        .copied()
        .filter(|state| match state_filter {
            Some(filter) => *state == filter,
            None => show_finished || !state.is_finished(),
        })
        .collect()
}

/// The columns the user folded, and the empty columns the user opened. Other empty columns are
/// folded, so the ones with tasks fit in view.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BoardFolds {
    collapsed: Vec<TaskState>,
    expanded: Vec<TaskState>,
}

impl BoardFolds {
    pub fn is_folded(&self, state: TaskState, count: usize, columns: usize) -> bool {
        self.collapsed.contains(&state)
            || (count == 0 && columns > 1 && !self.expanded.contains(&state))
    }

    pub fn set(&mut self, state: TaskState, fold: bool) {
        let (add, remove) = if fold {
            (&mut self.collapsed, &mut self.expanded)
        } else {
            (&mut self.expanded, &mut self.collapsed)
        };
        remove.retain(|known| *known != state);
        if !add.contains(&state) {
            add.push(state);
        }
    }
}

pub fn task_count(count: usize) -> String {
    if count == 1 {
        "1 task".into()
    } else {
        format!("{count} tasks")
    }
}

/// "Delete 3 done tasks?": the title of the board's Delete all confirm.
pub fn delete_all_title(state: TaskState, count: usize) -> String {
    let noun = if count == 1 { "task" } else { "tasks" };
    format!(
        "Delete {count} {} {noun}?",
        state_label(state).to_lowercase()
    )
}

/// `dirty`: the tasks whose workspace has files that are not committed, as (task id, file count).
pub fn delete_all_description(dirty: &[(String, usize)]) -> String {
    let files = uncommitted_sentence(dirty).map_or_else(String::new, |text| text + " ");
    format!(
        "This deletes these tasks, their notes and their history. A worktree folder and the \
         branch go with its task, also commits that are not merged. {files}This cannot be undone."
    )
}

pub fn delete_all_label(any_dirty: bool) -> &'static str {
    if any_dirty { "Delete anyway" } else { "Delete" }
}

/// The project a new task goes to: the one picked, while it still exists, else the first.
pub fn task_repo(repos: &[Repo], picked: Option<&str>) -> Option<String> {
    picked
        .filter(|picked| repos.iter().any(|repo| repo.id == *picked))
        .map(str::to_owned)
        .or_else(|| repos.first().map(|repo| repo.id.clone()))
}

/// Why Add task does nothing now, or None when it can add.
pub fn add_blocked_reason(no_projects: bool, title: &str) -> Option<&'static str> {
    if no_projects {
        Some("Add a project first")
    } else if title.trim().is_empty() {
        Some("Type a title first")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(id: &str) -> Repo {
        Repo {
            id: id.into(),
            name: id.into(),
            path: format!("/code/{id}"),
            worktrees_dir: None,
        }
    }

    #[test]
    fn shows_skeletons_until_the_list_comes_and_the_error_when_it_cannot() {
        assert_eq!(
            main_content(false, None, 0, 0, "", None),
            MainContent::Loading
        );
        assert_eq!(
            main_content(false, Some(("Cannot reach", true)), 0, 0, "", None),
            MainContent::Error {
                reason: "Cannot reach".into(),
                refused: false,
            }
        );
        assert_eq!(
            main_content(false, Some(("rejected", false)), 0, 0, "", None),
            MainContent::Error {
                reason: "rejected".into(),
                refused: true,
            }
        );
        // Once loaded, a dropped link keeps the rows; the status bar says it reconnects.
        assert_eq!(
            main_content(true, Some(("Cannot reach", true)), 2, 2, "", None),
            MainContent::Tasks
        );
    }

    #[test]
    fn tells_no_tasks_from_a_filter_miss() {
        assert_eq!(
            main_content(true, None, 0, 0, "", None),
            MainContent::NoTasks
        );
        assert_eq!(
            main_content(true, None, 3, 0, " zzzz ", None),
            MainContent::FilterMiss {
                title: "No tasks match \u{201c}zzzz\u{201d}".into(),
                body: "Check the spelling or clear the filter.",
            }
        );
        assert_eq!(
            main_content(true, None, 3, 0, "", Some(TaskState::Review)),
            MainContent::FilterMiss {
                title: "No tasks are in review".into(),
                body: "Clear the filter to see every task.",
            }
        );
    }

    #[test]
    fn offers_states_with_tasks_and_the_active_one_needs_you_first() {
        let counts = vec![
            (TaskState::Draft, 2),
            (TaskState::Running, 1),
            (TaskState::NeedsInput, 3),
            (TaskState::Done, 0),
        ];
        assert_eq!(
            filter_states(&counts, Some(TaskState::Done)),
            vec![
                (TaskState::NeedsInput, 3),
                (TaskState::Running, 1),
                (TaskState::Draft, 2),
                (TaskState::Done, 0),
            ]
        );
    }

    #[test]
    fn names_the_state_button_by_the_active_state() {
        let states = vec![(TaskState::Running, 3)];
        assert_eq!(state_button_text(&states, None), "State");
        assert_eq!(
            state_button_text(&states, Some(TaskState::Running)),
            "Running \u{b7} 3"
        );
    }

    #[test]
    fn hides_finished_columns_unless_asked_and_keeps_only_the_filtered_one() {
        assert!(!board_states(None, false).contains(&TaskState::Done));
        assert_eq!(board_states(None, false).len(), 7);
        assert_eq!(board_states(None, true).len(), 9);
        assert_eq!(
            board_states(Some(TaskState::Done), false),
            vec![TaskState::Done]
        );
    }

    #[test]
    fn folds_empty_columns_until_opened_and_full_ones_when_asked() {
        let mut folds = BoardFolds::default();
        assert!(folds.is_folded(TaskState::Blocked, 0, 7));
        // The only column never folds by itself.
        assert!(!folds.is_folded(TaskState::Blocked, 0, 1));
        assert!(!folds.is_folded(TaskState::Running, 2, 7));
        folds.set(TaskState::Blocked, false);
        assert!(!folds.is_folded(TaskState::Blocked, 0, 7));
        folds.set(TaskState::Running, true);
        assert!(folds.is_folded(TaskState::Running, 2, 7));
        folds.set(TaskState::Running, false);
        assert!(!folds.is_folded(TaskState::Running, 2, 7));
    }

    #[test]
    fn words_the_delete_all_confirm() {
        assert_eq!(delete_all_title(TaskState::Done, 3), "Delete 3 done tasks?");
        assert_eq!(
            delete_all_title(TaskState::Cancelled, 1),
            "Delete 1 cancelled task?"
        );
        assert_eq!(task_count(1), "1 task");
        let clean = delete_all_description(&[]);
        assert!(clean.ends_with("also commits that are not merged. This cannot be undone."));
        assert!(!clean.contains("skipped"));
        assert_eq!(delete_all_label(false), "Delete");
        assert_eq!(task_count(4), "4 tasks");
    }

    #[test]
    fn names_the_tasks_with_files_that_are_not_committed_and_deletes_anyway() {
        let text = delete_all_description(&[("T-0002".into(), 3)]);
        assert!(text.contains(
            "not merged. 1 of them has files that are not committed: T-0002 (3 files). Those \
             files are lost too. This cannot be undone."
        ));
        assert_eq!(delete_all_label(true), "Delete anyway");
    }

    #[test]
    fn adds_to_the_picked_project_while_it_exists() {
        let repos = vec![repo("a"), repo("b")];
        assert_eq!(task_repo(&repos, Some("b")).as_deref(), Some("b"));
        assert_eq!(task_repo(&repos, Some("gone")).as_deref(), Some("a"));
        assert_eq!(task_repo(&repos, None).as_deref(), Some("a"));
        assert_eq!(task_repo(&[], None), None);
    }

    #[test]
    fn says_why_add_task_cannot_add() {
        assert_eq!(add_blocked_reason(true, "x"), Some("Add a project first"));
        assert_eq!(add_blocked_reason(false, "  "), Some("Type a title first"));
        assert_eq!(add_blocked_reason(false, "Fix it"), None);
    }
}
