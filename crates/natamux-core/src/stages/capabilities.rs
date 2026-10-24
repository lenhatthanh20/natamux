//! The parts of `packages/core/src/stages/capabilities.ts` that `natamux-proto` lacks:
//! `stagesBefore`, `codeStageBefore`, `runsWorkflow`.

use natamux_proto::stages::capabilities::{can_run_workflow, offers_workflow};
use natamux_proto::stages::position::path_of;
use natamux_proto::stages::schema::{Stage, StageKind, Step};

use crate::stages::position::{Paths, step_index, taken_stages};

/// The stages before stage `id` on the way the task went: the earlier steps, then the earlier
/// stages of its own path.
pub fn stages_before(steps: &[Step], id: &str, paths: Option<&Paths>) -> Vec<Stage> {
    let index = step_index(steps, id).max(0) as usize;
    let mut before: Vec<Stage> = steps
        .iter()
        .take(index)
        .flat_map(|step| taken_stages(step, paths))
        .collect();
    if let Some(place) = path_of(steps, id) {
        before.extend(place.path.stages.iter().take(place.index).cloned());
    }
    before
}

/// The last stage before stage `id` that changes code: where a CI failure of a later stage is
/// fixed. With the task's `paths`, only the paths it took count.
pub fn code_stage_before(steps: &[Step], id: &str, paths: Option<&Paths>) -> Option<Stage> {
    stages_before(steps, id, paths)
        .into_iter()
        .rfind(|stage| stage.kind == StageKind::Agent && stage.output.is_none())
}

/// Whether the next start of the stage asks the agent for a workflow: the settings offer it, the
/// task chose it, and the agent can run one.
pub fn runs_workflow(stage: &Stage, chosen: &[String], agent: Option<&str>) -> bool {
    offers_workflow(stage) && chosen.contains(&stage.id) && can_run_workflow(agent)
}

#[cfg(test)]
#[path = "capabilities_tests.rs"]
mod tests;
