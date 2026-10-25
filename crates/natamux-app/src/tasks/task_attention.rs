//! Which tasks need the user, how many tasks each state has, and the search of the list (`isQuiet`,
//! `needsMeTasks`, `countByState`, `filterTasks` in `packages/web/src/lib/tasks.ts`).

use natamux_proto::tasks::task::{Task, TaskState};

use super::task_rules::needs_me;
use super::task_time::{next_age_change_ms, parse_iso_ms};

/// Ten minutes: a long command can keep an agent silent for a while without a problem.
pub const QUIET_AFTER_MS: i64 = 10 * 60 * 1000;

/// A running task with no sign of life for `QUIET_AFTER_MS` is probably stuck or closed. An agent
/// that waits for background work, such as a CI watch, is woken by it, so it is never quiet.
pub fn is_quiet(task: &Task, now_ms: i64) -> bool {
    task.state == TaskState::Running
        && task.background_since.is_none()
        && parse_iso_ms(&task.updated_at).is_some_and(|at| now_ms - at >= QUIET_AFTER_MS)
}

/// How long until the next running task turns quiet: then it needs the user by the clock alone,
/// with no change of the data. None when no task can turn quiet.
pub fn next_quiet_ms<'a>(tasks: impl IntoIterator<Item = &'a Task>, now_ms: i64) -> Option<i64> {
    tasks
        .into_iter()
        .filter_map(|task| quiet_wait(task, now_ms))
        .min()
}

fn quiet_wait(task: &Task, now_ms: i64) -> Option<i64> {
    if task.state != TaskState::Running || task.background_since.is_some() {
        return None;
    }
    let wait = parse_iso_ms(&task.updated_at)? + QUIET_AFTER_MS - now_ms;
    (wait > 0).then_some(wait)
}

/// How long until a row of these tasks shows another label by the clock alone: the age of its
/// last change, or the quiet mark. None when nothing will change.
pub fn next_row_change_ms<'a>(
    tasks: impl IntoIterator<Item = &'a Task>,
    now_ms: i64,
) -> Option<i64> {
    tasks
        .into_iter()
        .flat_map(|task| {
            [
                next_age_change_ms(&task.updated_at, now_ms),
                quiet_wait(task, now_ms),
            ]
        })
        .flatten()
        .min()
}

/// How many tasks `needs_me_tasks` gives, with no copy of them.
pub fn needs_me_count(tasks: &[Task], now_ms: i64) -> usize {
    tasks
        .iter()
        .filter(|task| needs_me(task.state) || is_quiet(task, now_ms))
        .count()
}

/// Tasks that need the user: waiting on them, or running but quiet. The longest wait comes first.
pub fn needs_me_tasks(tasks: &[Task], now_ms: i64) -> Vec<Task> {
    let mut list: Vec<Task> = tasks
        .iter()
        .filter(|task| needs_me(task.state) || is_quiet(task, now_ms))
        .cloned()
        .collect();
    list.sort_by(|a, b| {
        a.updated_at
            .cmp(&b.updated_at)
            .then_with(|| a.id.cmp(&b.id))
    });
    list
}

/// Every state with its count, zero for the empty ones, in the order of `TASK_STATES`.
pub fn count_by_state(tasks: &[Task]) -> Vec<(TaskState, usize)> {
    let mut counts = [
        TaskState::Draft,
        TaskState::Ready,
        TaskState::Running,
        TaskState::NeedsInput,
        TaskState::Review,
        TaskState::Blocked,
        TaskState::Failed,
        TaskState::Done,
        TaskState::Cancelled,
    ]
    .map(|state| (state, 0));
    for task in tasks {
        if let Some(entry) = counts.iter_mut().find(|(state, _)| *state == task.state) {
            entry.1 += 1;
        }
    }
    counts.to_vec()
}

