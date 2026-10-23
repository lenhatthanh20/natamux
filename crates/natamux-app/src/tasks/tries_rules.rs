//! Tries of a task (`features/tasks/tries.ts`) and the row texts of the Compare tries section
//! (`CompareTries.tsx`).

// Ported with their tests (spec D14); the details panel calls them.
#![cfg_attr(not(test), allow(dead_code))]

use natamux_proto::tasks::task::Task;
use natamux_proto::workspaces::workspace::DiffStat;

use super::task_sort::task_number;

/// `tries-<letters>-<digits>`, in lower case.
fn is_try_label(label: &str) -> bool {
    let Some(rest) = label.strip_prefix("tries-") else {
        return false;
    };
    let Some((letters, digits)) = rest.split_once('-') else {
        return false;
    };
    !letters.is_empty()
        && letters.bytes().all(|byte| byte.is_ascii_lowercase())
        && !digits.is_empty()
        && digits.bytes().all(|byte| byte.is_ascii_digit())
}

/// The label that groups a task with its tries (see fan-out in the daemon), or None.
pub fn try_group(task: &Task) -> Option<&str> {
    task.labels
        .iter()
        .map(String::as_str)
        .find(|label| is_try_label(label))
}

/// Every task of the group, the first try first. The group is named after the first try, whose
/// number may be higher than the others when the user chose its id.
pub fn tries_of<'a>(group: &str, tasks: &'a [Task]) -> Vec<&'a Task> {
    let first = |task: &Task| i32::from(group != format!("tries-{}", task.id.to_lowercase()));
    let mut tries: Vec<&Task> = tasks
        .iter()
        .filter(|task| task.labels.iter().any(|label| label == group))
        .collect();
    tries.sort_by_key(|task| (first(task), task_number(&task.id)));
    tries
}

/// The tries to cancel when one is kept: the others that are not finished yet.
pub fn tries_to_cancel<'a>(kept: &Task, tries: &[&'a Task]) -> Vec<&'a Task> {
    tries
        .iter()
        .copied()
        .filter(|task| task.id != kept.id && !task.state.is_finished())
        .collect()
}

/// "+12 −3, 2 files", or "-" when the try changed nothing or has no stat yet.
pub fn diff_stat_text(stat: Option<&DiffStat>) -> String {
    match stat {
        Some(stat) if stat.files > 0 => {
            format!("+{} −{}, {} files", stat.added, stat.removed, stat.files)
        }
        _ => "-".into(),
    }
}

/// A try that is not finished can be kept.
pub fn can_keep(task: &Task) -> bool {
    !task.state.is_finished()
}

#[cfg(test)]
#[path = "tries_rules_tests.rs"]
mod tests;
