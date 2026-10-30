//! The writers that log one kind of change of a task, for the other services and the routes
//! (`task-records.ts`).

use natamux_core::events::apply::apply_task_event;
use natamux_core::js::parse_iso_millis;
use natamux_core::stages::position::step_index;
use natamux_core::tasks::task::issue_link;
use natamux_core::zod::{Issues, ZodError};
use natamux_proto::events::task_event::TaskEvent;
use natamux_proto::stages::capabilities::{
    can_run_workflow, offers_ci, offers_workflow, stage_name,
};
use natamux_proto::stages::position::path_of;
use natamux_proto::stages::schema::{StageKind, Step};
use natamux_proto::tasks::task::{IssueLink, PullRequestStatus, Task, TaskState};
use serde_json::{Value, json};

use crate::shared::event_bus::DaemonMessage;
use crate::tasks::errors::{TaskError, TaskResult};
use crate::tasks::helpers::{
    ACTIVITY_FILE_INTERVAL_MS, ACTIVITY_INTERVAL_MS, move_task, position_of,
};
use crate::tasks::store::{Applied, Store, event_at, logged};

/// Where the task's working copy lives now; None for both after a removal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceChange {
    pub workspace: Option<String>,
    pub branch: Option<String>,
}

/// How often a sign of life is written, and whether it ends a wait for background work.
#[derive(Clone, Copy, Debug)]
pub struct ActivityOptions {
    pub min_interval_ms: i64,
    pub ends_wait: bool,
}

impl Default for ActivityOptions {
    fn default() -> ActivityOptions {
        ActivityOptions {
            min_interval_ms: ACTIVITY_INTERVAL_MS,
            ends_wait: true,
        }
    }
}

/// The events a new status of a pull request logs: only what changed.
fn pull_request_event_types(
    before: Option<&PullRequestStatus>,
    status: &PullRequestStatus,
) -> Vec<&'static str> {
    let checks_changed = before.is_none_or(|before| before.checks != status.checks);
    let state_changed = before.is_none_or(|before| before.state != status.state);
    let mut types = Vec::new();
    if checks_changed && status.checks == "passing" {
        types.push("ci.passed");
    }
    if checks_changed && status.checks == "failing" {
        types.push("ci.failed");
    }
    if state_changed && status.state == "merged" {
        types.push("pr.merged");
    }
    if state_changed && status.state == "closed" {
        types.push("pr.closed");
    }
    types
}

impl Store {
    /// Stores the task with one accepted event and tells the dashboards.
    fn record(&mut self, updated: Task, event: TaskEvent) -> TaskResult<Task> {
        self.sync
            .store(updated.clone(), &[logged(event.clone(), true)], true)?;
        self.publish(DaemonMessage::TaskUpdated {
            task: updated.clone(),
            event,
            accepted: true,
            moved: None,
        });
        Ok(updated)
    }

    /// Records where the task's working copy lives. Called by the workspace service.
    pub fn record_workspace(&mut self, task_id: &str, change: WorkspaceChange) -> TaskResult<Task> {
        let task = self.get_task(task_id)?;
        let kind = if change.workspace.is_none() {
            "workspace.removed"
        } else {
            "workspace.created"
        };
        let workspace = change.workspace.clone().or_else(|| task.workspace.clone());
        let event = self.event(
            task_id,
            "system",
            kind,
            json!({ "workspace": workspace, "branch": change.branch }),
        );
        let updated = Task {
            workspace: change.workspace,
            branch: change.branch,
            updated_at: event.at.clone(),
            ..task
        };
        self.record(updated, event)
    }

    /// Writes down why a workspace that was made after its request answered could not be made.
    pub fn record_workspace_failure(&mut self, task_id: &str, message: &str) -> TaskResult<Task> {
        let task = self.get_task(task_id)?;
        let event = self.event(
            task_id,
            "system",
            "workspace.failed",
            json!({ "message": message }),
        );
        let updated = Task {
            updated_at: event.at.clone(),
            ..task
        };
        self.record(updated, event)
    }

    /// Records the branch the agent switched the task's worktree to. Called by the workspace
    /// service.
    pub fn record_branch(
        &mut self,
        task_id: &str,
        from: Option<String>,
        to: String,
    ) -> TaskResult<Task> {
        let task = self.get_task(task_id)?;
        // Two callers may see the same switch; the second one logs nothing.
        if task.branch.as_deref() == Some(to.as_str()) {
            return Ok(task);
        }
        let event = self.event(
            task_id,
            "system",
            "workspace.branch-changed",
            json!({ "from": from, "to": to }),
        );
        let updated = Task {
            branch: Some(to),
            updated_at: event.at.clone(),
            ..task
        };
        self.record(updated, event)
    }

