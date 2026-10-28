//! Pure rules of the Remove workspace dialog (`RemoveWorkspaceDialog.tsx`) and the Remove
//! finished workspaces confirm (`ClearFinishedDialog.tsx`): what they say goes and what stays.

use natamux_proto::tasks::task::Task;
use natamux_proto::terminals::list::TerminalList;
use natamux_proto::workspaces::workspace::ChangeEntry;

use super::finished_rules::{busy_shells, is_agent_shell};
use crate::tasks::task_rules::state_label;

/// Files listed in the dialog that asks for a commit first.
pub const SHOWN_FILES: usize = 5;

pub const NOT_COMMITTED: &str =
    "Commit the changes, or discard them in Changes. Remove anyway deletes them with the folder.";

pub const KEEPS: &str = "Keeps the task, its notes, docs and history.";

pub const REMOVE_ANYWAY: &str = "Remove anyway";

pub fn not_committed_title(count: usize) -> String {
    if count == 1 {
        "1 file is not committed".into()
    } else {
        format!("{count} files are not committed")
    }
}

/// The warn row once Remove anyway was picked.
pub fn discards_text(count: usize) -> String {
    if count == 1 {
        "Discards 1 file that is not committed.".into()
    } else {
        format!("Discards {count} files that are not committed.")
    }
}

/// The files shown, and the "and N more" line under them.
pub fn shown_files(files: &[ChangeEntry]) -> (&[ChangeEntry], Option<String>) {
    let shown = &files[..files.len().min(SHOWN_FILES)];
    let more = files.len().saturating_sub(SHOWN_FILES);
    (shown, (more > 0).then(|| format!("and {more} more")))
}

fn shell_count(count: usize, other: bool) -> String {
    let word = if other { " other" } else { "" };
    if count == 1 {
        format!("1{word} terminal")
    } else {
        format!("{count}{word} terminals")
    }
}

/// What removing the workspace does to its terminals. An agent shell has its own row, "Stops the
/// agent".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShellsGone {
    /// (terminal, program) of each terminal where a program that is no agent runs.
    pub busy: Vec<(String, String)>,
    /// "Ends N terminals…", when plain terminals end.
    pub plain: Option<String>,
}

/// `shells` is None while the terminals are read.
pub fn shells_gone(shells: Option<&TerminalList>) -> ShellsGone {
    let Some(shells) = shells else {
        return ShellsGone::default();
    };
    let busy = busy_shells(shells);
    let others = shells
        .terminals
        .iter()
        .filter(|name| !is_agent_shell(shells, name))
        .count();
    let plain = others.saturating_sub(busy.len());
    let text = if busy.is_empty() {
        format!("Ends {}. Nothing runs in them.", shell_count(plain, false))
    } else {
        format!("Ends {}.", shell_count(plain, true))
    };
    ShellsGone {
        plain: (plain > 0).then_some(text),
        busy,
    }
}

pub fn busy_text(terminal: &str, program: &str) -> String {
    format!("Stops {program} in Terminal {terminal}")
}

/// The warn row of a task still at work.
pub fn agent_stops(task: &Task) -> Option<String> {
    (!task.state.is_finished()).then(|| {
        format!(
            "Stops the agent. {} stays {}.",
            task.id,
            state_label(task.state).to_lowercase()
        )
    })
}

pub fn remove_title(task_id: &str) -> String {
    format!("Remove the workspace of {task_id}?")
}

/// `discard`: Remove anyway was picked, so files that are not committed go too.
pub fn confirm_label(any_busy: bool, removing: bool, discard: bool) -> &'static str {
    match (removing, discard, any_busy) {
        (true, _, _) => "Removing…",
        (false, true, _) => "Discard and remove",
        (false, false, true) => "Stop it and remove",
        (false, false, false) => "Remove workspace",
    }
}

pub fn remove_finished_title(count: usize) -> String {
    if count == 1 {
        "Remove 1 finished workspace?".into()
    } else {
        format!("Remove {count} finished workspaces?")
    }
}

