//! Pure rules of the task table and the board: the cell texts (`lib/pull-request.ts`,
//! `features/tasks/usage-format.ts`, `DiffSize.tsx`), what a task waits for (core `waitsFor`),
//! the row menu of `TaskTable.tsx`, and the keys of a list (`lib/roving.ts`).

use natamux_proto::tasks::task::{Task, TaskState};
use natamux_proto::workspaces::workspace::DiffStat;

use super::task_rules::{AgentAction, TaskAction, actions_for, agent_action};

/// The tasks this task waits for: the ones it depends on that are not done, and its open
/// subtasks while no stage has started yet. A task that does not exist is not waited for.
pub fn waits_for(task: &Task, tasks: &[Task]) -> Vec<String> {
    let state_of = |id: &str| tasks.iter().find(|other| other.id == id).map(|t| t.state);
    let mut waits: Vec<String> = task
        .depends_on
        .iter()
        .filter(|id| state_of(id).is_some_and(|state| state != TaskState::Done))
        .cloned()
        .collect();
    if matches!(task.state, TaskState::Ready | TaskState::Draft) {
        waits.extend(
            tasks
                .iter()
                .filter(|other| {
                    other.parent_id.as_deref() == Some(task.id.as_str())
                        && !other.state.is_finished()
                })
                .map(|other| other.id.clone()),
        );
    }
    waits
}

/// "https://github.com/o/r/pull/12" becomes "#12".
pub fn pull_request_number(url: &str) -> String {
    let digits = url.split_once("/pull/").map(|(_, rest)| {
        rest.chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>()
    });
    match digits {
        Some(digits) if !digits.is_empty() => format!("#{digits}"),
        _ => "Pull request".into(),
    }
}

/// The colour of the checks text (`CHECKS_COLOR`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChecksTone {
    Passing,
    Failing,
    Pending,
    None,
}

/// "open, checks fail", or None when GitHub was not asked yet.
pub fn pull_request_summary(task: &Task) -> Option<String> {
    let status = task.pull_request_status()?;
    let checks = match status.checks.as_str() {
        "passing" => Some("checks pass"),
        "failing" => Some("checks fail"),
        "pending" => Some("checks running"),
        _ => None,
    };
    Some(match checks {
        Some(checks) => format!("{}, {checks}", status.state),
        None => status.state,
    })
}

/// The text after the pull request number in the PR cell, and its tone: the state alone when
/// there are no checks. None when GitHub was not asked yet.
pub fn pull_request_cell(task: &Task) -> Option<(String, ChecksTone)> {
    let status = task.pull_request_status()?;
    let tone = match status.checks.as_str() {
        "passing" => ChecksTone::Passing,
        "failing" => ChecksTone::Failing,
        "pending" => ChecksTone::Pending,
        _ => ChecksTone::None,
    };
    let text = match tone {
        ChecksTone::None => status.state,
        _ => pull_request_summary(task)?,
    };
    Some((text, tone))
}

/// 950, 12.3K, 610K, 1.2M.
pub fn format_tokens(count: u64) -> String {
    const UNITS: [&str; 4] = ["K", "M", "B", "T"];
    if count < 1000 {
        return count.to_string();
    }
    let mut value = count as f64;
    let mut unit = 0;
    value /= 1000.0;
    while unit + 1 < UNITS.len() && (value * 10.0).round() / 10.0 >= 1000.0 {
        value /= 1000.0;
        unit += 1;
    }
    let text = format!("{:.1}", (value * 10.0).round() / 10.0);
    let text = text.strip_suffix(".0").unwrap_or(&text);
    format!("{text}{}", UNITS[unit])
}

