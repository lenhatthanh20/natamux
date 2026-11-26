//! The sidebar's workspaces by project (`lib/workspace-groups.ts`). Times are passed in as epoch
//! milliseconds; nothing here reads the clock.

use std::borrow::Borrow;
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use natamux_proto::tasks::task::{Task, TaskState};

/// One ISO parser for the app: it takes the text by characters, so any text is safe.
#[cfg(test)]
use crate::tasks::task_time::parse_iso_ms;

pub use crate::tasks::task_attention::is_quiet;
use crate::tasks::task_attention::{Attention, next_quiet_ms};
use crate::tasks::task_changes::task_of;

pub const DEFAULT_REPO_ID: &str = "default";

/// A project, as far as the groups need it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepoRef {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RepoGroup {
    pub repo: RepoRef,
    /// Workspaces still at work, the newest first. The order does not change as agents work, so
    /// a row stays where the eye left it.
    pub open: Vec<Arc<Task>>,
    /// Done or cancelled, with the worktree still on disk.
    pub finished: Vec<Arc<Task>>,
    /// Open subtasks shown under a row of this group instead of on their own.
    pub nested: Vec<Arc<Task>>,
}

/// The tasks are shared with the dashboard: a group costs no copy of a task, and a task that did
/// not change is found equal by its address.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkspaceGroups {
    pub pinned: Vec<Arc<Task>>,
    pub groups: Vec<RepoGroup>,
    /// The open subtasks under each shown parent, by parent id, the oldest first.
    pub children: BTreeMap<String, Vec<Arc<Task>>>,
}

fn newest_first(a: &Task, b: &Task) -> std::cmp::Ordering {
    b.created_at
        .cmp(&a.created_at)
        .then_with(|| b.id.cmp(&a.id))
}

fn is_finished_task(task: &Task) -> bool {
    task.state.is_finished()
}

fn repo_of(task: &Task) -> &str {
    task.repo.as_deref().unwrap_or(DEFAULT_REPO_ID)
}

/// The states that wait for the user (`needsMe` in core).
pub fn needs_me(state: TaskState) -> bool {
    matches!(
        state,
        TaskState::NeedsInput | TaskState::Review | TaskState::Failed | TaskState::Blocked
    )
}

/// The repositories, then the ones only tasks still name (removed ones). Their id is their name.
pub fn known_repos<T: Borrow<Task>>(repos: &[RepoRef], tasks: &[T]) -> Vec<RepoRef> {
    known_repos_of(repos, tasks.iter().map(task_of))
}

fn known_repos_of<'a>(
    repos: &[RepoRef],
    tasks: impl IntoIterator<Item = &'a Task>,
) -> Vec<RepoRef> {
    let mut known = repos.to_vec();
    for task in tasks {
        let id = repo_of(task);
        if !known.iter().any(|repo| repo.id == id) {
            known.push(RepoRef {
                id: id.to_owned(),
                name: id.to_owned(),
            });
        }
    }
    known
}

/// The sidebar's workspaces: the pinned ones, then one group per repository. Every repository has
/// a group, an empty one too, so a workspace can be made in it.
/// `held_id`: a task that finished while it was open stays among the open rows, where it was.
pub fn workspace_groups(
    repos: &[RepoRef],
    tasks: &[Arc<Task>],
    pinned_ids: &[String],
    held_id: Option<&str>,
) -> WorkspaceGroups {
    let tasks: Vec<&Arc<Task>> = tasks.iter().collect();
    workspace_groups_of(repos, &tasks, pinned_ids, held_id)
}

/// `workspace_groups` on borrowed tasks: the tasks that end up in a group are shared, not copied.
pub fn workspace_groups_of(
    repos: &[RepoRef],
    tasks: &[&Arc<Task>],
    pinned_ids: &[String],
    held_id: Option<&str>,
) -> WorkspaceGroups {
    let is_finished = |task: &Task| is_finished_task(task) && Some(task.id.as_str()) != held_id;
    let is_pinned = |id: &str| pinned_ids.iter().any(|pinned| pinned == id);
    let with_workspace: Vec<&Arc<Task>> = tasks
        .iter()
        .copied()
        .filter(|task| task.workspace.is_some())
        .collect();
    let pinned: Vec<Arc<Task>> = pinned_ids
        .iter()
        .filter_map(|id| with_workspace.iter().find(|task| &task.id == id))
        .map(|task| (*task).clone())
        .collect();
    // The tasks that can hold subtasks, by id and project: one lookup per subtask, not a scan.
    let can_hold: HashSet<(&str, &str)> = with_workspace
        .iter()
        .filter(|task| task.parent_id.is_none() && !is_finished(task))
        .map(|task| (task.id.as_str(), repo_of(task)))
        .collect();
    // A finished or pinned subtask stays where the user expects it. An open one stays out of a
    // finished parent, which is folded away in "N finished". One level only.
    let parent_of = |task: &Task| -> Option<String> {
        let parent_id = task.parent_id.as_deref()?;
        if is_finished(task) || is_pinned(&task.id) {
            return None;
        }
        can_hold
            .contains(&(parent_id, repo_of(task)))
            .then(|| parent_id.to_owned())
    };
    let mut oldest_first = with_workspace.clone();
    oldest_first.sort_by(|a, b| newest_first(a, b));
    oldest_first.reverse();
    let mut children: BTreeMap<String, Vec<Arc<Task>>> = BTreeMap::new();
    for task in oldest_first {
        if let Some(parent) = parent_of(task) {
            children.entry(parent).or_default().push(Arc::clone(task));
        }
    }
    let mut rest: Vec<&Arc<Task>> = with_workspace
        .iter()
        .copied()
        .filter(|task| !is_pinned(&task.id) && parent_of(task).is_none())
        .collect();
    rest.sort_by(|a, b| newest_first(a, b));

    let groups = repo_groups(repos, &rest, &children, is_finished);
    WorkspaceGroups {
        pinned,
        groups,
        children,
    }
}

