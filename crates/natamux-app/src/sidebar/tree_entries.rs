//! The sidebar rows as they are on screen, for the keyboard (`features/sidebar/tree-entries.ts`,
//! plus the Up, Down, Home and End rule of `lib/roving.ts`). A row's `toggle` is data, not a
//! callback: the view runs the action it names.

use std::collections::BTreeMap;

use natamux_proto::tasks::task::Task;

use crate::sidebar::shell_rows::ShellRow;
use crate::sidebar::workspace_groups::RepoGroup;

/// What Enter, Left or Right does on a row that can fold.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntryAction {
    /// Make the task the open workspace.
    OpenTask(String),
    /// Open or close a group, a finished list or a task card.
    SetFold { key: String, open: bool },
}

/// A row, in the order they are shown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub key: String,
    pub parent: Option<String>,
    pub expandable: bool,
    pub label: String,
    pub open: Option<bool>,
    pub toggle: Option<EntryAction>,
}

/// What the tree reads from the sidebar's state.
pub struct TreeSource<'a> {
    pub pinned: &'a [Task],
    pub groups: &'a [RepoGroup],
    pub children: &'a BTreeMap<String, Vec<Task>>,
    pub repo_open: &'a dyn Fn(&str) -> bool,
    pub finished_shown: &'a dyn Fn(&str) -> bool,
    pub task_open: &'a dyn Fn(&Task) -> bool,
    pub rows_of: &'a dyn Fn(&Task) -> Vec<ShellRow>,
    pub waiting_children: &'a dyn Fn(&Task) -> Vec<Task>,
}

fn entry(key: String, parent: Option<&str>, label: &str) -> Entry {
    Entry {
        key,
        parent: parent.map(str::to_owned),
        expandable: false,
        label: label.to_owned(),
        open: None,
        toggle: None,
    }
}

/// A card (pinned or open row) always folds; a row of "N finished" is one plain line.
fn push_task(
    source: &TreeSource,
    entries: &mut Vec<Entry>,
    task: &Task,
    parent: Option<&str>,
    card: bool,
) {
    let key = format!("task:{}", task.id);
    let open = card && (source.task_open)(task);
    entries.push(Entry {
        expandable: card,
        open: card.then_some(open),
        toggle: card.then(|| EntryAction::SetFold {
            key: key.clone(),
            open: !open,
        }),
        ..entry(key.clone(), parent, &task.title)
    });
    if !card {
        return;
    }
    if open {
        for shell in (source.rows_of)(task) {
            let shell_key = format!("agent:{}:{}", task.id, shell.key);
            entries.push(entry(shell_key, Some(&key), &shell.label));
        }
    }
    let waiting;
    let shown: &[Task] = if open {
        source.children.get(&task.id).map_or(&[], Vec::as_slice)
    } else {
        waiting = (source.waiting_children)(task);
        &waiting
    };
    for child in shown {
        entries.push(entry(
            format!("task:{}", child.id),
            Some(&key),
            &child.title,
        ));
    }
}

/// The rows as they are on screen.
pub fn tree_entries(source: &TreeSource) -> Vec<Entry> {
    let mut entries = Vec::new();
    for task in source.pinned {
        push_task(source, &mut entries, task, None, true);
    }
    for RepoGroup {
        repo,
        open,
        finished,
        ..
    } in source.groups
    {
        let repo_key = format!("repo:{}", repo.id);
        let shown = (source.repo_open)(&repo.id);
        entries.push(Entry {
            expandable: true,
            open: Some(shown),
            toggle: Some(EntryAction::SetFold {
                key: repo_key.clone(),
                open: !shown,
            }),
            ..entry(repo_key.clone(), None, &repo.name)
        });
        if !shown {
            continue;
        }
        for task in open {
            push_task(source, &mut entries, task, Some(&repo_key), true);
        }
        if !finished.is_empty() {
            let finished_key = format!("finished:{}", repo.id);
            let finished_open = (source.finished_shown)(&repo.id);
            entries.push(Entry {
                expandable: true,
                open: Some(finished_open),
                toggle: Some(EntryAction::SetFold {
                    key: finished_key.clone(),
                    open: !finished_open,
                }),
                ..entry(finished_key.clone(), Some(&repo_key), "")
            });
            if finished_open {
                for task in finished {
                    push_task(source, &mut entries, task, Some(&finished_key), false);
                }
            }
        }
    }
    entries
}

/// What a key does on a tree row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TreeKey {
    /// Not a tree key here: the list handles it.
    Ignored,
    /// A tree key with nothing to do on this row.
    Handled,
    Toggle(EntryAction),
    Focus(String),
}

fn parent_key<'a>(entries: &'a [Entry], key: &str) -> Option<&'a str> {
    entries.iter().find(|row| row.key == key)?.parent.as_deref()
}

fn first_child_key<'a>(entries: &'a [Entry], key: &str) -> Option<&'a str> {
    entries
        .iter()
        .find(|row| row.parent.as_deref() == Some(key))
        .map(|row| row.key.as_str())
}

/// Right unfolds or steps into the children; Left folds or steps to the parent.
pub fn tree_key(event_key: &str, key: &str, entries: &[Entry]) -> TreeKey {
    let row = entries.iter().find(|candidate| candidate.key == key);
    let expandable = row.is_some_and(|row| row.expandable);
    let is_open = row.is_some_and(|row| row.open == Some(true));
    let toggle = row.and_then(|row| row.toggle.clone());
    if event_key == "ArrowRight" && expandable {
        if !is_open {
            return toggle.map_or(TreeKey::Handled, TreeKey::Toggle);
        }
        return first_child_key(entries, key)
            .map_or(TreeKey::Handled, |child| TreeKey::Focus(child.to_owned()));
    }
    if event_key == "ArrowLeft" {
        if expandable && is_open && toggle.is_some() {
            return toggle.map_or(TreeKey::Handled, TreeKey::Toggle);
        }
        if let Some(parent) = parent_key(entries, key) {
            return TreeKey::Focus(parent.to_owned());
        }
    }
    TreeKey::Ignored
}

/// The index a navigation key moves to in a list of `count` items, or None when the key does not
/// move. `letters` adds j and k, for lists where a single letter is not a type-ahead.
pub fn move_target(key: &str, index: usize, count: usize, letters: bool) -> Option<usize> {
    if count == 0 {
        return None;
    }
    let last = count - 1;
    match key {
        "ArrowDown" => Some((index + 1).min(last)),
        "j" if letters => Some((index + 1).min(last)),
        "ArrowUp" => Some(index.saturating_sub(1)),
        "k" if letters => Some(index.saturating_sub(1)),
        "Home" => Some(0),
        "End" => Some(last),
        _ => None,
    }
}

#[cfg(test)]
#[path = "tree_entries_tests.rs"]
mod tests;