pub fn remove_finished_description(repo_name: &str) -> String {
    format!(
        "This deletes the worktree folders of these tasks in {repo_name}. A branch stays when it \
         has commits that are not merged. The tasks stay too. A workspace with changes that are \
         not committed is skipped."
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use natamux_proto::tasks::task::TaskState;
    use natamux_proto::terminals::list::TerminalLane;

    use super::*;

    fn change(path: &str) -> ChangeEntry {
        ChangeEntry {
            path: path.into(),
            status: "M".into(),
            original_path: None,
            added: None,
            removed: None,
        }
    }

    fn shells(programs: &[(&str, Option<&str>)], agents: &[&str]) -> TerminalList {
        let mut list = TerminalList {
            terminals: programs.iter().map(|(name, _)| name.to_string()).collect(),
            programs: programs
                .iter()
                .map(|(name, program)| (name.to_string(), program.map(str::to_string)))
                .collect::<BTreeMap<_, _>>(),
            ..TerminalList::default()
        };
        for name in agents {
            let lane: TerminalLane = serde_json::from_value(serde_json::json!({
                "stage": "implement",
                "agent": "claude-code"
            }))
            .unwrap();
            list.lanes.insert(name.to_string(), lane);
        }
        list
    }

    #[test]
    fn asks_for_a_commit_and_lists_five_files_then_the_rest_as_a_count() {
        let files: Vec<ChangeEntry> = (0..7).map(|i| change(&format!("f{i}.rs"))).collect();
        let (shown, more) = shown_files(&files);
        assert_eq!(shown.len(), 5);
        assert_eq!(more.as_deref(), Some("and 2 more"));
        assert_eq!(not_committed_title(7), "7 files are not committed");
        assert_eq!(not_committed_title(1), "1 file is not committed");
        assert_eq!(shown_files(&files[..2]).1, None);
    }

    #[test]
    fn says_the_terminals_end_and_nothing_runs_in_them() {
        let list = shells(
            &[("1", Some("claude")), ("2", Some("zsh")), ("3", None)],
            &["1"],
        );
        let gone = shells_gone(Some(&list));
        assert!(gone.busy.is_empty());
        assert_eq!(
            gone.plain.as_deref(),
            Some("Ends 2 terminals. Nothing runs in them.")
        );
    }

    #[test]
    fn names_each_program_that_stops_and_counts_the_other_terminals() {
        let list = shells(&[("2", Some("node")), ("3", Some("zsh"))], &[]);
        let gone = shells_gone(Some(&list));
        assert_eq!(gone.busy, vec![("2".to_string(), "node".to_string())]);
        assert_eq!(gone.plain.as_deref(), Some("Ends 1 other terminal."));
        assert_eq!(busy_text("2", "node"), "Stops node in Terminal 2");
        assert_eq!(confirm_label(true, false, false), "Stop it and remove");
    }

    #[test]
    fn says_nothing_of_terminals_while_they_are_read_or_when_none_end() {
        assert_eq!(shells_gone(None), ShellsGone::default());
        let only_agent = shells(&[("1", Some("claude"))], &["1"]);
        assert_eq!(shells_gone(Some(&only_agent)).plain, None);
    }

    #[test]
    fn says_the_agent_stops_and_the_state_stays_for_a_task_at_work() {
        let mut task: Task = serde_json::from_value(serde_json::json!({
            "id": "T-1",
            "title": "Fix",
            "createdAt": "2026-10-22T09:00:00.000Z",
            "updatedAt": "2026-10-22T09:00:00.000Z"
        }))
        .unwrap();
        task.state = TaskState::Review;
        assert_eq!(
            agent_stops(&task).as_deref(),
            Some("Stops the agent. T-1 stays in review.")
        );
        task.state = TaskState::Done;
        assert_eq!(agent_stops(&task), None);
    }

    #[test]
    fn shows_removing_while_the_daemon_works() {
        assert_eq!(confirm_label(false, false, false), "Remove workspace");
        assert_eq!(confirm_label(true, true, false), "Removing…");
        assert_eq!(confirm_label(false, true, true), "Removing…");
        assert_eq!(remove_title("T-1"), "Remove the workspace of T-1?");
    }

    #[test]
    fn says_what_remove_anyway_discards() {
        assert_eq!(discards_text(1), "Discards 1 file that is not committed.");
        assert_eq!(discards_text(3), "Discards 3 files that are not committed.");
        assert_eq!(confirm_label(false, false, true), "Discard and remove");
        assert_eq!(confirm_label(true, false, true), "Discard and remove");
    }

    #[test]
    fn counts_the_finished_workspaces_in_the_title() {
        assert_eq!(remove_finished_title(1), "Remove 1 finished workspace?");
        assert_eq!(remove_finished_title(3), "Remove 3 finished workspaces?");
        assert!(remove_finished_description("natamux").contains("these tasks in natamux."));
    }
}
