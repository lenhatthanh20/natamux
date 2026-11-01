//! The Progress tab (docs/design-task-stages.md 7.5, `ProgressTab.tsx`, `BranchLane.tsx`): the
//! timeline of steps as a node flow (a stage, a group side by side, or a branch with its paths,
//! joined by connectors; T-0246), with the details card of the selected node right under its
//! row (T-0257).

use std::collections::{BTreeMap, BTreeSet};

use gpui::AnyElement;
use natamux_proto::agents::activity::TaskUsage;
use natamux_proto::events::task_event::{TaskEvent, WaitReason};
use natamux_proto::stages::actions::StageFile;
use natamux_proto::stages::progress::{StageProgress, StageStatus};
use natamux_proto::stages::schema::Step;
use natamux_proto::tasks::task::Task;
use ui::prelude::*;

use super::card_rules::{card_facts, node_agent};
use super::flow_rules::{CaretAt, NodeLinks};
use super::stage_card::{ActionsInput, CardInput, FileChip, details_card, stage_card};
use super::stage_choices::ChoiceBox;
use super::stage_view_rules::{find_stage, missing_skills, ran_with_workflow, waiting_for_ci};
use crate::agents::agent_choices::{custom_agents, workflow_agent};
use crate::tasks::details_commands::{OnCommand, StageToggle};
use crate::tasks::usage_rules::{stage_usage, usage_line};
use natamux_proto::stages::capabilities::{can_run_workflow, offers_ci, offers_workflow};

/// What the tab shows; the panel reads it.
pub struct ProgressInput {
    pub task: Task,
    pub steps: Vec<Step>,
    pub progress: Vec<StageProgress>,
    pub events: Vec<TaskEvent>,
    pub files: Option<Result<Vec<StageFile>, String>>,
    pub usage: Option<TaskUsage>,
    /// What the agent does now, from the activity feed.
    pub activity: Option<String>,
    pub wait: WaitReason,
    pub now_ms: i64,
    pub busy: bool,
    pub resuming: bool,
    pub toggling: BTreeSet<String>,
    /// The node the user opened (`card_rules::pick_after_click`): its details card shows under
    /// its step. None until a click, the running stage too.
    pub selected: Option<String>,
    /// The task's agent (`agent_of`), the icon of a node that runs it.
    pub task_agent: String,
    pub open_lanes: BTreeSet<String>,
    pub open_evidence: BTreeSet<String>,
    /// The focus of each node.
    pub card_focus: BTreeMap<String, gpui::FocusHandle>,
    pub agent_label: String,
    /// The task's workspace is open in main beside the panel: Answer in terminal can focus it.
    pub can_answer: bool,
    /// Not done or cancelled.
    pub can_act: bool,
}

/// The pieces of one step's row.
pub(super) struct Ctx<'a> {
    input: &'a ProgressInput,
    by_id: BTreeMap<&'a str, &'a StageProgress>,
    workflow_ran: Vec<String>,
    ci: Vec<(String, String)>,
    missing: Vec<(String, super::stage_view_rules::MissingSkills)>,
    on: &'a OnCommand,
}

