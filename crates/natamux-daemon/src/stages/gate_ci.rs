//! `stage-gate.ts`, CI part: the CI check of a stage whose local checks passed, its result, and
//! the send back of a CI failure to an agent. The CI itself is behind `CiCheck`; until part 5 the
//! daemon's answers as Node does without `gh`.

use std::path::Path;

use natamux_core::stages::capabilities::code_stage_before;
use natamux_proto::stages::capabilities::{offers_ci, stage_name, stage_name_of_id};
use natamux_proto::stages::schema::{Stage, StageKind};
use natamux_proto::tasks::task::Task;
use natamux_stages::actions::MAX_AGENT_SEND_BACKS;
use natamux_stages::ci::{CiResult, ci_branch_of, ci_error, ci_reason, stages_waiting_for_ci};
use serde_json::json;

use crate::stages::errors::StageResult;
use crate::stages::run::{EventInput, StageRun, key};
use crate::tasks::helpers::position_of;

impl StageRun {
    /// A CI check that ran when the daemon stopped goes on: it pushes the work again and waits.
    pub fn resume_ci_checks(&self) {
        for task in self.all_tasks() {
            if task.state.is_finished() {
                continue;
            }
            for stage_id in stages_waiting_for_ci(&self.events(&task.id)) {
                let seq = self.last_start(&task.id, &stage_id).seq;
                self.start_ci_round(&task.id, &stage_id, seq, Vec::new());
            }
        }
    }

    /// The task asked for CI on this stage, and has the branch to push.
    pub fn waits_for_ci(&self, task: &Task, stage: &Stage) -> bool {
        let chosen = task
            .ci_stages
            .as_ref()
            .is_some_and(|stages| stages.contains(&stage.id));
        chosen && offers_ci(stage) && task.workspace.is_some() && task.branch.is_some()
    }

    /// Starts the CI check of a stage whose local checks passed. A second report waits for the
    /// check that runs.
    pub fn ask_ci(
        &self,
        task: &Task,
        stage_id: &str,
        seq: i64,
        passes: Vec<String>,
    ) -> StageResult<()> {
        if self.state().ci_rounds.contains(&key(&task.id, stage_id)) {
            return Ok(());
        }
        let branch = ci_branch_of(task.branch.as_deref().unwrap_or(""));
        self.record(
            &task.id,
            "stage.ci-started",
            json!({ "stage": stage_id, "branch": branch }),
        )?;
        self.start_ci_round(&task.id, stage_id, seq, passes);
        Ok(())
    }

    fn start_ci_round(&self, task_id: &str, stage_id: &str, seq: i64, passes: Vec<String>) {
        let round = key(task_id, stage_id);
        if !self.state().ci_rounds.insert(round.clone()) {
            return;
        }
        let name = self
            .task(task_id)
            .and_then(|task| self.steps(&task))
            .ok()
            .and_then(|steps| {
                steps
                    .iter()
                    .flat_map(|step| step.stages())
                    .find(|stage| stage.id == stage_id)
                    .map(stage_name)
            })
            .unwrap_or_else(|| stage_name_of_id(stage_id));
        let (task_id, stage_id) = (task_id.to_string(), stage_id.to_string());
        self.background(move |run| {
            let label = format!("Waiting for CI on {name}");
            let result = run
                .options
                .jobs
                .run_blocking("ci", &label, Some(&task_id), || {
                    run.ci_round(&task_id, &stage_id, seq, passes)
                });
            if let Err(error) = result {
                eprintln!("The CI round of {task_id} failed: {error}");
            }
            run.state().ci_rounds.remove(&round);
        });
    }

    /// CI takes minutes, so it runs after the report has its answer. Its result counts only for
    /// the round of the stage that asked for it.
    fn ci_round(
        &self,
        task_id: &str,
        stage_id: &str,
        seq: i64,
        passes: Vec<String>,
    ) -> StageResult<()> {
        let task = self.task(task_id)?;
        // The workspace can be removed while CI waits, then the daemon restarts. An empty
        // worktree would run git in the daemon's own folder and push that checkout.
        let ci: CiResult = match (&task.workspace, &task.branch) {
            (Some(workspace), Some(branch)) => self.options.ci.check(Path::new(workspace), branch),
            _ => ci_error("The task has no workspace or branch for the CI check."),
        };
        if self.last_start(task_id, stage_id).seq != seq {
            return Ok(());
        }
        if ci.passed {
            let all = [passes, ci.passes].concat();
            return self.apply_checked(task_id, stage_id, seq, all);
        }
        let logs = ci.logs.clone().unwrap_or_else(|| ci.reasons.clone());
        if !ci.final_ && self.send_back_for_ci(task_id, stage_id, &ci.reasons, &logs)? {
            return Ok(());
        }
        self.fail_checks(task_id, stage_id, &ci.reasons, ci.final_, Some(&logs))
    }

    /// A CI failure goes back to an agent as a send back, at most MAX_AGENT_SEND_BACKS times, so
    /// the stage starts a new round and its prompt is pasted as a review's would be. An agent
    /// stage gets it with the log. A testing stage only runs the code, so its failure goes to the
    /// stage that changes code, without the log.
    fn send_back_for_ci(
        &self,
        task_id: &str,
        stage_id: &str,
        reasons: &[String],
        logs: &[String],
    ) -> StageResult<bool> {
        let task = self.task(task_id)?;
        let Some((stage, to)) = self.ci_target(&task, stage_id)? else {
            return Ok(false);
        };
        if self.send_backs_from(task_id, stage_id) >= MAX_AGENT_SEND_BACKS {
            self.fail_checks(task_id, stage_id, reasons, true, None)?;
            return Ok(true);
        }
        // Ends the CI wait of the stage, so a restart does not check it again.
        self.apply_system(
            task_id,
            "stage.check-failed",
            json!({ "stage": stage_id, "reasons": reasons, "final": false }),
        )?;
        let shown: &[String] = if stage.kind == StageKind::Agent {
            logs
        } else {
            &[]
        };
        let reason = ci_reason(&stage, reasons, shown);
        self.reopen_for_ci(&task, &to.id, stage_id, &reason)?;
        Ok(true)
    }

    /// The stage a CI failure goes back to: an agent stage itself, a testing stage the stage
    /// before it that changes code. None for other stages.
    fn ci_target(&self, task: &Task, stage_id: &str) -> StageResult<Option<(Stage, Stage)>> {
        let steps = self.steps(task)?;
        let step = self.step_of(task)?;
        let Some(stage) = step
            .stages()
            .into_iter()
            .find(|candidate| candidate.id == stage_id)
            .cloned()
        else {
            return Ok(None);
        };
        let to = match stage.kind {
            StageKind::Agent => Some(stage.clone()),
            StageKind::Testing => {
                code_stage_before(&steps, stage_id, position_of(task).paths.as_ref())
            }
            _ => None,
        };
        Ok(to.map(|to| (stage, to)))
    }

    fn reopen_for_ci(
        &self,
        task: &Task,
        to: &str,
        stage_id: &str,
        reason: &str,
    ) -> StageResult<()> {
        let step = self.step_of(task)?;
        self.state().leaving.insert(task.id.clone(), step);
        let input = EventInput::new(
            "system",
            "stage.reopened",
            json!({ "to": to, "reason": reason, "from": stage_id, "byAgent": true, "ci": true }),
        );
        let applied = self.apply(&task.id, &input);
        self.state().leaving.remove(&task.id);
        applied.map(|_| ())
    }
}
