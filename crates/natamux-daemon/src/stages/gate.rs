//! `stage-gate.ts`, local part: the report that a stage is done, its local checks, then the
//! pass or the fail. The CI part is in `gate_ci.rs`.

use std::path::PathBuf;

use natamux_core::stages::terminal_lane::terminal_of_stage_pinned;
use natamux_proto::projects::repo_trust::UNTRUSTED_REASON;
use natamux_proto::stages::capabilities::stage_name;
use natamux_proto::stages::schema::{Stage, StageKind};
use natamux_proto::tasks::task::Task;
use natamux_stages::checks::{CheckInput, CheckResult, run_stage_checks};
use natamux_stages::files::{read_stage_file, working_dir, write_stage_file};
use natamux_store::attachments::{list_attachments, with_attachment_section};
use serde_json::{Value, json};

use crate::stages::errors::{StageError, StageResult, action};
use crate::stages::run::{StageRun, key};

/// The last start of a stage: its place in the log, its time, the commit HEAD was on, and how
/// often its checks failed since.
#[derive(Clone, Debug, PartialEq)]
pub struct LastStart {
    pub seq: i64,
    pub at: Option<String>,
    pub head: Option<String>,
    pub failures: usize,
}

impl StageRun {
    /// The report that a stage is done, then its checks.
    pub fn report_and_check(
        &self,
        task_id: &str,
        stage: &str,
        source: &str,
        terminal: Option<&str>,
    ) -> StageResult<()> {
        let input = crate::stages::run::EventInput::new(
            source,
            "stage.reported",
            json!({ "stage": stage }),
        );
        // Noted before the report moves the task: a shell that the move ends waits for this
        // agent's turn end.
        let shell = terminal
            .filter(|_| source == "agent")
            .map(|terminal| key(task_id, terminal));
        let before = shell
            .as_ref()
            .and_then(|shell| self.state().reported.insert(shell.clone(), stage.into()));
        let accepted = self.apply(task_id, &input).map(|applied| applied.accepted);
        if let (false, Some(shell)) = (matches!(accepted, Ok(true)), &shell) {
            self.restore_report(shell, before);
        }
        if !accepted? {
            return Err(action(format!(
                "{task_id}: the {stage} stage is not running"
            )));
        }
        self.check(task_id, stage)
    }

    /// Undoes a report noted for an action that did not happen.
    pub fn restore_report(&self, shell: &str, before: Option<String>) {
        let mut state = self.state();
        match before {
            Some(stage) => state.reported.insert(shell.to_string(), stage),
            None => state.reported.remove(shell),
        };
    }

    pub fn last_start(&self, task_id: &str, stage: &str) -> LastStart {
        let events = self.events(task_id);
        let at = events.iter().rposition(|stored| {
            stored.event.kind == "stage.started"
                && stored.event.payload.get("stage").and_then(Value::as_str) == Some(stage)
        });
        let Some(index) = at else {
            return LastStart {
                seq: -1,
                at: None,
                head: None,
                failures: 0,
            };
        };
        let stored = &events[index];
        let failures = events[index..]
            .iter()
            .filter(|later| {
                later.event.kind == "stage.check-failed"
                    && later.event.payload.get("stage").and_then(Value::as_str) == Some(stage)
            })
            .count();
        LastStart {
            seq: stored.seq as i64,
            at: Some(stored.event.at.clone()),
            head: stored
                .event
                .payload
                .get("head")
                .and_then(Value::as_str)
                .map(str::to_string),
            failures,
        }
    }

    /// Two reports at once (the agent's and a person's) would run the checks twice in one
    /// worktree, and the second failure would count as the agent's last try.
    fn check(&self, task_id: &str, stage_id: &str) -> StageResult<()> {
        let checking = format!(
            "{task_id}/{stage_id}/{}",
            self.last_start(task_id, stage_id).seq
        );
        if !self.state().checking.insert(checking.clone()) {
            return Ok(());
        }
        let result = self.check_once(task_id, stage_id);
        self.state().checking.remove(&checking);
        result
    }

    fn check_once(&self, task_id: &str, stage_id: &str) -> StageResult<()> {
        let task = self.task(task_id)?;
        let step = self.step_of(&task)?;
        let Some(stage) = step
            .stages()
            .into_iter()
            .find(|candidate| candidate.id == stage_id)
            .cloned()
        else {
            return Ok(());
        };
        if stage.kind == StageKind::Context {
            self.add_attachment_section(&task, stage.output.as_deref().unwrap_or("context.md"));
        }
        // Nothing an agent can fix: the user approves the commands first.
        if stage.check.is_some() && !self.repo_trusted(&task) {
            return self.fail_checks(task_id, stage_id, &[UNTRUSTED_REASON.into()], true, None);
        }
        let start = self.last_start(task_id, stage_id);
        let label = format!("Checking {}", stage_name(&stage));
        let result = self
            .options
            .jobs
            .run_blocking("stage-check", &label, Some(task_id), || {
                self.run_checks(&task, &stage, &start)
            });
        // The stage started again while the checks ran: their result is about the round before.
        if self.last_start(task_id, stage_id).seq != start.seq {
            return Ok(());
        }
        if result.passed && self.waits_for_ci(&task, &stage) {
            self.ask_ci(&task, stage_id, start.seq, result.passes)?;
            return Ok(());
        }
        if result.passed {
            return self.apply_checked(task_id, stage_id, start.seq, result.passes);
        }
        self.fail_checks(task_id, stage_id, &result.reasons, false, None)
    }