    /// Natamux ended the agents of the task. A stage whose agent ran or waited for you would stay
    /// so with no agent, and would show up in Needs you later.
    pub fn stop_agents(&mut self, task_id: &str) -> TaskResult<Task> {
        let task = self.get_task(task_id)?;
        let position = position_of(&task);
        let lanes = if position.lanes.is_empty() {
            vec![(task.stage.clone(), task.state)]
        } else {
            position.lanes
        };
        for (stage, state) in lanes {
            if matches!(state, TaskState::Running | TaskState::NeedsInput) {
                let input = json!({ "source": "system", "type": "agent.stopped", "payload": { "stage": stage } });
                self.apply_event(task_id, Some(&input))?;
            }
        }
        self.get_task(task_id)
    }

    /// Writes down a commit, a push or a new pull request. A pull request also sets `pr`.
    pub fn record_git(&mut self, task_id: &str, kind: &str, payload: Value) -> TaskResult<Task> {
        let task = self.get_task(task_id)?;
        let url = payload
            .get("url")
            .and_then(Value::as_str)
            .filter(|url| !url.is_empty())
            .map(str::to_string);
        let event = self.event(task_id, "git", kind, payload);
        let pr = match (kind, url) {
            ("pr.opened", Some(url)) => Some(url),
            _ => task.pr.clone(),
        };
        let updated = Task {
            pr,
            updated_at: event.at.clone(),
            ..task
        };
        self.record(updated, event)
    }

    /// Writes down a review comment that was sent to the agent of the task.
    pub fn record_review_comment(&mut self, task_id: &str, text: &str) -> TaskResult<Task> {
        let task = self.get_task(task_id)?;
        let event = self.event(
            task_id,
            "human",
            "review.commented",
            json!({ "text": text }),
        );
        let updated = Task {
            updated_at: event.at.clone(),
            ..task
        };
        self.record(updated, event)
    }

    /// Writes down that a batch of review comments was sent to the agent of the task.
    pub fn record_review_comments(&mut self, task_id: &str, ids: &[String]) -> TaskResult<Task> {
        let task = self.get_task(task_id)?;
        let event = self.event(
            task_id,
            "human",
            "review.commented",
            json!({ "comments": ids }),
        );
        let updated = Task {
            updated_at: event.at.clone(),
            ..task
        };
        self.record(updated, event)
    }

    /// Writes down that an agent was started in the task's terminal, and names it on the task. The
    /// agent of a stage that starts its own, or one restored in its shell, is not the task's.
    pub fn record_agent_launch(
        &mut self,
        task_id: &str,
        agent: &str,
        program: Option<&str>,
        stage: Option<&str>,
        restored_in: Option<&str>,
    ) -> TaskResult<Task> {
        let task = self.get_task(task_id)?;
        let mut payload = json!({ "agent": agent });
        if let Some(map) = payload.as_object_mut() {
            if let Some(program) = program {
                map.insert("program".into(), json!(program));
            }
            if let Some(stage) = stage {
                map.insert("stage".into(), json!(stage));
            }
            if let Some(terminal) = restored_in {
                map.insert("terminal".into(), json!(terminal));
                map.insert("restored".into(), json!(true));
            }
        }
        let event = self.event(task_id, "human", "agent.launched", payload);
        let own = stage.is_none() && restored_in.is_none();
        let updated = Task {
            agent: if own {
                Some(agent.into())
            } else {
                task.agent.clone()
            },
            updated_at: event.at.clone(),
            ..task
        };
        self.record(updated, event)
    }

