//! The fake's screen-3 data and writes: the reference pipeline, the stage events, and the stage
//! actions, toggles, edits, templates, tries and issue of the details panel. Each write changes
//! the fake data and sends the changed tasks, as the daemon does.

use std::collections::BTreeMap;

use natamux_client::ClientError;
use natamux_proto::agents::activity::TaskUsage;
use natamux_proto::events::task_event::{StoredEvent, TaskEvent};
use natamux_proto::stages::actions::{StageAction, StageActionInput, StageFile};
use natamux_proto::stages::catalog::catalog_stage;
use natamux_proto::stages::position::{describe_condition, path_of};
use natamux_proto::stages::resolve::resolve_steps;
use natamux_proto::stages::schema::{
    BranchPath, Condition, Gate, Stage, StageBranch, StageGroup, StageKind, Step,
};
use natamux_proto::tasks::task::{IssueLink, Task, TaskChanges, TaskState};
use natamux_proto::tasks::template::{TaskTemplate, TemplateInput};
use natamux_proto::workspaces::workspace::{DiffStat, FanOutInput};
use serde_json::{Map, Value, json};

use super::{Answer, Data, FakeDaemon, NOW, not_found};
use crate::sidebar::sidebar_tree::steps_of;

const STAGES_SEED: &str = include_str!("fake_stages.json");

fn stage_of(id: &str) -> Step {
    Step::Stage(catalog_stage(id).unwrap_or_default())
}

/// The reference pipeline of the shots: a branch on the kind of work, a group, and the reviews.
pub(super) fn reference_pipeline() -> Vec<Step> {
    let reproduce = Stage {
        name: Some("Reproduce the bug".into()),
        output: Some("repro.md".into()),
        agent: Some("cursor".into()),
        args: Some("--model sonnet-4.5".into()),
        prompt: Some(
            "Reproduce the bug first. Write the steps and the failing case to repro.md, \
             then stop for review."
                .into(),
        ),
        ..Stage::new("reproduce", StageKind::Agent, Gate::Auto)
    };
    let path = |id: &str, name: &str, when: Option<Condition>, stage: Stage| BranchPath {
        id: id.into(),
        name: Some(name.into()),
        when,
        not: None,
        stages: vec![stage],
    };
    let bug = Condition::Command {
        run: "grep -qi bug .natamux-task/context.md".into(),
    };
    let one = |id: &str| catalog_stage(id).unwrap_or_default();
    let test_spec = Stage {
        agent: Some("gemini".into()),
        ..one("test-spec")
    };
    let review = Stage {
        name: Some("Review".into()),
        agent: Some("codex".into()),
        ..one("agent-review")
    };
    vec![
        stage_of("context"),
        stage_of("spec"),
        Step::Branch(StageBranch {
            id: "kind".into(),
            name: Some("Bug or feature".into()),
            paths: vec![
                path("bug", "Bug fix", Some(bug), reproduce),
                path("feature", "Feature", None, one("plan")),
            ],
        }),
        Step::Group(StageGroup {
            id: "build".into(),
            name: Some("Test spec and Implement".into()),
            parallel: vec![test_spec, one("implement")],
        }),
        Step::Stage(review),
        stage_of("security"),
        stage_of("review"),
        stage_of("release"),
    ]
}

/// The stages of a task in the order it runs them: the path it took (the first, before it chose
/// one) of each branch.
fn run_order(task: &Task) -> Vec<String> {
    let steps = resolve_steps(&steps_of(task), &reference_pipeline());
    let mut order = Vec::new();
    for step in &steps {
        match step {
            Step::Branch(branch) => {
                let taken = task.paths.get(&branch.id).and_then(Value::as_str);
                let path = branch
                    .paths
                    .iter()
                    .find(|path| Some(path.id.as_str()) == taken)
                    .or(branch.paths.first());
                order.extend(
                    path.iter()
                        .flat_map(|path| &path.stages)
                        .map(|s| s.id.clone()),
                );
            }
            other => order.extend(other.stages().into_iter().map(|stage| stage.id.clone())),
        }
    }
    order
}

fn kind_of(task: &Task, id: &str) -> Option<Stage> {
    let steps = resolve_steps(&steps_of(task), &reference_pipeline());
    steps
        .iter()
        .flat_map(Step::stages)
        .find(|stage| stage.id == id)
        .cloned()
}

fn status(code: u16, message: &str) -> ClientError {
    ClientError::Status {
        status: code,
        message: message.into(),
    }
}