/// $0.84, <$0.01, or an em dash when there is no price.
pub fn format_usd(cost: Option<f64>) -> String {
    let Some(cost) = cost else {
        return "\u{2014}".into();
    };
    if cost > 0.0 && cost < 0.005 {
        return "<$0.01".into();
    }
    let text = format!("{:.2}", cost.abs());
    let (whole, cents) = text.split_once('.').unwrap_or((&text, "00"));
    let mut grouped = String::new();
    for (index, digit) in whole.chars().enumerate() {
        if index > 0 && (whole.len() - index) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    let sign = if cost < 0.0 { "-" } else { "" };
    format!("{sign}${grouped}.{cents}")
}

/// "+12 −3" and its tooltip "2 files". None when the task changed nothing.
pub fn diff_size(stat: Option<&DiffStat>) -> Option<(String, String, String)> {
    let stat = stat.filter(|stat| stat.files != 0)?;
    let files = if stat.files == 1 {
        "1 file".to_string()
    } else {
        format!("{} files", stat.files)
    };
    Some((
        format!("+{}", stat.added),
        format!("\u{2212}{}", stat.removed),
        files,
    ))
}

/// One item of a row menu, in the order shown. The view gives each its icon and handler.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowItem {
    Open,
    /// Opens the details panel on the task.
    Details,
    /// Only a task with no parent.
    AddSubtask,
    Separator,
    /// A state action that does not ask first, such as Mark ready.
    Action(TaskAction),
    /// Start or restart the agent; off while the task waits for others.
    Agent {
        restart: bool,
        waits: Vec<String>,
    },
    CopyId,
    /// Off when the task has no branch.
    CopyBranch {
        enabled: bool,
    },
    /// The state action that asks first: Cancel task.
    Cancel(TaskAction),
    Delete,
}

/// Where the row menu offers to start an agent. Elsewhere the agent works, or the task waits for
/// a person or is finished.
fn starts_agent(state: TaskState) -> bool {
    matches!(
        state,
        TaskState::Draft | TaskState::Ready | TaskState::Failed
    )
}

/// The row menu of the table (`menuFor` in `TaskTable.tsx`).
pub fn row_menu(task: &Task, tasks: &[Task]) -> Vec<RowItem> {
    let mut items = vec![RowItem::Open, RowItem::Details];
    if task.parent_id.is_none() {
        items.push(RowItem::AddSubtask);
    }
    items.push(RowItem::Separator);
    let actions = actions_for(task.state);
    let mut middle: Vec<RowItem> = actions
        .iter()
        .filter(|action| !action.confirm)
        .cloned()
        .map(RowItem::Action)
        .collect();
    if let Some(agent) = agent_action(task.state, task.agent.as_deref())
        && starts_agent(task.state)
    {
        middle.push(RowItem::Agent {
            restart: agent == AgentAction::Restart,
            waits: waits_for(task, tasks),
        });
    }
    if !middle.is_empty() {
        items.extend(middle);
        items.push(RowItem::Separator);
    }
    items.push(RowItem::CopyId);
    items.push(RowItem::CopyBranch {
        enabled: task.branch.is_some(),
    });
    items.push(RowItem::Separator);
    if let Some(cancel) = actions.into_iter().find(|action| action.confirm) {
        items.push(RowItem::Cancel(cancel));
    }
    items.push(RowItem::Delete);
    items
}

/// A key that moves the selection of a list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Next,
    Previous,
    First,
    Last,
}

/// The row a key moves to in a list of `count` rows, stopping at the ends. With no row selected,
/// Next and First go to the first row, Previous and Last to the last. None when the list is empty.
pub fn step(count: usize, current: Option<usize>, step: Step) -> Option<usize> {
    let last = count.checked_sub(1)?;
    Some(match (step, current) {
        (Step::First, _) | (Step::Next, None) => 0,
        (Step::Last, _) | (Step::Previous, None) => last,
        (Step::Next, Some(index)) => (index + 1).min(last),
        (Step::Previous, Some(index)) => index.saturating_sub(1).min(last),
    })
}

/// How long typed letters add up to one word, in ms.
pub const TYPE_AHEAD_MS: i64 = 600;

/// The next label that starts with what was typed. A single letter pressed again goes to the next
/// label with that letter. None when nothing matches (`typeAheadTarget`).
pub fn type_ahead_target(labels: &[&str], typed: &str, from: usize) -> Option<usize> {
    let text = typed.to_lowercase();
    if text.is_empty() || labels.is_empty() {
        return None;
    }
    let count = labels.len();
    let start = if text.chars().count() == 1 {
        from + 1
    } else {
        from
    };
    (0..count)
        .map(|offset| (start + offset) % count)
        .find(|index| labels[*index].to_lowercase().starts_with(&text))
}