    /// Keeps what GitHub says about the task's pull request. Logs only what changed; a merge
    /// finishes a task in review.
    pub fn record_pull_request_status(
        &mut self,
        task_id: &str,
        status: PullRequestStatus,
    ) -> TaskResult<Task> {
        let task = self.get_task(task_id)?;
        let before = task.pr_status.clone();
        if before.as_ref().is_some_and(|before| *before == status) {
            return Ok(task);
        }
        let at = self.timestamp();
        let steps = self.steps_of(&task);
        let mut moving = task.clone();
        let mut logged_events = Vec::new();
        for kind in pull_request_event_types(before.as_ref(), &status) {
            let event = event_at(&task.id, "git", kind, json!({ "url": task.pr }), at.clone());
            let applied = apply_task_event(&position_of(&moving), &event, &steps);
            moving = move_task(&moving, &applied.position, applied.moved, &at);
            logged_events.push(logged(event, applied.accepted));
        }
        let updated = Task {
            pr_status: Some(status),
            updated_at: at,
            ..moving
        };
        self.sync.store(updated.clone(), &logged_events, true)?;
        for entry in &logged_events {
            self.publish(DaemonMessage::TaskUpdated {
                task: updated.clone(),
                event: entry.event.clone(),
                accepted: entry.accepted,
                moved: Some(updated.stage != task.stage),
            });
        }
        if logged_events.is_empty() {
            self.publish(DaemonMessage::TaskChanged {
                task: updated.clone(),
            });
        }
        Ok(updated)
    }

    /// Links the task to the item that tracks it elsewhere: a GitHub issue, a Jira ticket.
    pub fn link_issue(&mut self, task_id: &str, raw: &Value, source: &str) -> TaskResult<Task> {
        let task = self.get_task(task_id)?;
        let mut issues = Issues::default();
        let link = issue_link(raw, &[], &mut issues);
        let link = issues.into_result(link)?;
        let issue: IssueLink = serde_json::from_value(link.clone()).map_err(|error| {
            TaskError::Invalid(ZodError(vec![natamux_core::zod::Issue::custom(
                &[],
                &error.to_string(),
            )]))
        })?;
        let event = self.event(task_id, source, "issue.linked", link);
        let updated = Task {
            issue: Some(issue),
            updated_at: event.at.clone(),
            ..task
        };
        self.record(updated, event)
    }

    fn stage_of(
        &self,
        task: &Task,
        stage_id: &str,
    ) -> TaskResult<(Vec<Step>, natamux_proto::stages::schema::Stage)> {
        let steps = self.steps_of(task);
        let stage = steps
            .iter()
            .flat_map(Step::stages)
            .find(|each| each.id == stage_id)
            .cloned()
            .ok_or_else(|| TaskError::Edit(format!("{} has no stage \"{stage_id}\"", task.id)))?;
        Ok((steps, stage))
    }

    /// Chooses whether a stage of the task runs as a workflow, from its next start.
    pub fn set_workflow(&mut self, task_id: &str, stage_id: &str, on: bool) -> TaskResult<Task> {
        let task = self.get_task(task_id)?;
        let (_, stage) = self.stage_of(&task, stage_id)?;
        if on && !offers_workflow(&stage) {
            return Err(TaskError::Edit(format!(
                "The {} stage cannot run with a workflow. Turn it on for the stage in the project settings first",
                stage_name(&stage)
            )));
        }
        if on && !can_run_workflow(task.agent.as_deref()) {
            return Err(TaskError::Edit(format!(
                "Only Claude Code can run a workflow; {task_id} uses {}",
                task.agent.as_deref().unwrap_or("null")
            )));
        }
        let stages = toggle(&task.workflow_stages, stage_id, on);
        if stages.len() == task.workflow_stages.len() {
            return Ok(task);
        }
        let event = self.event(
            task_id,
            "human",
            "stage.workflow-set",
            json!({ "stage": stage_id, "on": on }),
        );
        let updated = Task {
            workflow_stages: stages,
            updated_at: event.at.clone(),
            ..task
        };
        self.record(updated, event)
    }

    /// Whether the stage waits for CI before it passes. The project settings must offer it.
    pub fn set_ci(&mut self, task_id: &str, stage_id: &str, on: bool) -> TaskResult<Task> {
        let task = self.get_task(task_id)?;
        let (_, stage) = self.stage_of(&task, stage_id)?;
        if on && !offers_ci(&stage) {
            return Err(TaskError::Edit(format!(
                "The {} stage cannot be checked on CI. Turn it on for the stage in the project settings first",
                stage_name(&stage)
            )));
        }
        let before = task.ci_stages.clone().unwrap_or_default();
        let stages = toggle(&before, stage_id, on);
        if stages.len() == before.len() {
            return Ok(task);
        }
        let event = self.event(
            task_id,
            "human",
            "stage.ci-set",
            json!({ "stage": stage_id, "on": on }),
        );
        let updated = Task {
            ci_stages: Some(stages),
            updated_at: event.at.clone(),
            ..task
        };
        self.record(updated, event)
    }