impl Data {
    /// Adds the screen-3 tasks and their events, files, terminals and stats to the seed.
    pub(super) fn add_stage_seed(&mut self) {
        let seed: Value = serde_json::from_str(STAGES_SEED).unwrap_or(Value::Null);
        let tasks: Vec<Task> = part!(seed, "tasks");
        self.tasks.extend(tasks);
        let events: BTreeMap<String, Vec<StoredEvent>> = part!(seed, "events");
        for (id, list) in events {
            self.events.entry(id).or_default().extend(list);
        }
        let diffs: BTreeMap<String, DiffStat> = part!(seed, "diffStats");
        self.diff_stats.extend(diffs);
        let terminals: BTreeMap<String, _> = part!(seed, "terminals");
        self.terminals.extend(terminals);
        let files: BTreeMap<String, Vec<StageFile>> = part!(seed, "stageFiles");
        self.stage_files.extend(files);
        let usage: BTreeMap<String, TaskUsage> = part!(seed, "usage");
        self.usage.extend(usage);
    }

    /// Logs an accepted event of the task at the time of every fake write.
    pub(super) fn record(&mut self, task_id: &str, source: &str, kind: &str, payload: Value) {
        let list = self.events.entry(task_id.into()).or_default();
        let seq = list.iter().map(|stored| stored.seq).max().unwrap_or(0) + 1;
        let payload: Map<String, Value> = match payload {
            Value::Object(map) => map,
            _ => Map::new(),
        };
        list.push(StoredEvent {
            seq,
            accepted: true,
            event: TaskEvent {
                task_id: task_id.into(),
                source: source.into(),
                kind: kind.into(),
                at: NOW.into(),
                payload,
            },
        });
    }

    /// Starts `next`, or ends the task when there is none.
    fn advance(&mut self, task_id: &str, next: Option<String>) -> Answer<()> {
        let Some(next) = next else {
            self.task_mut(task_id)?.state = TaskState::Done;
            return Ok(());
        };
        let task = self.task_mut(task_id)?;
        let steps = resolve_steps(&steps_of(task), &reference_pipeline());
        let mut payload = json!({ "stage": next });
        if let Some(place) = path_of(&steps, &next) {
            let first = place.index == 0 && !task.paths.contains_key(&place.branch.id);
            if first {
                task.paths
                    .insert(place.branch.id.clone(), json!(place.path.id));
                payload["path"] = json!(place.path.id);
                payload["because"] = json!(describe_condition(place.path));
            }
        }
        task.stage = next;
        task.state = TaskState::Running;
        task.stage_since = Some(NOW.into());
        self.record(task_id, "system", "stage.started", payload);
        Ok(())
    }
}

/// The stage after `stage` in the task's run order.
fn next_after(task: &Task, stage: &str) -> Option<String> {
    let order = run_order(task);
    let at = order.iter().position(|id| id == stage).or_else(|| {
        // A group or branch id: the last stage it holds.
        let steps = resolve_steps(&steps_of(task), &reference_pipeline());
        let held = steps.iter().find(|step| step.id() == stage)?.stages();
        held.iter()
            .filter_map(|held| order.iter().position(|id| *id == held.id))
            .max()
    })?;
    order.get(at + 1).cloned()
}

impl FakeDaemon {
    pub fn stage_action(&self, task_id: &str, input: StageActionInput) -> Answer<Task> {
        let mut data = self.data();
        let task = data.task_mut(task_id)?.clone();
        let stage = input.stage.clone().unwrap_or_else(|| task.stage.clone());
        let next = next_after(&task, &stage);
        let reason = input.reason.clone();
        let payload = json!({ "stage": stage });
        match input.action {
            StageAction::Approve | StageAction::Pass | StageAction::Skip => {
                let manual_review =
                    kind_of(&task, &stage).is_some_and(|s| s.kind == StageKind::Review);
                let (source, kind) = match input.action {
                    StageAction::Skip => ("human", "stage.skipped"),
                    StageAction::Pass => ("system", "stage.passed"),
                    _ if manual_review => ("human", "review.approved"),
                    _ => ("human", "stage.approved"),
                };
                data.record(task_id, source, kind, payload);
                data.advance(task_id, next)?;
            }
            StageAction::Report => {
                data.record(task_id, "agent", "stage.reported", payload.clone());
                let manual = kind_of(&task, &stage).is_none_or(|s| s.gate == Gate::Manual);
                if manual {
                    data.task_mut(task_id)?.state = TaskState::Review;
                } else {
                    data.record(task_id, "system", "stage.checked", payload);
                    data.advance(task_id, next)?;
                }
            }
            StageAction::Reopen => {
                let to = input.to.clone().unwrap_or_else(|| stage.clone());
                let mut body = json!({ "from": stage, "to": to });
                if let Some(reason) = reason.filter(|reason| !reason.is_empty()) {
                    body["reason"] = json!(reason);
                }
                data.record(task_id, "human", "stage.reopened", body);
                data.advance(task_id, Some(to))?;
            }
            StageAction::Start => {
                data.record(task_id, "system", "stage.started", payload);
                data.task_mut(task_id)?.state = TaskState::Running;
            }
            StageAction::Fail => {
                let body = json!({ "stage": stage, "reason": reason.unwrap_or_default() });
                data.record(task_id, "agent", "stage.failed", body);
                data.task_mut(task_id)?.state = TaskState::Failed;
            }
        }
        let task = data.task_mut(task_id)?;
        task.updated_at = NOW.into();
        let task = task.clone();
        data.tasks_changed();
        Ok(task)
    }