impl Ctx<'_> {
    fn file_chip(&self, stage: &natamux_proto::stages::schema::Stage) -> Option<FileChip> {
        stage.output.as_ref().map(|name| match &self.input.files {
            None => FileChip::Loading,
            Some(Err(_)) => FileChip::Unreadable,
            Some(Ok(files)) => files
                .iter()
                .find(|file| &file.name == name)
                .cloned()
                .map_or(FileChip::Missing, FileChip::Written),
        })
    }

    /// The workflow and CI boxes of a stage that is not finished, while the task is open.
    fn boxes(
        &self,
        entry: &StageProgress,
        stage: &natamux_proto::stages::schema::Stage,
    ) -> Vec<ChoiceBox> {
        let input = self.input;
        let task = &input.task;
        let id = stage.id.as_str();
        let finished = matches!(entry.status, StageStatus::Done | StageStatus::Skipped);
        let started = entry.status == StageStatus::Current
            && !matches!(
                entry.state,
                Some(
                    natamux_proto::tasks::task::TaskState::Draft
                        | natamux_proto::tasks::task::TaskState::Ready
                )
            );
        let mut boxes = Vec::new();
        if !finished && input.can_act {
            let toggling = |kind: StageToggle| input.toggling.contains(&format!("{kind:?}:{id}"));
            if offers_workflow(stage) {
                boxes.push(ChoiceBox {
                    kind: StageToggle::Workflow,
                    stage: id.to_owned(),
                    on: task.workflow_stages.iter().any(|each| each == id),
                    busy: toggling(StageToggle::Workflow),
                    // A stage that starts its own agent runs the workflow with that agent.
                    allowed: {
                        let custom = custom_agents();
                        let agent = stage.agent.as_deref().or(task.agent.as_deref());
                        can_run_workflow(agent.map(|id| workflow_agent(id, &custom)))
                    },
                    started,
                });
            }
            if offers_ci(stage) {
                boxes.push(ChoiceBox {
                    kind: StageToggle::Ci,
                    stage: id.to_owned(),
                    on: task
                        .ci_stages
                        .as_ref()
                        .is_some_and(|list| list.iter().any(|each| each == id)),
                    busy: toggling(StageToggle::Ci),
                    allowed: true,
                    started,
                });
            }
        }
        boxes
    }

    /// `links`: the lines inside the node.
    fn card(&self, id: &str, links: NodeLinks, cx: &App) -> Option<AnyElement> {
        let input = self.card_input(id, links)?;
        Some(stage_card(input, self.on, cx))
    }

    /// The full-width details card of the selected stage when `stages` hold it, its caret
    /// pointing `at` the node.
    fn details_of<'s>(
        &self,
        stages: impl IntoIterator<Item = &'s natamux_proto::stages::schema::Stage>,
        at: CaretAt,
        cx: &App,
    ) -> Option<AnyElement> {
        let selected = self.input.selected.as_deref()?;
        let stage = stages.into_iter().find(|stage| stage.id == selected)?;
        let input = self.card_input(&stage.id, NodeLinks::default())?;
        Some(details_card(input, at, self.on, cx))
    }

    fn card_input(&self, id: &str, links: NodeLinks) -> Option<CardInput> {
        let entry = (*self.by_id.get(id)?).clone();
        let stage = find_stage(&self.input.steps, id)?.clone();
        let input = self.input;
        let task = &input.task;
        let current = entry.status == StageStatus::Current;
        let upcoming = entry.status == StageStatus::Upcoming;
        let file = self.file_chip(&stage);
        let boxes = self.boxes(&entry, &stage);
        let marked = task
            .skip_stages
            .as_ref()
            .is_some_and(|list| list.iter().any(|each| each == id));
        let skippable = upcoming
            && input.can_act
            && stage.kind != natamux_proto::stages::schema::StageKind::Context;
        let missing = self
            .missing
            .iter()
            .find(|(stage, _)| stage == id)
            .map(|(_, missing)| missing.clone());
        let release = stage.kind == natamux_proto::stages::schema::StageKind::Release;
        let facts = card_facts(
            &entry,
            stage.kind,
            upcoming && marked,
            &input.wait,
            input.can_answer,
            input.can_act,
        );
        let done = task.state == natamux_proto::tasks::task::TaskState::Done;
        let card = CardInput {
            links,
            usage: stage_usage(input.usage.as_ref(), id)
                .map(|usage| usage_line(usage.tokens, usage.cost_usd)),
            activity: (current
                && entry.state == Some(natamux_proto::tasks::task::TaskState::Running)
                && stage.kind == natamux_proto::stages::schema::StageKind::Agent
                && stage.output.is_none())
            .then(|| input.activity.clone())
            .flatten(),
            ci_branch: self
                .ci
                .iter()
                .find(|(stage, _)| stage == id)
                .map(|(_, branch)| branch.clone()),
            ran_workflow: self.workflow_ran.iter().any(|each| each == id),
            missing_skills: missing
                .as_ref()
                .map(|each| each.names.clone())
                .unwrap_or_default(),
            skills_agent: crate::agents::agent_choices::agent_label(
                missing
                    .as_ref()
                    .and_then(|each| each.agent.as_deref())
                    .or(task.agent.as_deref())
                    .or(Some(crate::agents::agent_choices::DEFAULT_AGENT)),
            ),
            facts,
            evidence_open: input.open_evidence.contains(id),
            fold_focus: input.card_focus.get(id).cloned(),
            boxes,
            skip: skippable.then(|| (marked, input.toggling.contains(&format!("Skip:{id}")))),
            actions: (current && input.can_act).then(|| ActionsInput {
                busy: input.busy,
                resuming: input.resuming,
                agent_label: input.agent_label.clone(),
            }),
            // A done task's pull request was merged: Release finishes when it is.
            pr: (release && task.pr.is_some()).then(|| (task.pr.clone().unwrap_or_default(), done)),
            selected: input.selected.as_deref() == Some(id),
            node_agent: node_agent(&stage, &input.task_agent),
            now_ms: input.now_ms,
            file,
            entry,
            stage,
        };
        Some(card)
    }
}

impl Ctx<'_> {
    /// The progress of the stages a step holds.
    fn entries(&self, step: &Step) -> Vec<&StageProgress> {
        step.stages()
            .iter()
            .filter_map(|stage| self.by_id.get(stage.id.as_str()).copied())
            .collect()
    }
}

#[path = "progress_flow.rs"]
mod flow;

#[path = "progress_branch.rs"]
mod branches;

#[path = "progress_steps.rs"]
mod steps;

/// The steps of the timeline, top to bottom, one element each so the panel can scroll to one.
pub fn timeline(input: &ProgressInput, on: &OnCommand, cx: &App) -> Vec<AnyElement> {
    let ctx = Ctx {
        input,
        by_id: input
            .progress
            .iter()
            .map(|entry| (entry.stage.as_str(), entry))
            .collect(),
        workflow_ran: ran_with_workflow(&input.events),
        ci: waiting_for_ci(&input.events),
        missing: missing_skills(&input.events),
        on,
    };
    let last = input.steps.len().saturating_sub(1);
    ctx.column(&input.steps, false, cx)
        .into_iter()
        .enumerate()
        .map(|(index, step)| {
            // One element per step so the panel can scroll to one; the panel draws the dot grid.
            div()
                .px_3()
                .when(index == last, |this| this.pb_3())
                .child(step)
                .into_any_element()
        })
        .collect()
}

/// The step that holds the current stage, to bring into view when the tab opens.
pub fn current_step(input: &ProgressInput) -> Option<usize> {
    input.steps.iter().position(|step| {
        step.stages().iter().any(|stage| {
            input
                .progress
                .iter()
                .any(|entry| entry.stage == stage.id && entry.status == StageStatus::Current)
        })
    })
}

#[cfg(test)]
#[path = "progress_tab_tests.rs"]
mod tests;