    /// The local checks of the stage. Checks that cannot run fail.
    fn run_checks(&self, task: &Task, stage: &Stage, start: &LastStart) -> CheckResult {
        let folder = working_dir(self.data_dir(), &task.id, task.workspace.as_deref());
        let cwd = task
            .workspace
            .as_ref()
            .map_or_else(|| folder.clone(), PathBuf::from);
        let input = CheckInput {
            stage: stage.clone(),
            folder,
            cwd,
            is_worktree: task.workspace.is_some(),
            started_at: start.at.clone(),
            start_head: start.head.clone(),
        };
        run_stage_checks(&input, &self.options.run_command).unwrap_or_else(|error| CheckResult {
            passed: false,
            reasons: vec![error.to_string()],
            passes: Vec::new(),
        })
    }

    /// The conditions of a branch after the stage can take minutes, and the stage may start
    /// again meanwhile: then the checks are about the round before.
    pub fn apply_checked(
        &self,
        task_id: &str,
        stage_id: &str,
        seq: i64,
        passes: Vec<String>,
    ) -> StageResult<()> {
        let input = crate::stages::run::EventInput::new(
            "system",
            "stage.checked",
            json!({ "stage": stage_id, "passes": passes }),
        );
        let event = match self.with_path(task_id, input) {
            Ok(event) => event,
            Err(StageError::Action(reason)) if reason == UNTRUSTED_REASON => {
                return self.fail_checks(task_id, stage_id, &[reason], true, None);
            }
            Err(error) => return Err(error),
        };
        if self.last_start(task_id, stage_id).seq == seq {
            self.apply(task_id, &event)?;
        }
        Ok(())
    }

    /// An agent gets one more try; a stage that no agent works on, or a failure no agent can
    /// fix, waits for you at once. `logs`: what the agent reads, when it is longer than the
    /// reasons the stage card shows.
    pub fn fail_checks(
        &self,
        task_id: &str,
        stage_id: &str,
        reasons: &[String],
        cannot_fix: bool,
        logs: Option<&[String]>,
    ) -> StageResult<()> {
        let task = self.task(task_id)?;
        let step = self.step_of(&task)?;
        let Some(stage) = step
            .stages()
            .into_iter()
            .find(|candidate| candidate.id == stage_id)
            .cloned()
        else {
            return Ok(());
        };
        let start = self.last_start(task_id, stage_id);
        let steps = self.steps(&task)?;
        let terminal = terminal_of_stage_pinned(&steps, stage_id, &self.pinned(task_id));
        let agent_stage = matches!(stage.kind, StageKind::Agent | StageKind::Testing)
            || self.is_agent_context(&task);
        let agent_works = agent_stage && self.agent_runs(task_id, &terminal);
        let last = cannot_fix || !agent_works || start.failures >= 1;
        self.apply_system(
            task_id,
            "stage.check-failed",
            json!({ "stage": stage_id, "reasons": reasons, "final": last }),
        )?;
        if !last {
            let mut lines = vec!["The checks of this stage failed:".to_string()];
            lines.extend(
                logs.unwrap_or(reasons)
                    .iter()
                    .map(|reason| format!("- {reason}")),
            );
            lines.push("Fix them, then run `natamux done` again.".into());
            // The terminal closed since: the reasons are in the log, and the next failure waits.
            let _ = self
                .options
                .terminals
                .paste(task_id, &lines.join("\n"), &terminal);
        }
        Ok(())
    }

    /// An agent that read the context from a skill or an MCP server may leave the attachments
    /// out of it. The later stages find them only through the context.
    fn add_attachment_section(&self, task: &Task, output: &str) {
        let attachments = list_attachments(self.data_dir(), &task.id).unwrap_or_default();
        if attachments.is_empty() {
            return;
        }
        let Ok(Some(text)) =
            read_stage_file(self.data_dir(), &task.id, task.workspace.as_deref(), output)
        else {
            return;
        };
        let next = with_attachment_section(&text, &attachments);
        if next != text {
            let _ = write_stage_file(
                self.data_dir(),
                &task.id,
                task.workspace.as_deref(),
                output,
                &next,
            );
        }
    }
}
