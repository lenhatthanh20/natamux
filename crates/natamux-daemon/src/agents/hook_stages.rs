//! What the hook service needs to know about stages, apart from the service itself.

use natamux_proto::tasks::task::{Task, TaskState};

use crate::stages::runner::StageRunner;

/// The stage a terminal works on, and the state of its lane.
pub type Lane = (String, TaskState);

/// What the hook service needs to know about stages: which stage a terminal works on (None for
/// none), and whether a turn end in a terminal follows a report (see `StageRun`).
pub trait StageLookup: Send + Sync {
    fn lane_for(&self, task: &Task, terminal: Option<&str>) -> Result<Option<Lane>, String>;
    fn take_report(&self, task_id: &str, terminal: &str) -> bool;
    /// The stage reported from this terminal whose turn end has not come yet, without taking it.
    fn report_of(&self, _task_id: &str, _terminal: &str) -> Option<String> {
        None
    }
}

impl StageLookup for StageRunner {
    fn lane_for(&self, task: &Task, terminal: Option<&str>) -> Result<Option<Lane>, String> {
        self.run
            .lane_for(task, terminal)
            .map_err(|error| error.to_string())
    }

    fn take_report(&self, task_id: &str, terminal: &str) -> bool {
        self.run.take_report(task_id, terminal)
    }

    fn report_of(&self, task_id: &str, terminal: &str) -> Option<String> {
        self.run.report_of(task_id, terminal)
    }
}

/// No stages: the task's own stage and state, and no report.
pub struct NoStages;

impl StageLookup for NoStages {
    fn lane_for(&self, task: &Task, _terminal: Option<&str>) -> Result<Option<Lane>, String> {
        Ok(Some((task.stage.clone(), task.state)))
    }

    fn take_report(&self, _task_id: &str, _terminal: &str) -> bool {
        false
    }
}
