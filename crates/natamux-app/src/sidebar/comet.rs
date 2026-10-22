//! How far a task is through its own steps (`cometOf` in `sidebar-tree.ts`).

use natamux_proto::tasks::task::{Task, TaskState};

use crate::sidebar::sidebar_tree::{
    PathRef, StepRef, lane_state, stage_name, state_label, step_holds, steps_of, urgent_state,
};

#[derive(Clone, Debug, PartialEq)]
pub struct Comet {
    /// From 0 to 1: the steps done, and the current one.
    pub progress: f64,
    pub at: usize,
    pub of: usize,
    /// The state of the current stage, which colors the comet.
    pub state: TaskState,
    /// The current stage by name. None for a finished task.
    pub stage: Option<String>,
    /// Every step and its state, for the tooltip and screen readers.
    pub summary: String,
}

fn lower(state: TaskState) -> String {
    state_label(state).to_lowercase()
}

/// How far a task is through its own steps. A group is one step; its most urgent stage speaks
/// for it.
pub fn comet_of(task: &Task) -> Comet {
    let steps = steps_of(task);
    let finished = task.state.is_finished();
    let found = steps.iter().position(|step| step_holds(step, &task.stage));
    // An empty pipeline of a done task has no current step: the index is -1, as in the dashboard.
    let index: isize = if task.state == TaskState::Done {
        steps.len() as isize - 1
    } else {
        found.unwrap_or(0) as isize
    };
    let current = usize::try_from(index).ok().and_then(|at| steps.get(at));
    let lanes: &[String] = match current {
        Some(StepRef::Group { parallel, .. }) => parallel,
        _ => &[],
    };
    let open: Vec<&String> = lanes
        .iter()
        .filter(|id| lane_state(task, id) != Some(TaskState::Done))
        .collect();
    let open_states: Vec<TaskState> = open
        .iter()
        .map(|id| lane_state(task, id).unwrap_or(TaskState::Ready))
        .collect();
    let group_state = urgent_state(&open_states);
    let urgent_lane =
        group_state.and_then(|wanted| open.iter().find(|id| lane_state(task, id) == Some(wanted)));
    let state = if finished {
        task.state
    } else {
        group_state.unwrap_or(task.state)
    };
    let summary = summary_of(task, &steps, index);
    let stage = (!finished).then(|| {
        stage_name(match (urgent_lane, current) {
            (Some(lane), _) => lane,
            (None, Some(StepRef::Stage(id))) => id,
            _ => &task.stage,
        })
    });
    let progress = if steps.is_empty() || task.state == TaskState::Done {
        1.0
    } else {
        (index + 1) as f64 / steps.len() as f64
    };
    Comet {
        progress,
        at: (index + 1) as usize,
        of: steps.len(),
        state,
        stage,
        summary,
    }
}

/// The state word of step `at` in the summary. `index`: the current step, -1 for none.
fn mark(task: &Task, index: isize, at: usize, lane: Option<&str>) -> String {
    let at = at as isize;
    if at < index || task.state == TaskState::Done {
        return "done".into();
    }
    if at > index {
        return "not started".into();
    }
    lower(lane.map_or(task.state, |id| lane_state(task, id).unwrap_or(task.state)))
}

/// The stages of a branch step. Only the path a task took counts. Inside the current path, the
/// stages before the current one are done and the ones after it have not started.
fn path_marks(task: &Task, index: isize, id: &str, paths: &[PathRef], at: usize) -> Vec<String> {
    let chosen = task.paths.get(id).and_then(|value| value.as_str());
    let taken = paths
        .iter()
        .filter(|path| chosen.is_none_or(|chosen| path.id == chosen));
    taken
        .flat_map(|path| {
            let here = path.stages.iter().position(|each| *each == task.stage);
            path.stages
                .iter()
                .enumerate()
                .map(move |(place, stage)| (here, place, stage))
        })
        .map(|(here, place, stage)| {
            let state = match here {
                Some(here) if at as isize == index && place != here => {
                    if place < here {
                        "done".to_owned()
                    } else {
                        "not started".to_owned()
                    }
                }
                _ => mark(task, index, at, None),
            };
            format!("{} {state}", stage_name(stage))
        })
        .collect()
}

