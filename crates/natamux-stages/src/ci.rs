//! Ports the types and names of `packages/daemon/src/stages/ci-check.ts` (not `checkOnCi`), and
//! `stagesWaitingForCi`, `ciError` and `ciReason` of `stage-gate.ts`.

use std::path::Path;

use natamux_core::js::is_js_space;
use natamux_proto::events::task_event::StoredEvent;
use natamux_proto::stages::capabilities::stage_name;
use natamux_proto::stages::schema::Stage;
use natamux_proto::terminals::text::plain_text;

/// The branch of origin that gets a snapshot of a task's work, for CI to run on.
pub const CI_BRANCH_PREFIX: &str = "natamux-ci/";

pub fn ci_branch_of(branch: &str) -> String {
    format!("{CI_BRANCH_PREFIX}{branch}")
}

/// `final_`: the agent cannot fix it, so the stage waits for the user. `logs`: the reasons with
/// the end of each failed log, for the agent only; the stage card shows the short reasons.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CiResult {
    pub passed: bool,
    pub reasons: Vec<String>,
    pub passes: Vec<String>,
    pub final_: bool,
    pub logs: Option<Vec<String>>,
}

/// Pushes the work of a task as it is now to its CI branch, waits for the CI runs of that commit,
/// and tells how they ended (`checkOnCi`). Blocking.
pub trait CiCheck: Send + Sync {
    fn check(&self, worktree: &Path, branch: &str) -> CiResult;
}

/// Until the CI check is ported: the answer of Node's `checkOnCi` where `gh` is missing.
pub struct NoCi;

impl CiCheck for NoCi {
    fn check(&self, _worktree: &Path, _branch: &str) -> CiResult {
        ci_error(
            "Could not read the CI runs: GitHub CLI failed: gh is not installed or not on the PATH",
        )
    }
}

/// The stages whose CI check started and has no result yet.
pub fn stages_waiting_for_ci(events: &[StoredEvent]) -> Vec<String> {
    let mut waits: Vec<String> = Vec::new();
    for stored in events {
        let event = &stored.event;
        let Some(stage) = event.payload.get("stage").and_then(|stage| stage.as_str()) else {
            continue;
        };
        if event.kind == "stage.ci-started" {
            if !waits.iter().any(|each| each == stage) {
                waits.push(stage.to_string());
            }
        } else if ["stage.started", "stage.checked", "stage.check-failed"]
            .contains(&event.kind.as_str())
        {
            waits.retain(|each| each != stage);
        }
    }
    waits
}

/// A CI check that could not run, such as gh missing, is a failure no agent can fix.
pub fn ci_error(text: &str) -> CiResult {
    CiResult {
        passed: false,
        reasons: vec![text.to_string()],
        passes: Vec::new(),
        final_: true,
        logs: None,
    }
}

/// The reason of a send back for a CI failure. `logs` go to the agent, after the blank line.
pub fn ci_reason(stage: &Stage, reasons: &[String], logs: &[String]) -> String {
    // The stage card shows the lines before the first blank line.
    let mut lines = vec![format!("CI failed on {}:", stage_name(stage))];
    lines.extend(reasons.iter().map(|each| format!("- {each}")));
    lines.push(String::new());
    lines.extend(
        logs.iter()
            .map(|log| plain_text(log).trim_end_matches(is_js_space).to_string()),
    );
    lines.push(
        "Read the failed log with `gh run view <run id> --log-failed`; the id ends the run url."
            .into(),
    );
    lines.join("\n")
}

#[cfg(test)]
#[path = "ci_tests.rs"]
mod tests;
