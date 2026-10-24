//! The sidebar tree as lines on screen, worked out from the dashboard data with no gpui
//! (`WorkspaceList.tsx`): the groups, which task is open, the rows under it, which tasks have their
//! terminals read, and the keyboard entries. The tree view draws the lines in order.

use std::collections::{BTreeMap, BTreeSet};

use natamux_proto::projects::repo::Repo;
use natamux_proto::tasks::task::{Task, TaskState};
use natamux_proto::terminals::list::TerminalList;

use crate::sidebar::shell_rows::{ShellRow, shell_rows};
use crate::sidebar::sidebar_tree::{RowTier, is_fold_open, row_tier, with_parents};
use crate::sidebar::tree_entries::{Entry, TreeSource, tree_entries};
use crate::sidebar::workspace_groups::{
    DEFAULT_REPO_ID, RepoGroup, RepoRef, WorkspaceGroups, workspace_groups,
};
use crate::tasks::task_attention::filter_tasks;

/// What the tree reads from the dashboard and the saved view.
pub struct TreeInput<'a> {
    pub repos: &'a [Repo],
    /// Every task: the filter is applied here.
    pub tasks: &'a [Task],
    pub filter: &'a str,
    /// The task open in main.
    pub current_id: Option<&'a str>,
    /// A task that finished while it was open keeps its place (`workspace_groups::Hold`).
    pub held_id: Option<&'a str>,
    pub pinned: &'a [String],
    pub folds: &'a [(String, bool)],
    pub terminals: &'a BTreeMap<String, TerminalList>,
}

/// The numbers and flags of one project group.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GroupInfo {
    /// The badge: open workspaces, with their subtasks.
    pub count: usize,
    /// Every workspace of the project, also the ones the filter hides.
    pub workspaces: usize,
    /// Workspaces the filter hides.
    pub hidden: usize,
    /// The filter hides every workspace of the group: "No matches in <project> (N hidden)".
    pub all_hidden: bool,
    /// The daemon no longer has the project; it takes no new tasks.
    pub gone: bool,
    pub open: bool,
    pub finished_shown: bool,
}

/// One line of the tree, in the order they are shown. `group` is the index of the project card
/// the line sits in; pinned lines sit in none.
#[derive(Clone, Debug, PartialEq)]
pub enum Line {
    PinnedHeader {
        count: usize,
    },
    GroupHeader {
        group: usize,
    },
    Task {
        task: Box<Task>,
        group: Option<usize>,
        /// Under its parent, behind the guide line.
        nested: bool,
        /// A pinned row names its project.
        pinned_row: bool,
    },
    Terminal {
        task_id: String,
        row: ShellRow,
        group: Option<usize>,
    },
    /// "No workspaces. Start a task"
    NoWorkspaces {
        group: usize,
    },
    /// "No matches in <project> (N hidden)"
    NoMatches {
        group: usize,
    },
    /// "N finished"
    Finished {
        group: usize,
    },
}

impl Line {
    /// The key of a line the keyboard can reach, as the entries name it.
    pub fn key(&self, groups: &[RepoGroup]) -> Option<String> {
        let repo = |group: &usize| groups.get(*group).map(|group| group.repo.id.as_str());
        match self {
            Line::GroupHeader { group } => repo(group).map(|id| format!("repo:{id}")),
            Line::Task { task, .. } => Some(format!("task:{}", task.id)),
            Line::Terminal { task_id, row, .. } => Some(format!("agent:{task_id}:{}", row.key)),
            Line::Finished { group } => repo(group).map(|id| format!("finished:{id}")),
            _ => None,
        }
    }

    /// The project group a line belongs to; the pinned header and pinned rows have none.
    #[cfg(test)]
    pub fn group(&self) -> Option<usize> {
        match self {
            Line::PinnedHeader { .. } => None,
            Line::GroupHeader { group }
            | Line::NoWorkspaces { group }
            | Line::NoMatches { group }
            | Line::Finished { group } => Some(*group),
            Line::Task { group, .. } | Line::Terminal { group, .. } => *group,
        }
    }
}

pub struct TreeModel {
    pub groups: WorkspaceGroups,
    pub infos: Vec<GroupInfo>,
    #[cfg_attr(not(test), allow(dead_code))]
    pub filtered: bool,
    /// The rows under each task.
    rows: BTreeMap<String, Vec<ShellRow>>,
    open: BTreeSet<String>,
    pub entries: Vec<Entry>,
    pub lines: Vec<Line>,
    /// The tasks whose terminals are read: their agents work or wait, or they are open.
    pub reads_terminals: Vec<String>,
}

