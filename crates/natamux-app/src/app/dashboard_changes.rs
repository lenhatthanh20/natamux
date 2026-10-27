//! Whether an event of the feed changes the data: an equal part draws nothing.

use natamux_client::DashboardEvent;

use super::dashboard_data::{DashboardData, Link};

impl DashboardData {
    /// Whether `apply` of this event changes what the views show. The feed sends a part again
    /// after a reconnect, a poll or a log line; an equal part draws nothing.
    pub fn changes(&self, event: &DashboardEvent) -> bool {
        match event {
            DashboardEvent::Tasks(tasks) => {
                !self.loaded || self.link != Link::Live || self.tasks != *tasks
            }
            DashboardEvent::Repos {
                repos,
                worktrees_dir,
            } => self.repos != *repos || self.worktrees_dir != *worktrees_dir,
            DashboardEvent::Jobs(jobs) => self.jobs != *jobs,
            DashboardEvent::Activity(activity) => self.activity != *activity,
            DashboardEvent::Usage(usage) => self.usage != *usage,
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