    pub fn set_stage_toggle(
        &self,
        task_id: &str,
        kind: &str,
        stage: &str,
        on: bool,
    ) -> Answer<Task> {
        let mut data = self.data();
        let task = data.task_mut(task_id)?;
        let event = match kind {
            "workflow" => {
                set_member(&mut task.workflow_stages, stage, on);
                "stage.workflow-set"
            }
            "ci" => {
                set_member(task.ci_stages.get_or_insert_with(Vec::new), stage, on);
                "stage.ci-set"
            }
            "skip" => {
                set_member(task.skip_stages.get_or_insert_with(Vec::new), stage, on);
                "stage.skip-set"
            }
            other => return Err(status(400, &format!("No stage choice named {other}"))),
        };
        let task = task.clone();
        data.record(task_id, "human", event, json!({ "stage": stage, "on": on }));
        data.tasks_changed();
        Ok(task)
    }

    pub fn list_stage_files(&self, task_id: &str) -> Answer<Vec<StageFile>> {
        let data = self.data();
        if !data.tasks.iter().any(|task| task.id == task_id) {
            return Err(not_found("Task", task_id));
        }
        Ok(data.stage_files.get(task_id).cloned().unwrap_or_default())
    }

    pub fn update_task(&self, task_id: &str, changes: TaskChanges) -> Answer<Task> {
        let mut data = self.data();
        let task = data.task_mut(task_id)?;
        if let Some(title) = changes.title {
            task.title = title;
        }
        if let Some(description) = changes.description {
            task.description = description;
        }
        if let Some(priority) = changes.priority {
            task.priority = priority;
        }
        if let Some(labels) = changes.labels {
            task.labels = labels;
        }
        if let Some(depends_on) = changes.depends_on {
            task.depends_on = depends_on;
        }
        task.updated_at = NOW.into();
        let task = task.clone();
        data.record(task_id, "human", "task.edited", json!({}));
        data.tasks_changed();
        Ok(task)
    }

    pub fn save_template(&self, input: TemplateInput) -> Answer<TaskTemplate> {
        let mut data = self.data();
        let slug: String = input
            .name
            .to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect();
        let template = TaskTemplate {
            id: slug,
            name: input.name,
            title: input.title,
            description: input.description,
            priority: input.priority,
            labels: input.labels,
        };
        data.templates.retain(|each| each.id != template.id);
        data.templates.push(template.clone());
        Ok(template)
    }

    /// Copies of the task, each ready with no workspace. The group label ties them together.
    pub fn fan_out(&self, task_id: &str, input: FanOutInput) -> Answer<Vec<Task>> {
        let mut data = self.data();
        let source = data.task_mut(task_id)?.clone();
        let group = format!("tries-{}", task_id.to_lowercase());
        if !source.labels.contains(&group) {
            let task = data.task_mut(task_id)?;
            task.labels.push(group.clone());
        }
        let mut tries = data
            .tasks
            .iter()
            .filter(|t| t.labels.contains(&group))
            .count();
        let mut made = Vec::new();
        for _ in 0..input.count {
            tries += 1;
            let mut copy = data.new_task(
                &format!("{} (try {tries})", source.title),
                source.repo.clone(),
            );
            copy.description = source.description.clone();
            copy.priority = source.priority.clone();
            copy.labels = source.labels.clone();
            copy.labels.retain(|label| !label.starts_with("tries-"));
            copy.labels.push(group.clone());
            copy.stages = source.stages.clone();
            copy.agent = source.agent.clone();
            copy.state = TaskState::Ready;
            data.tasks.push(copy.clone());
            made.push(copy);
        }
        data.tasks_changed();
        Ok(made)
    }

    pub fn create_issue(&self, task_id: &str) -> Answer<Task> {
        let mut data = self.data();
        let task = data.task_mut(task_id)?;
        task.issue = Some(IssueLink {
            provider: "github".into(),
            key: "#12".into(),
            url: "https://github.com/lenhatthanh20/natamux/issues/12".into(),
        });
        let task = task.clone();
        data.record(task_id, "system", "issue.created", json!({ "key": "#12" }));
        data.tasks_changed();
        Ok(task)
    }

    pub fn refresh_pull_request(&self, task_id: &str) -> Answer<Task> {
        Ok(self.data().task_mut(task_id)?.clone())
    }
}

fn set_member(list: &mut Vec<String>, stage: &str, on: bool) {
    list.retain(|id| id != stage);
    if on {
        list.push(stage.into());
    }
}

#[cfg(test)]
#[path = "fake_stages_tests.rs"]
mod tests;