#[cfg(test)]
mod tests {

    use super::*;
    use natamux_proto::tasks::task::PullRequestStatus;

    fn make(id: &str, state: TaskState) -> Task {
        Task {
            id: id.into(),
            title: format!("Task {id}"),
            state,
            stage: "implement".into(),
            created_at: "2026-10-08T09:00:00.000Z".into(),
            updated_at: "2026-10-08T09:00:00.000Z".into(),
            ..Task::default()
        }
    }

    #[test]
    fn waits_for_dependencies_that_are_not_done_and_skips_missing_ones() {
        let mut task = make("T-3", TaskState::Running);
        task.depends_on = vec!["T-1".into(), "T-2".into(), "T-9".into()];
        let tasks = vec![
            make("T-1", TaskState::Done),
            make("T-2", TaskState::Review),
            task.clone(),
        ];
        assert_eq!(waits_for(&task, &tasks), vec!["T-2"]);
    }

    #[test]
    fn waits_for_open_subtasks_only_before_a_stage_started() {
        let parent = make("T-1", TaskState::Ready);
        let mut open = make("T-2", TaskState::Running);
        open.parent_id = Some("T-1".into());
        let mut done = make("T-3", TaskState::Cancelled);
        done.parent_id = Some("T-1".into());
        let tasks = vec![parent.clone(), open, done];
        assert_eq!(waits_for(&parent, &tasks), vec!["T-2"]);
        let running = make("T-1", TaskState::Running);
        assert!(waits_for(&running, &tasks).is_empty());
    }

    #[test]
    fn turns_a_pull_request_url_into_its_number() {
        assert_eq!(pull_request_number("https://github.com/o/r/pull/12"), "#12");
        assert_eq!(
            pull_request_number("https://example.com/merge/3"),
            "Pull request"
        );
    }

    #[test]
    fn says_the_pull_request_state_and_its_checks() {
        let mut task = make("T-1", TaskState::Review);
        assert_eq!(pull_request_summary(&task), None);
        assert_eq!(pull_request_cell(&task), None);
        task.pr_status = Some(PullRequestStatus {
            state: "open".into(),
            checks: "failing".into(),
        });
        assert_eq!(
            pull_request_summary(&task).as_deref(),
            Some("open, checks fail")
        );
        assert_eq!(
            pull_request_cell(&task),
            Some(("open, checks fail".into(), ChecksTone::Failing))
        );
        task.pr_status = Some(PullRequestStatus {
            state: "merged".into(),
            checks: "none".into(),
        });
        assert_eq!(pull_request_summary(&task).as_deref(), Some("merged"));
        assert_eq!(
            pull_request_cell(&task),
            Some(("merged".into(), ChecksTone::None))
        );
    }

    #[test]
    fn writes_token_counts_short() {
        assert_eq!(format_tokens(950), "950");
        assert_eq!(format_tokens(1000), "1K");
        assert_eq!(format_tokens(12_300), "12.3K");
        assert_eq!(format_tokens(610_000), "610K");
        assert_eq!(format_tokens(1_200_000), "1.2M");
        assert_eq!(format_tokens(999_960), "1M");
    }

    #[test]
    fn writes_dollars_with_cents_and_a_floor() {
        assert_eq!(format_usd(None), "\u{2014}");
        assert_eq!(format_usd(Some(0.84)), "$0.84");
        assert_eq!(format_usd(Some(0.004)), "<$0.01");
        assert_eq!(format_usd(Some(0.0)), "$0.00");
        assert_eq!(format_usd(Some(1234.5)), "$1,234.50");
    }

