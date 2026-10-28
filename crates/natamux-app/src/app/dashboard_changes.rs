//! Whether an event of the feed changes the data: an equal part draws nothing.

use natamux_client::DashboardEvent;
use natamux_proto::tasks::task::Task;

use super::dashboard_data::{DashboardData, Link, Part};

impl DashboardData {
    /// Whether `apply` of this event changes what the views show. The feed sends a part again
    /// after a reconnect, a poll or a log line; an equal part draws nothing.
    pub fn changes(&self, event: &DashboardEvent) -> bool {
        match event {
            DashboardEvent::Tasks(tasks) => {
                !self.loaded || self.link != Link::Live || self.tasks != *tasks
            }
            DashboardEvent::TaskChanged(task) => self.task_changes(task),
            DashboardEvent::Repos {
                repos,
                worktrees_dir,
            } => self.repos != *repos || self.worktrees_dir != *worktrees_dir,
            DashboardEvent::Jobs(jobs) => self.jobs != *jobs,
            DashboardEvent::Activity(activity) => self.activity != *activity,
            DashboardEvent::Usage(usage) => self.usage != *usage,
            DashboardEvent::Limits(limits) => self.limits != *limits,
            DashboardEvent::DiffStats(stats) => self.diff_stats != *stats,
            DashboardEvent::Terminals { task_id, terminals } => {
                self.terminals.get(task_id) != Some(terminals)
            }
            DashboardEvent::Preparing(preparing) => self.preparing != *preparing,
            DashboardEvent::WorkspaceStep(_) | DashboardEvent::Status(_) => true,
            DashboardEvent::WorkspaceChanged { .. }
            | DashboardEvent::ShellsChanged { .. }
            | DashboardEvent::ReviewCommentsChanged { .. }
            | DashboardEvent::TaskLogged { .. } => false,
        }
    }

    /// Bumps the parts `apply` of this event changes; called only when `changes` says it does.
    /// One task bumps Tasks in `merge_task`.
    pub(super) fn bump_parts(&mut self, event: &DashboardEvent) {
        let parts: &[Part] = match event {
            // A task that is gone takes its terminals and attention card with it.
            DashboardEvent::Tasks(_) => {
                if self.link != Link::Live {
                    self.gens.bump(Part::Link);
                }
                &[Part::Tasks, Part::Terminals, Part::Events]
            }
            DashboardEvent::Repos { .. } => &[Part::Repos],
            DashboardEvent::Jobs(_) => &[Part::Jobs],
            DashboardEvent::Activity(_) => &[Part::Activity],
            DashboardEvent::Usage(_) => &[Part::Usage],
            DashboardEvent::Limits(_) => &[Part::Limits],
            DashboardEvent::DiffStats(_) => &[Part::DiffStats],
            DashboardEvent::Terminals { .. } => &[Part::Terminals],
            DashboardEvent::Preparing(_) | DashboardEvent::WorkspaceStep(_) => &[Part::Preparing],
            DashboardEvent::Status(_) => &[Part::Link],
            DashboardEvent::TaskChanged(_)
            | DashboardEvent::WorkspaceChanged { .. }
            | DashboardEvent::ShellsChanged { .. }
            | DashboardEvent::ReviewCommentsChanged { .. }
            | DashboardEvent::TaskLogged { .. } => &[],
        };
        for &part in parts {
            self.gens.bump(part);
        }
    }

    /// `merge_task` keeps a held task that is newer. Unlike a list, one task changes neither
    /// `loaded` nor the link, so those do not count.
    pub(super) fn task_changes(&self, task: &Task) -> bool {
        match self.task(&task.id) {
            Some(held) => held != task && held.updated_at <= task.updated_at,
            None => true,
        }
    }

    /// The events after which `Dashboard::after_tasks` runs: the whole list, or one task.
    pub fn brings_tasks(event: &DashboardEvent) -> bool {
        matches!(
            event,
            DashboardEvent::Tasks(_) | DashboardEvent::TaskChanged(_)
        )
    }
}

#[cfg(test)]
mod tests {
    use natamux_client::LinkStatus;
    use natamux_proto::events::kind::PreparingMessage;
    use natamux_proto::tasks::task::{Task, TaskState};
    use natamux_proto::terminals::list::TerminalList;

    use super::*;

    fn task(id: &str, state: TaskState, updated_at: &str) -> Task {
        Task {
            id: id.into(),
            state,
            updated_at: updated_at.into(),
            ..Task::default()
        }
    }