/// One group per repository: its open and finished rows, and the subtasks under them.
fn repo_groups(
    repos: &[RepoRef],
    rest: &[&Arc<Task>],
    children: &BTreeMap<String, Vec<Arc<Task>>>,
    is_finished: impl Fn(&Task) -> bool,
) -> Vec<RepoGroup> {
    known_repos_of(repos, rest.iter().map(|task| task_of(*task)))
        .into_iter()
        .map(|repo| {
            let own: Vec<&Arc<Task>> = rest
                .iter()
                .copied()
                .filter(|task| repo_of(task) == repo.id)
                .collect();
            let split = |finished: bool| -> Vec<Arc<Task>> {
                own.iter()
                    .filter(|task| is_finished(task) == finished)
                    .map(|task| (*task).clone())
                    .collect()
            };
            let nested = own
                .iter()
                .flat_map(|task| children.get(&task.id).cloned().unwrap_or_default())
                .collect();
            RepoGroup {
                open: split(false),
                finished: split(true),
                nested,
                repo,
            }
        })
        .collect()
}

/// The current task, and whether it was still open while it was current. A task that finishes in
/// front of the user keeps its row in place, so the selection and the focus do not jump away.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Hold {
    pub id: Option<String>,
    pub open: bool,
}

pub fn next_hold<T: Borrow<Task>>(hold: &Hold, current_id: Option<&str>, tasks: &[T]) -> Hold {
    let task = current_id.and_then(|id| tasks.iter().map(task_of).find(|each| each.id == id));
    let open = task.is_some_and(|task| !is_finished_task(task));
    let current = current_id.map(str::to_owned);
    if hold.id != current {
        return Hold { id: current, open };
    }
    // The tasks may come after the selection.
    if open && !hold.open {
        Hold { id: current, open }
    } else {
        hold.clone()
    }
}

pub fn held_id(hold: &Hold) -> Option<&str> {
    if hold.open { hold.id.as_deref() } else { None }
}

/// The branch without the "<project>/t-0001-" part, which the row shows already.
pub fn short_branch(task: &Task) -> String {
    let Some(branch) = task.branch.as_deref() else {
        return String::new();
    };
    let prefix = format!("{}-", task.id.to_lowercase());
    branch
        .split_once('/')
        .and_then(|(_, rest)| rest.strip_prefix(&prefix))
        .unwrap_or(branch)
        .to_owned()
}

/// What a folded group hides that the user should still see: a workspace that needs them, else
/// the workspace that is open. The state of that workspace is the color of the dot. None: nothing.
/// How long until the hint of a folded group changes by the clock alone: a running task in it
/// turns quiet, and the hint shows it as one that needs the user.
pub fn next_hint_change_ms(group: &RepoGroup, at: impl Into<Attention>) -> Option<i64> {
    next_quiet_ms(group.open.iter().chain(&group.nested).map(task_of), at)
}

pub fn collapsed_hint(
    group: &RepoGroup,
    current_id: Option<&str>,
    at: impl Into<Attention>,
) -> Option<TaskState> {
    let at = at.into();
    let open = || group.open.iter().chain(&group.nested);
    if let Some(waiting) = open().find(|task| needs_me(task.state) || is_quiet(task, at)) {
        return Some(if needs_me(waiting.state) {
            waiting.state
        } else {
            TaskState::NeedsInput
        });
    }
    open()
        .chain(&group.finished)
        .find(|task| Some(task.id.as_str()) == current_id)
        .map(|task| task.state)
}

#[cfg(test)]
#[path = "workspace_groups_tests.rs"]
mod tests;
// The hint, quiet and hold rules are tested here: they share the helpers of `tests`.
#[cfg(test)]
#[path = "workspace_hint_tests.rs"]
mod hint_tests;