    #[test]
    fn shows_no_diff_size_for_a_task_that_changed_nothing() {
        assert_eq!(diff_size(None), None);
        let none = DiffStat::default();
        assert_eq!(diff_size(Some(&none)), None);
        let one = DiffStat {
            files: 1,
            added: 12,
            removed: 3,
        };
        assert_eq!(
            diff_size(Some(&one)),
            Some(("+12".into(), "\u{2212}3".into(), "1 file".into()))
        );
    }

    fn labels(items: &[RowItem]) -> Vec<String> {
        items
            .iter()
            .map(|item| match item {
                RowItem::Action(action) | RowItem::Cancel(action) => action.label.to_string(),
                other => format!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn row_menu_of_a_draft_offers_the_state_action_the_agent_and_cancel() {
        let task = make("T-1", TaskState::Draft);
        assert_eq!(
            labels(&row_menu(&task, std::slice::from_ref(&task))),
            vec![
                "Open",
                "Details",
                "AddSubtask",
                "Separator",
                "Mark ready",
                "Agent { restart: false, waits: [] }",
                "Separator",
                "CopyId",
                "CopyBranch { enabled: false }",
                "Separator",
                "Cancel task",
                "Delete",
            ]
        );
    }

    #[test]
    fn row_menu_of_a_finished_subtask_has_no_actions() {
        let mut task = make("T-2", TaskState::Done);
        task.parent_id = Some("T-1".into());
        task.branch = Some("natamux/t-2".into());
        assert_eq!(
            labels(&row_menu(&task, &[])),
            vec![
                "Open",
                "Details",
                "Separator",
                "CopyId",
                "CopyBranch { enabled: true }",
                "Separator",
                "Delete",
            ]
        );
    }

    #[test]
    fn row_menu_restarts_a_failed_agent_and_says_what_it_waits_for() {
        let mut task = make("T-3", TaskState::Failed);
        task.agent = Some("claude".into());
        task.depends_on = vec!["T-1".into()];
        let tasks = vec![make("T-1", TaskState::Running), task.clone()];
        let menu = row_menu(&task, &tasks);
        assert!(menu.contains(&RowItem::Agent {
            restart: true,
            waits: vec!["T-1".into()],
        }));
        // A running task's agent works, so the menu offers no start.
        let running = make("T-4", TaskState::Running);
        assert!(
            !row_menu(&running, &[])
                .iter()
                .any(|item| matches!(item, RowItem::Agent { .. }))
        );
    }

    #[test]
    fn steps_through_a_list_and_stops_at_the_ends() {
        assert_eq!(step(0, None, Step::Next), None);
        assert_eq!(step(3, None, Step::Next), Some(0));
        assert_eq!(step(3, None, Step::Previous), Some(2));
        assert_eq!(step(3, Some(2), Step::Next), Some(2));
        assert_eq!(step(3, Some(0), Step::Previous), Some(0));
        assert_eq!(step(3, Some(1), Step::First), Some(0));
        assert_eq!(step(3, Some(1), Step::Last), Some(2));
        // A row past the end, after the list shrank, comes back inside.
        assert_eq!(step(3, Some(7), Step::Previous), Some(2));
    }

    const LABELS: [&str; 4] = ["Fix login", "Add export", "Add import", "Zoom"];

    #[test]
    fn finds_the_first_label_that_starts_with_the_text_after_the_current_one() {
        assert_eq!(type_ahead_target(&LABELS, "a", 0), Some(1));
    }

    #[test]
    fn goes_to_the_next_match_when_the_same_letter_is_pressed_again() {
        assert_eq!(type_ahead_target(&LABELS, "a", 1), Some(2));
        assert_eq!(type_ahead_target(&LABELS, "a", 2), Some(1));
    }

    #[test]
    fn keeps_the_current_item_while_a_longer_word_still_matches_it() {
        assert_eq!(type_ahead_target(&LABELS, "add i", 1), Some(2));
        assert_eq!(type_ahead_target(&LABELS, "add e", 1), Some(1));
    }

    #[test]
    fn returns_1_when_nothing_matches() {
        assert_eq!(type_ahead_target(&LABELS, "q", 0), None);
        assert_eq!(type_ahead_target(&[], "a", 0), None);
    }
}