fn is_finished(task: &Task) -> bool {
    matches!(task.state, TaskState::Done | TaskState::Cancelled)
}

fn repo_of(task: &Task) -> &str {
    task.repo.as_deref().unwrap_or(DEFAULT_REPO_ID)
}

impl TreeModel {
    pub fn build(input: &TreeInput) -> TreeModel {
        let filtered = !input.filter.trim().is_empty();
        let searched = if filtered {
            filter_tasks(input.tasks, None, input.filter)
        } else {
            input.tasks.to_vec()
        };
        let repos: Vec<RepoRef> = input
            .repos
            .iter()
            .map(|repo| RepoRef {
                id: repo.id.clone(),
                name: repo.name.clone(),
            })
            .collect();
        let groups = workspace_groups(
            &repos,
            &with_parents(&searched, input.tasks),
            input.pinned,
            input.held_id,
        );
        let repo_open = |id: &str| is_fold_open(input.folds, &format!("repo:{id}"), true);
        let finished_shown = |id: &str| is_fold_open(input.folds, &format!("finished:{id}"), false);
        // Only the open workspace shows what works under it, or its parent when it is a subtask.
        // A filter shows the subtasks it matched.
        let holds_current = |task: &Task| {
            Some(task.id.as_str()) == input.current_id
                || groups.children.get(&task.id).is_some_and(|children| {
                    children
                        .iter()
                        .any(|child| filtered || Some(child.id.as_str()) == input.current_id)
                })
        };
        let shown = shown_rows(&groups, &repo_open, &finished_shown);
        // A finished task, or one without a workspace, has no terminals left.
        let reads = |task: &Task| {
            !is_finished(task)
                && task.workspace.is_some()
                && (holds_current(task)
                    || matches!(task.state, TaskState::Running | TaskState::NeedsInput))
        };
        let mut reads_terminals: Vec<String> = Vec::new();
        for task in shown.iter().filter(|task| reads(task)) {
            if !reads_terminals.contains(&task.id) {
                reads_terminals.push(task.id.clone());
            }
        }
        let rows = rows_under(&groups, &reads_terminals, input.terminals);
        let can_fold = |task: &Task| {
            groups
                .children
                .get(&task.id)
                .is_some_and(|children| !children.is_empty())
                || rows.get(&task.id).is_some_and(|rows| !rows.is_empty())
        };
        let open: BTreeSet<String> = shown
            .iter()
            .filter(|task| can_fold(task) && holds_current(task))
            .map(|task| task.id.clone())
            .collect();
        let infos = groups
            .groups
            .iter()
            .map(|group| group_info(group, input, filtered, &repo_open, &finished_shown))
            .collect();
        let mut model = TreeModel {
            groups,
            infos,
            filtered,
            rows,
            open,
            entries: Vec::new(),
            lines: Vec::new(),
            reads_terminals,
        };
        model.entries = model.make_entries(&repo_open, &finished_shown);
        model.lines = model.make_lines();
        model
    }

    pub fn rows_of(&self, task_id: &str) -> &[ShellRow] {
        self.rows.get(task_id).map_or(&[], Vec::as_slice)
    }

    /// The task shows the rows under it. The user cannot fold the open one.
    pub fn is_open(&self, task_id: &str) -> bool {
        self.open.contains(task_id)
    }

    pub fn can_fold(&self, task_id: &str) -> bool {
        self.groups
            .children
            .get(task_id)
            .is_some_and(|children| !children.is_empty())
            || !self.rows_of(task_id).is_empty()
    }