/// The tasks in this state (all when None) whose id, title, agent, branch or labels have the
/// search text in them, ignoring case.
pub fn filter_tasks(tasks: &[Task], state_filter: Option<TaskState>, search: &str) -> Vec<Task> {
    let needle = search.trim().to_lowercase();
    tasks
        .iter()
        .filter(|task| {
            if state_filter.is_some_and(|state| task.state != state) {
                return false;
            }
            if needle.is_empty() {
                return true;
            }
            let mut parts = vec![task.id.as_str(), task.title.as_str()];
            parts.extend(task.agent.as_deref());
            parts.extend(task.branch.as_deref());
            parts.extend(task.labels.iter().map(String::as_str));
            parts.join(" ").to_lowercase().contains(&needle)
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(overrides: impl FnOnce(&mut Task)) -> Task {
        let mut task = Task {
            id: "T-0001".into(),
            title: "Example".into(),
            state: TaskState::Draft,
            stage: "implement".into(),
            created_at: "2026-10-08T09:00:00.000Z".into(),
            updated_at: "2026-10-08T09:00:00.000Z".into(),
            ..Task::default()
        };
        overrides(&mut task);
        task
    }

    fn at_ms(time: &str) -> i64 {
        parse_iso_ms(time).unwrap()
    }

    fn ids(tasks: &[Task]) -> Vec<&str> {
        tasks.iter().map(|task| task.id.as_str()).collect()
    }

    fn with(id: &str, state: TaskState, updated_at: &str) -> Task {
        make(|task| {
            task.id = id.into();
            task.state = state;
            task.updated_at = updated_at.into();
        })
    }

    const ALL_STATES: [TaskState; 9] = [
        TaskState::Draft,
        TaskState::Ready,
        TaskState::Running,
        TaskState::NeedsInput,
        TaskState::Review,
        TaskState::Blocked,
        TaskState::Failed,
        TaskState::Done,
        TaskState::Cancelled,
    ];

    fn quiet_task() -> Task {
        with("T-0001", TaskState::Running, "2026-10-08T09:00:00.000Z")
    }

    #[test]
    fn counts_the_tasks_that_need_the_user_as_the_list_does() {
        let now = at_ms("2026-10-08T09:30:00.000Z");
        let tasks: Vec<Task> = ALL_STATES
            .iter()
            .enumerate()
            .map(|(at, state)| with(&format!("T-{at}"), *state, "2026-10-08T09:00:00.000Z"))
            .chain([with("T-9", TaskState::Running, "2026-10-08T09:29:00.000Z")])
            .collect();
        assert_eq!(
            needs_me_count(&tasks, now),
            needs_me_tasks(&tasks, now).len()
        );
        assert_eq!(needs_me_count(&[], now), 0);
    }

    #[test]
    fn is_false_while_the_last_sign_of_life_is_recent() {
        assert!(!is_quiet(&quiet_task(), at_ms("2026-10-08T09:09:59.000Z")));
    }

    #[test]
    fn is_true_once_the_quiet_time_has_passed() {
        assert!(is_quiet(&quiet_task(), at_ms("2026-10-08T09:10:00.000Z")));
        assert!(is_quiet(&quiet_task(), at_ms("2026-10-08T11:00:00.000Z")));
    }

    #[test]
    fn is_never_true_for_a_task_that_is_not_running() {
        let states = [
            TaskState::Draft,
            TaskState::Ready,
            TaskState::NeedsInput,
            TaskState::Review,
            TaskState::Blocked,
            TaskState::Failed,
            TaskState::Done,
        ];
        for state in states {
            let task = Task {
                state,
                ..quiet_task()
            };
            assert!(
                !is_quiet(&task, at_ms("2026-10-09T09:00:00.000Z")),
                "{state:?}"
            );
        }
    }

    #[test]
    fn is_never_true_while_the_agent_waits_for_its_background_work() {
        let times = [
            "2026-10-08T09:01:00.000Z",
            "2026-10-08T09:10:00.000Z",
            "2026-10-08T09:11:00.000Z",
            "2026-10-08T15:00:00.000Z",
        ];
        for now in times {
            let task = quiet_task();
            let waiting = Task {
                background_since: Some(task.updated_at.clone()),
                ..task
            };
            assert!(!is_quiet(&waiting, at_ms(now)), "{now}");
        }
    }

    fn now_five_past() -> i64 {
        at_ms("2026-10-08T09:05:00.000Z")
    }

    #[test]
    fn keeps_only_tasks_that_need_the_user_longest_wait_first() {
        let tasks = [
            with("T-0001", TaskState::Running, "2026-10-08T09:00:00.000Z"),
            with("T-0002", TaskState::Review, "2026-10-08T10:00:00.000Z"),
            with("T-0003", TaskState::Failed, "2026-10-08T08:00:00.000Z"),
            with("T-0004", TaskState::Done, "2026-10-08T09:00:00.000Z"),
        ];
        assert_eq!(
            ids(&needs_me_tasks(&tasks, now_five_past())),
            ["T-0003", "T-0002"]
        );
    }

    #[test]
    fn adds_a_running_task_that_has_been_quiet_for_a_long_time() {
        let tasks = [
            with("T-0001", TaskState::Running, "2026-10-08T08:30:00.000Z"),
            with("T-0002", TaskState::Running, "2026-10-08T09:04:00.000Z"),
            with("T-0003", TaskState::Review, "2026-10-08T09:00:00.000Z"),
        ];
        assert_eq!(
            ids(&needs_me_tasks(&tasks, now_five_past())),
            ["T-0001", "T-0003"]
        );
    }

    #[test]
    fn leaves_out_a_running_task_that_waits_for_background_work_however_long_it_waits() {
        let task = Task {
            background_since: Some("2026-10-08T07:00:00.000Z".into()),
            ..with("T-0001", TaskState::Running, "2026-10-08T07:00:00.000Z")
        };
        assert!(needs_me_tasks(&[task], now_five_past()).is_empty());
    }

    #[test]
    fn leaves_the_task_out_after_a_workspace_is_removed() {
        let removed = with("T-0001", TaskState::Ready, "2026-10-08T09:00:00.000Z");
        let times = [
            "2026-10-08T09:10:00.000Z",
            "2026-10-08T09:10:00.001Z",
            "2026-10-08T09:15:00.000Z",
            "2026-10-09T09:00:00.000Z",
        ];
        for time in times {
            assert!(
                needs_me_tasks(std::slice::from_ref(&removed), at_ms(time)).is_empty(),
                "{time}"
            );
        }
    }

    #[test]
    fn still_lists_another_workspace_that_is_quiet() {
        let removed = with("T-0001", TaskState::Ready, "2026-10-08T09:00:00.000Z");
        let quiet = Task {
            workspace: Some("/work/T-0002".into()),
            ..with("T-0002", TaskState::Running, "2026-10-08T08:00:00.000Z")
        };
        let listed = needs_me_tasks(&[removed, quiet], at_ms("2026-10-08T09:15:00.000Z"));
        assert_eq!(ids(&listed), ["T-0002"]);
    }

    #[test]
    fn lists_a_task_with_no_workspace_that_waits_for_the_user() {
        for state in [TaskState::NeedsInput, TaskState::Failed, TaskState::Blocked] {
            let task = make(|t| t.state = state);
            assert_eq!(
                needs_me_tasks(&[task], now_five_past()).len(),
                1,
                "{state:?}"
            );
        }
    }

    #[test]
    fn lists_a_running_task_with_no_workspace_that_has_been_quiet_for_a_long_time() {
        let task = with("T-0001", TaskState::Running, "2026-10-08T08:00:00.000Z");
        assert_eq!(needs_me_tasks(&[task], now_five_past()).len(), 1);
    }

    #[test]
    fn does_not_change_the_list_it_is_given() {
        let tasks = [
            with("T-0002", TaskState::Review, "2026-10-08T09:00:00.000Z"),
            with("T-0001", TaskState::Draft, "2026-10-08T09:00:00.000Z"),
        ];
        needs_me_tasks(&tasks, now_five_past());
        assert_eq!(ids(&tasks), ["T-0002", "T-0001"]);
    }

    #[test]
    fn counts_every_state_with_zero_for_the_empty_ones() {
        let tasks = [
            with("T-0001", TaskState::Running, ""),
            with("T-0002", TaskState::Running, ""),
            with("T-0003", TaskState::Review, ""),
        ];
        let counts = count_by_state(&tasks);
        let count = |state| counts.iter().find(|(s, _)| *s == state).map(|(_, n)| *n);
        assert_eq!(count(TaskState::Running), Some(2));
        assert_eq!(count(TaskState::Review), Some(1));
        assert_eq!(count(TaskState::Done), Some(0));
        let order: Vec<TaskState> = counts.iter().map(|(state, _)| *state).collect();
        assert_eq!(order, ALL_STATES);
    }

    fn filter_ids(state: Option<TaskState>, search: &str) -> Vec<String> {
        let tasks = [
            make(|t| {
                t.title = "Fix timezone".into();
                t.state = TaskState::Running;
                t.agent = Some("claude-code".into());
            }),
            make(|t| {
                t.id = "T-0002".into();
                t.title = "Add export".into();
                t.state = TaskState::Review;
                t.branch = Some("natamux/export".into());
            }),
            make(|t| {
                t.id = "T-0003".into();
                t.title = "Cleanup".into();
                t.state = TaskState::Review;
                t.labels = vec!["chore".into()];
            }),
        ];
        filter_tasks(&tasks, state, search)
            .into_iter()
            .map(|t| t.id)
            .collect()
    }

    #[test]
    fn returns_everything_without_a_filter() {
        assert_eq!(filter_ids(None, ""), ["T-0001", "T-0002", "T-0003"]);
    }

    #[test]
    fn filters_by_state() {
        assert_eq!(
            filter_ids(Some(TaskState::Review), ""),
            ["T-0002", "T-0003"]
        );
    }

    #[test]
    fn searches_in_title_id_agent_branch_and_labels() {
        let cases: [(&str, &[&str]); 6] = [
            ("timezone", &["T-0001"]),
            ("T-0003", &["T-0003"]),
            ("CLAUDE", &["T-0001"]),
            ("natamux/export", &["T-0002"]),
            ("chore", &["T-0003"]),
            ("nothing like this", &[]),
        ];
        for (search, expected) in cases {
            assert_eq!(filter_ids(None, search), expected, "{search}");
        }
    }

    #[test]
    fn combines_the_state_and_the_search() {
        assert_eq!(filter_ids(Some(TaskState::Review), "export"), ["T-0002"]);
    }

    #[test]
    fn the_next_quiet_time_is_the_soonest_running_task_to_go_quiet() {
        let now = at_ms("2026-10-08T09:05:00.000Z");
        let tasks = [
            with("T-1", TaskState::Running, "2026-10-08T09:00:00.000Z"),
            with("T-2", TaskState::Running, "2026-10-08T09:03:00.000Z"),
            with("T-3", TaskState::Review, "2026-10-08T08:59:00.000Z"),
        ];
        assert_eq!(next_quiet_ms(&tasks, now), Some(5 * 60 * 1000));
        // At that time T-1 has turned quiet and counts, with the task in review; T-2 is next.
        let later = now + 5 * 60 * 1000;
        assert_eq!(needs_me_count(&tasks, now), 1);
        assert_eq!(needs_me_count(&tasks, later), 2);
        assert_eq!(next_quiet_ms(&tasks, later), Some(3 * 60 * 1000));
    }

    #[test]
    fn no_quiet_time_for_a_quiet_waiting_or_stopped_task() {
        let now = at_ms("2026-10-08T09:30:00.000Z");
        let waiting = make(|task| {
            task.state = TaskState::Running;
            task.updated_at = "2026-10-08T09:25:00.000Z".into();
            task.background_since = Some(task.updated_at.clone());
        });
        let tasks = [
            quiet_task(),
            waiting,
            with("T-2", TaskState::Draft, "2026-10-08T09:29:00.000Z"),
            with("T-3", TaskState::Running, "soon"),
        ];
        assert_eq!(next_quiet_ms(&tasks, now), None);
        assert_eq!(next_quiet_ms(Vec::<Task>::new().iter(), now), None);
    }

    #[test]
    fn a_row_changes_at_the_next_minute_of_its_age_or_when_it_turns_quiet() {
        let now = at_ms("2026-10-08T09:09:40.000Z");
        // 9m 40s old: the age shows 10m in 20 s, as the task turns quiet.
        let running = with("T-1", TaskState::Running, "2026-10-08T09:00:00.000Z");
        assert_eq!(next_row_change_ms([&running], now), Some(20_000));
        // 30 s old: its age changes in 30 s, before it can turn quiet.
        let fresh = with("T-2", TaskState::Review, "2026-10-08T09:09:10.000Z");
        assert_eq!(next_row_change_ms([&running, &fresh], now), Some(20_000));
        assert_eq!(next_row_change_ms([&fresh], now), Some(30_000));
        // Past a week the age is a date, and a stopped task never turns quiet.
        let old = with("T-3", TaskState::Done, "2026-09-01T09:00:00.000Z");
        assert_eq!(next_row_change_ms([&old], now), None);
        assert_eq!(next_row_change_ms(Vec::<Task>::new().iter(), now), None);
    }
}