/// Every step and its state, for the tooltip and screen readers.
fn summary_of(task: &Task, steps: &[StepRef], index: isize) -> String {
    steps
        .iter()
        .enumerate()
        .flat_map(|(at, step)| match step {
            StepRef::Stage(id) => vec![format!(
                "{} {}",
                stage_name(id),
                mark(task, index, at, None)
            )],
            StepRef::Group { parallel, .. } => parallel
                .iter()
                .map(|id| format!("{} {}", stage_name(id), mark(task, index, at, Some(id))))
                .collect(),
            StepRef::Branch { id, paths } => path_marks(task, index, id, paths, at),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn make_task(overrides: Value) -> Task {
        let mut base = json!({
            "id": "T-0001", "title": "Example", "state": "running", "workspace": "/work/T-0001",
            "createdAt": "2026-10-08T09:00:00.000Z", "updatedAt": "2026-10-08T09:00:00.000Z"
        });
        if let (Some(base), Value::Object(extra)) = (base.as_object_mut(), overrides) {
            base.extend(extra);
        }
        serde_json::from_value(base).unwrap_or_default()
    }

    fn six() -> Value {
        json!(["context", "spec", "plan", {"id": "build", "parallel": ["test-spec", "implement"]},
            "testing", "review"])
    }

    #[test]
    fn puts_the_head_at_the_current_step_counting_it() {
        let comet = comet_of(&make_task(json!({"stages": six(), "stage": "plan"})));
        assert_eq!(comet.progress, 0.5);
        assert_eq!((comet.at, comet.of), (3, 6));
        assert_eq!(comet.state, TaskState::Running);
        assert_eq!(comet.stage.as_deref(), Some("Plan"));
        let first = comet_of(&make_task(json!({"stages": six(), "stage": "context"})));
        assert_eq!(first.progress, 1.0 / 6.0);
    }

    #[test]
    fn names_the_most_urgent_stage_of_a_group_and_takes_its_state() {
        let waiting = comet_of(&make_task(json!({
            "stages": six(), "stage": "build", "state": "needs-input",
            "lanes": {"test-spec": "done", "implement": "needs-input"}
        })));
        assert_eq!(waiting.stage.as_deref(), Some("Implement"));
        assert_eq!((waiting.state, waiting.at), (TaskState::NeedsInput, 4));
        let failed = comet_of(&make_task(json!({
            "stages": six(), "stage": "build", "state": "failed",
            "lanes": {"test-spec": "failed", "implement": "running"}
        })));
        assert_eq!(failed.stage.as_deref(), Some("Test spec"));
        assert_eq!(failed.state, TaskState::Failed);
    }

    #[test]
    fn counts_a_branch_as_one_step_and_names_only_the_path_the_task_took() {
        let stages = json!(["plan", {"id": "after-plan", "paths": [
            {"id": "small", "stages": ["implement", "testing"]},
            {"id": "pr-review", "stages": ["review"]}
        ]}, "release"]);
        let comet = comet_of(&make_task(json!({
            "stages": stages, "stage": "testing", "paths": {"after-plan": "small"}
        })));
        assert_eq!((comet.at, comet.of), (2, 3));
        assert_eq!(comet.stage.as_deref(), Some("Automated tests"));
        assert_eq!(comet.state, TaskState::Running);
        assert_eq!(
            comet.summary,
            "Plan done, Implement done, Automated tests running, Release not started"
        );
    }

    #[test]
    fn fills_a_finished_task_and_a_pipeline_of_one_step() {
        let done = comet_of(&make_task(
            json!({"stages": six(), "stage": "review", "state": "done"}),
        ));
        assert_eq!(
            (done.progress, done.state, done.stage),
            (1.0, TaskState::Done, None)
        );
        let one = comet_of(&make_task(
            json!({"stages": ["implement"], "stage": "implement"}),
        ));
        assert_eq!(one.progress, 1.0);
    }

    #[test]
    fn lists_every_step_with_its_state_for_the_tooltip() {
        let comet = comet_of(&make_task(json!({
            "stages": six(), "stage": "build", "state": "needs-input",
            "lanes": {"test-spec": "done", "implement": "needs-input"}
        })));
        assert_eq!(
            comet.summary,
            "Context done, Spec done, Plan done, Test spec done, Implement needs input, \
             Automated tests not started, Manual review not started"
        );
    }

    #[test]
    fn has_no_current_step_for_a_done_task_with_no_steps() {
        let comet = comet_of(&make_task(json!({"stages": [], "state": "done"})));
        assert_eq!((comet.progress, comet.at, comet.of), (1.0, 0, 0));
    }
}