    /// A folded row still shows the subtasks that wait on the user: no other part of the sidebar
    /// does.
    pub fn waiting_children(&self, task_id: &str) -> Vec<Task> {
        self.groups
            .children
            .get(task_id)
            .map(|children| {
                children
                    .iter()
                    .filter(|child| row_tier(child.state) == RowTier::Attention)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    fn make_entries(
        &self,
        repo_open: &dyn Fn(&str) -> bool,
        finished_shown: &dyn Fn(&str) -> bool,
    ) -> Vec<Entry> {
        tree_entries(&TreeSource {
            pinned: &self.groups.pinned,
            groups: &self.groups.groups,
            children: &self.groups.children,
            repo_open,
            finished_shown,
            task_open: &|task: &Task| self.is_open(&task.id),
            can_fold: &|task: &Task| self.can_fold(&task.id),
            rows_of: &|task: &Task| self.rows_of(&task.id).to_vec(),
            waiting_children: &|task: &Task| self.waiting_children(&task.id),
        })
    }

    fn push_task(&self, lines: &mut Vec<Line>, task: &Task, group: Option<usize>, pinned: bool) {
        lines.push(Line::Task {
            task: Box::new(task.clone()),
            group,
            nested: false,
            pinned_row: pinned,
        });
        let nested = |child: &Task| Line::Task {
            task: Box::new(child.clone()),
            group,
            nested: true,
            pinned_row: false,
        };
        if self.is_open(&task.id) {
            for row in self.rows_of(&task.id) {
                lines.push(Line::Terminal {
                    task_id: task.id.clone(),
                    row: row.clone(),
                    group,
                });
            }
            let children = self.groups.children.get(&task.id);
            lines.extend(children.into_iter().flatten().map(nested));
        } else {
            lines.extend(self.waiting_children(&task.id).iter().map(nested));
        }
    }

    fn make_lines(&self) -> Vec<Line> {
        let mut lines = Vec::new();
        if !self.groups.pinned.is_empty() {
            lines.push(Line::PinnedHeader {
                count: self.groups.pinned.len(),
            });
            for task in &self.groups.pinned {
                self.push_task(&mut lines, task, None, true);
            }
        }
        for (index, (group, info)) in self.groups.groups.iter().zip(&self.infos).enumerate() {
            lines.push(Line::GroupHeader { group: index });
            if !info.open {
                continue;
            }
            for task in &group.open {
                self.push_task(&mut lines, task, Some(index), false);
            }
            if group.open.is_empty() && !info.all_hidden {
                lines.push(Line::NoWorkspaces { group: index });
            }
            if info.all_hidden {
                lines.push(Line::NoMatches { group: index });
            }
            if !group.finished.is_empty() {
                lines.push(Line::Finished { group: index });
                if info.finished_shown {
                    for task in &group.finished {
                        self.push_task(&mut lines, task, Some(index), false);
                    }
                }
            }
        }
        lines
    }
}

/// The rows in sight: the pinned ones, then the open and shown finished rows of each open group.
fn shown_rows<'a>(
    groups: &'a WorkspaceGroups,
    repo_open: &dyn Fn(&str) -> bool,
    finished_shown: &dyn Fn(&str) -> bool,
) -> Vec<&'a Task> {
    let mut shown: Vec<&Task> = groups.pinned.iter().collect();
    for group in groups
        .groups
        .iter()
        .filter(|group| repo_open(&group.repo.id))
    {
        shown.extend(&group.open);
        if finished_shown(&group.repo.id) {
            shown.extend(&group.finished);
        }
    }
    shown
}

/// The rows under every task. Terminal rows come only for the tasks whose terminals are read.
fn rows_under(
    groups: &WorkspaceGroups,
    reads_terminals: &[String],
    terminals: &BTreeMap<String, TerminalList>,
) -> BTreeMap<String, Vec<ShellRow>> {
    let every = groups
        .pinned
        .iter()
        .chain(groups.groups.iter().flat_map(|group| {
            group
                .open
                .iter()
                .chain(&group.finished)
                .chain(&group.nested)
        }));
    every
        .map(|task| {
            let list = reads_terminals
                .contains(&task.id)
                .then(|| terminals.get(&task.id))
                .flatten();
            (task.id.clone(), shell_rows(task, list))
        })
        .collect()
}

fn group_info(
    group: &RepoGroup,
    input: &TreeInput,
    filtered: bool,
    repo_open: &dyn Fn(&str) -> bool,
    finished_shown: &dyn Fn(&str) -> bool,
) -> GroupInfo {
    let id = group.repo.id.as_str();
    let workspaces = input
        .tasks
        .iter()
        .filter(|task| task.workspace.is_some() && repo_of(task) == id)
        .count();
    let listed = group.open.len() + group.finished.len();
    let hidden = workspaces.saturating_sub(listed + group.nested.len());
    GroupInfo {
        count: group.open.len() + group.nested.len(),
        workspaces,
        hidden,
        all_hidden: filtered && listed == 0 && hidden > 0,
        gone: !input.repos.iter().any(|repo| repo.id == id),
        open: repo_open(id),
        finished_shown: finished_shown(id),
    }
}

#[cfg(test)]
#[path = "tree_model_tests.rs"]
mod tests;