    /// Marks a later stage to skip. The current stage has its own Skip; a mark comes off at any
    /// stage, but not once the task is over.
    pub fn set_skip(&mut self, task_id: &str, stage_id: &str, on: bool) -> TaskResult<Task> {
        let task = self.get_task(task_id)?;
        let (steps, stage) = self.stage_of(&task, stage_id)?;
        if task.state.is_finished() {
            return Err(TaskError::Edit(format!(
                "{task_id} is {}, so it has no stage left to skip",
                task.state.name()
            )));
        }
        if on && stage.kind == StageKind::Context {
            return Err(TaskError::Edit(
                "The context stage comes first, so it cannot be skipped early".into(),
            ));
        }
        let target = step_index(&steps, stage_id);
        let at = step_index(&steps, &task.stage);
        // Stages of one path share the index of their branch.
        let place = |id: &str| path_of(&steps, id).map_or(0, |place| place.index);
        let earlier = place(stage_id) <= place(&task.stage);
        if on && (target < at || (target == at && earlier)) {
            return Err(TaskError::Edit(format!(
                "{task_id} is at or past the {} stage. Use Skip on the current stage",
                stage_name(&stage)
            )));
        }
        let before = task.skip_stages.clone().unwrap_or_default();
        let stages = toggle(&before, stage_id, on);
        if stages.len() == before.len() {
            return Ok(task);
        }
        let event = self.event(
            task_id,
            "human",
            "stage.skip-set",
            json!({ "stage": stage_id, "on": on }),
        );
        let updated = Task {
            skip_stages: Some(stages),
            updated_at: event.at.clone(),
            ..task
        };
        self.record(updated, event)
    }

    /// Marks a task as left to a person, or not. Only the stage runner reads it, so no event.
    pub fn set_without_agent(&mut self, task_id: &str, on: bool) -> TaskResult<()> {
        let task = self.get_task(task_id)?;
        if (task.without_agent == Some(true)) != on {
            let without_agent = if on { Some(true) } else { None };
            self.sync.store(
                Task {
                    without_agent,
                    ..task
                },
                &[],
                true,
            )?;
        }
        Ok(())
    }

    /// Writes down an event that changes no state, such as fetched context.
    pub fn record_event(
        &mut self,
        task_id: &str,
        kind: &str,
        payload: Value,
        source: &str,
    ) -> TaskResult<Task> {
        let task = self.get_task(task_id)?;
        let event = self.event(task_id, source, kind, payload);
        let updated = Task {
            updated_at: event.at.clone(),
            ..task
        };
        self.record(updated, event)
    }

    /// Moves the time of the last update of a running task to its agent's last sign of life, at
    /// most once per `min_interval_ms`; the first sign of life ends a wait for background work at
    /// once. No line in the log. Returns whether it changed anything.
    pub fn record_activity(&mut self, task_id: &str, options: ActivityOptions) -> TaskResult<bool> {
        let task = self.get_task(task_id)?;
        if task.state != TaskState::Running {
            return Ok(false);
        }
        let at = self.timestamp();
        let millis = |text: &str| parse_iso_millis(text).unwrap_or(0);
        let ends_wait_now = options.ends_wait && task.background_since.is_some();
        if !ends_wait_now && millis(&at) - millis(&task.updated_at) < options.min_interval_ms {
            return Ok(false);
        }
        let background_since = if options.ends_wait {
            None
        } else {
            task.background_since.clone()
        };
        // When only the time moves, the file can wait: the next real change writes it anyway.
        let written_at = self.sync.written_at.get(task_id).cloned();
        let write_file = background_since != task.background_since
            || written_at
                .is_none_or(|written| millis(&at) - millis(&written) >= ACTIVITY_FILE_INTERVAL_MS);
        let updated = Task {
            background_since,
            updated_at: at,
            ..task
        };
        self.sync.store(updated.clone(), &[], write_file)?;
        self.publish(DaemonMessage::TaskChanged { task: updated });
        Ok(true)
    }

    /// `apply_event` for the stage engine and routes that build their own input.
    pub fn apply(&mut self, task_id: &str, input: &Value) -> TaskResult<Applied> {
        self.apply_event(task_id, Some(input))
    }
}

/// `[...new Set([...list, id])]` or the list without `id`.
fn toggle(list: &[String], id: &str, on: bool) -> Vec<String> {
    if on {
        let mut next = list.to_vec();
        if !next.iter().any(|each| each == id) {
            next.push(id.into());
        }
        let mut seen = std::collections::HashSet::new();
        next.retain(|each| seen.insert(each.clone()));
        next
    } else {
        list.iter().filter(|each| *each != id).cloned().collect()
    }
}