    fn step(id: &str) -> DashboardEvent {
        DashboardEvent::WorkspaceStep(PreparingMessage {
            task_id: id.into(),
            step: None,
            error: None,
        })
    }

    #[test]
    fn a_part_equal_to_the_one_held_changes_nothing() {
        let mut data = DashboardData::default();
        let tasks = DashboardEvent::Tasks(vec![task("T-1", TaskState::Running, "a")]);
        assert!(data.changes(&tasks));
        data.apply(tasks.clone());
        assert!(!data.changes(&tasks));
        let moved = DashboardEvent::Tasks(vec![task("T-1", TaskState::Review, "b")]);
        assert!(data.changes(&moved));

        let jobs = DashboardEvent::Jobs(Vec::new());
        assert!(!data.changes(&jobs));
        let terminals = DashboardEvent::Terminals {
            task_id: "T-1".into(),
            terminals: TerminalList::default(),
        };
        assert!(data.changes(&terminals));
        data.apply(terminals.clone());
        assert!(!data.changes(&terminals));
    }

    #[test]
    fn one_task_changes_the_data_when_it_differs_and_is_not_older() {
        let mut data = DashboardData::default();
        let one = |state, updated_at| {
            DashboardEvent::TaskChanged(Box::new(task("T-1", state, updated_at)))
        };
        assert!(data.changes(&one(TaskState::Running, "b")));
        let list = DashboardEvent::Tasks(vec![task("T-1", TaskState::Running, "b")]);
        data.apply(list);
        assert!(!data.changes(&one(TaskState::Running, "b")));
        // The same time counts as newer, as in `merge_task`.
        assert!(data.changes(&one(TaskState::Review, "b")));
        assert!(data.changes(&one(TaskState::Review, "c")));
        assert!(!data.changes(&one(TaskState::Review, "a")));
        let other = DashboardEvent::TaskChanged(Box::new(task("T-2", TaskState::Draft, "a")));
        assert!(data.changes(&other));
        assert!(DashboardData::brings_tasks(&other));
    }

    #[test]
    fn one_task_of_the_feed_takes_its_place_in_the_list_order() {
        let mut data = DashboardData::default();
        let one = |task: Task| DashboardEvent::TaskChanged(Box::new(task));
        data.apply(one(task("T-9", TaskState::Running, "b")));
        assert!(!data.loaded);
        assert_eq!(data.link, Link::Loading);
        data.apply(DashboardEvent::Tasks(vec![
            task("T-2", TaskState::Draft, "c"),
            task("T-1", TaskState::Running, "a"),
            task("T-3", TaskState::Ready, "a"),
        ]));
        data.apply(one(task("T-1", TaskState::Review, "d")));
        data.apply(one(task("T-0", TaskState::Draft, "a")));
        // Older than the one held: a list read since is newer.
        data.apply(one(task("T-2", TaskState::Running, "b")));
        let ids: Vec<&str> = data.tasks.iter().map(|task| task.id.as_str()).collect();
        assert_eq!(ids, ["T-1", "T-2", "T-0", "T-3"]);
        assert_eq!(
            data.task("T-2").map(|task| task.state),
            Some(TaskState::Draft)
        );
    }

    #[test]
    fn one_task_that_needs_input_reads_its_events_again() {
        let mut data = DashboardData::default();
        data.apply(DashboardEvent::Tasks(vec![task(
            "T-1",
            TaskState::Running,
            "a",
        )]));
        data.set_events("T-1", "a", Vec::new());
        let asks = task("T-1", TaskState::NeedsInput, "b");
        data.apply(DashboardEvent::TaskChanged(Box::new(asks)));
        assert_eq!(data.events_to_read(), [("T-1".into(), "b".into())]);
    }

    #[test]
    fn the_workspace_events_change_no_data_and_a_step_or_a_status_always_does() {
        let data = DashboardData::default();
        let task_id = "T-1".to_string();
        let quiet = [
            DashboardEvent::WorkspaceChanged {
                task_id: task_id.clone(),
            },
            DashboardEvent::ShellsChanged {
                task_id: task_id.clone(),
            },
            DashboardEvent::ReviewCommentsChanged {
                task_id: task_id.clone(),
            },
            DashboardEvent::TaskLogged { task_id },
        ];
        assert!(quiet.iter().all(|event| !data.changes(event)));
        assert!(data.changes(&step("T-1")));
        let live = DashboardEvent::Status(LinkStatus::Live);
        assert!(data.changes(&live));
    }
}
