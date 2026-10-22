//! The sidebar's workspaces by project (`lib/workspace-groups.ts`). Times are passed in as epoch
//! milliseconds; nothing here reads the clock.

use std::collections::BTreeMap;

use natamux_proto::tasks::task::{Task, TaskState};

/// One ISO parser for the app: it takes the text by characters, so any text is safe.
pub use crate::tasks::task_time::parse_iso_ms;

pub const DEFAULT_REPO_ID: &str = "default";

// A running task with no news for this long counts as one that needs the user.
const QUIET_AFTER_MS: i64 = 10 * 60 * 1000;

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
    pub open: Vec<Task>,
    /// Done or cancelled, with the worktree still on disk.
    pub finished: Vec<Task>,
    /// Open subtasks shown under a row of this group instead of on their own.
    pub nested: Vec<Task>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorkspaceGroups {
    pub pinned: Vec<Task>,
    pub groups: Vec<RepoGroup>,
    /// The open subtasks under each shown parent, by parent id, the oldest first.
    pub children: BTreeMap<String, Vec<Task>>,
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

/// A running task that has said nothing for 10 minutes and waits for no background work.
pub fn is_quiet(task: &Task, now_ms: i64) -> bool {
    task.state == TaskState::Running
        && task.background_since.is_none()
        && parse_iso_ms(&task.updated_at).is_some_and(|updated| now_ms - updated >= QUIET_AFTER_MS)
}

/// The repositories, then the ones only tasks still name (removed ones). Their id is their name.
pub fn known_repos(repos: &[RepoRef], tasks: &[Task]) -> Vec<RepoRef> {
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
    tasks: &[Task],
    pinned_ids: &[String],
    held_id: Option<&str>,
) -> WorkspaceGroups {
    let is_finished = |task: &Task| is_finished_task(task) && Some(task.id.as_str()) != held_id;
    let is_pinned = |id: &str| pinned_ids.iter().any(|pinned| pinned == id);
    let with_workspace: Vec<&Task> = tasks
        .iter()
        .filter(|task| task.workspace.is_some())
        .collect();
    let pinned: Vec<Task> = pinned_ids
        .iter()
        .filter_map(|id| with_workspace.iter().find(|task| &task.id == id))
        .map(|task| (*task).clone())
        .collect();
    // A finished or pinned subtask stays where the user expects it. An open one stays out of a
    // finished parent, which is folded away in "N finished". One level only.
    let parent_of = |task: &Task| -> Option<String> {
        let parent_id = task.parent_id.as_deref()?;
        if is_finished(task) || is_pinned(&task.id) {
            return None;
        }
        with_workspace
            .iter()
            .find(|other| {
                other.id == parent_id
                    && other.parent_id.is_none()
                    && !is_finished(other)
                    && repo_of(other) == repo_of(task)
            })
            .map(|other| other.id.clone())
    };
    let mut oldest_first = with_workspace.clone();
    oldest_first.sort_by(|a, b| newest_first(a, b));
    oldest_first.reverse();
    let mut children: BTreeMap<String, Vec<Task>> = BTreeMap::new();
    for task in oldest_first {
        if let Some(parent) = parent_of(task) {
            children.entry(parent).or_default().push(task.clone());
        }
    }
    let mut rest: Vec<Task> = with_workspace
        .iter()
        .filter(|task| !is_pinned(&task.id) && parent_of(task).is_none())
        .map(|task| (*task).clone())
        .collect();
    rest.sort_by(newest_first);

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
    rest: &[Task],
    children: &BTreeMap<String, Vec<Task>>,
    is_finished: impl Fn(&Task) -> bool,
) -> Vec<RepoGroup> {
    known_repos(repos, rest)
        .into_iter()
        .map(|repo| {
            let own: Vec<&Task> = rest
                .iter()
                .filter(|task| repo_of(task) == repo.id)
                .collect();
            let split = |finished: bool| -> Vec<Task> {
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

pub fn next_hold(hold: &Hold, current_id: Option<&str>, tasks: &[Task]) -> Hold {
    let task = current_id.and_then(|id| tasks.iter().find(|each| each.id == id));
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

/// The branch without the "natamux/t-0001-" part, which the row shows already.
pub fn short_branch(task: &Task) -> String {
    let Some(branch) = task.branch.as_deref() else {
        return String::new();
    };
    let prefix = format!("natamux/{}-", task.id.to_lowercase());
    branch.strip_prefix(&prefix).unwrap_or(branch).to_owned()
}

/// What a folded group hides that the user should still see: a workspace that needs them, else
/// the workspace that is open. The state of that workspace is the color of the dot. None: nothing.
pub fn collapsed_hint(
    group: &RepoGroup,
    current_id: Option<&str>,
    now_ms: i64,
) -> Option<TaskState> {
    let open = || group.open.iter().chain(&group.nested);
    if let Some(waiting) = open().find(|task| needs_me(task.state) || is_quiet(task, now_ms)) {
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
