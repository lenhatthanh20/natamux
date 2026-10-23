//! The state of the Stages tab of Project settings (`ProjectSettingsDialog.tsx`): the draft text of
//! `.natamux.json`, its steps and problems, and the picked stage or branch. The preview, the
//! stage list and the details read it; every edit goes through `edit`, which writes the steps
//! back into the draft and keeps every other key as it is.

use gpui::EventEmitter;
use natamux_proto::stages::pipeline_rules::{IssueLevel, PipelineIssue, pipeline_issues};
use natamux_proto::stages::schema::{Stage, StageBranch, Step};
use serde_json::{Map, Value};
use ui::prelude::*;

use super::editor_catalog::unique_issues;
use super::editor_draft::{Draft, format_settings, parse_draft, with_steps};
use super::editor_find::{find_branch, find_stage};

/// The draft changed: the dialog works out again whether it differs from the file.
pub struct DraftEdited;

pub struct PipelineEditor {
    text: String,
    draft: Draft,
    issues: Vec<PipelineIssue>,
    /// The stage or branch the user picked; None until then.
    picked: Option<String>,
    /// The stage picker takes the place of the details (Add stage).
    pub picking: bool,
}

impl EventEmitter<DraftEdited> for PipelineEditor {}

impl PipelineEditor {
    /// `text` is the file as read, or `{}`; a text the tab cannot read starts from `{}`.
    pub fn new(text: &str) -> Self {
        let (text, draft) = match parse_draft(text) {
            Ok(draft) => (text.to_owned(), draft),
            Err(_) => ("{}".to_owned(), empty_draft()),
        };
        let issues = issues_of(&draft.steps);
        PipelineEditor {
            text,
            draft,
            issues,
            picked: None,
            picking: false,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn raw(&self) -> &Map<String, Value> {
        &self.draft.raw
    }

    pub fn steps(&self) -> &[Step] {
        &self.draft.steps
    }

    pub fn issues(&self) -> &[PipelineIssue] {
        &self.issues
    }

    /// The problems shown next to `id`; None: the ones of the whole pipeline.
    pub fn issues_of(&self, id: Option<&str>) -> Vec<PipelineIssue> {
        self.issues
            .iter()
            .filter(|issue| issue.stage.as_deref() == id)
            .cloned()
            .collect()
    }

    pub fn errors(&self) -> usize {
        self.count(IssueLevel::Error)
    }

    pub fn warnings(&self) -> usize {
        self.count(IssueLevel::Warning)
    }

    fn count(&self, level: IssueLevel) -> usize {
        self.issues
            .iter()
            .filter(|issue| issue.level == level)
            .count()
    }

    /// New steps: the draft text takes them, the problems are worked out again.
    pub fn edit(&mut self, steps: Vec<Step>, cx: &mut Context<Self>) {
        let raw = with_steps(&self.draft.raw, &steps);
        self.text = format_settings(&raw);
        self.issues = issues_of(&steps);
        self.draft = Draft { raw, steps };
        cx.emit(DraftEdited);
        cx.notify();
    }

    /// A whole new text (Import, Reload). Refused with the reason when the tab cannot read it.
    pub fn replace_text(&mut self, text: &str, cx: &mut Context<Self>) -> Result<(), String> {
        let draft = parse_draft(text)?;
        self.issues = issues_of(&draft.steps);
        self.text = text.to_owned();
        self.draft = draft;
        cx.emit(DraftEdited);
        cx.notify();
        Ok(())
    }

    /// The picked branch, if a branch is picked.
    pub fn selected_branch(&self) -> Option<&StageBranch> {
        find_branch(&self.draft.steps, self.picked.as_deref()?)
    }

    /// The picked stage; the first stage until one is picked, and when the picked one is gone.
    pub fn selected_stage(&self) -> Option<&Stage> {
        if self.selected_branch().is_some() {
            return None;
        }
        self.picked
            .as_deref()
            .and_then(|id| find_stage(&self.draft.steps, id))
            .or_else(|| {
                self.draft
                    .steps
                    .first()
                    .and_then(|step| step.stages().into_iter().next())
            })
    }

    /// The id of the picked stage or branch, as the list and the preview mark it.
    pub fn selected_id(&self) -> Option<String> {
        self.selected_branch()
            .map(|branch| branch.id.clone())
            .or_else(|| self.selected_stage().map(|stage| stage.id.clone()))
    }

    pub fn select(&mut self, id: &str, cx: &mut Context<Self>) {
        self.picked = Some(id.to_owned());
        self.picking = false;
        cx.notify();
    }

    pub fn set_picking(&mut self, picking: bool, cx: &mut Context<Self>) {
        self.picking = picking;
        cx.notify();
    }
}

fn issues_of(steps: &[Step]) -> Vec<PipelineIssue> {
    unique_issues(&pipeline_issues(steps))
}

fn empty_draft() -> Draft {
    Draft {
        raw: Map::new(),
        steps: natamux_proto::stages::catalog::default_pipeline(),
    }
}
