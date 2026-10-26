//! The draft of Project settings (`ProjectSettingsDialog.tsx`): the text of `.natamux.json` (which
//! may not parse), its steps and problems, and the picked stage or branch. The preview, the
//! stage list and the details read it; every edit goes through `edit`, which writes the steps
//! back into the draft and keeps every other key as it is.

use gpui::EventEmitter;
use natamux_proto::projects::repo::TrackerSettings;
use natamux_proto::stages::pipeline_rules::{IssueLevel, PipelineIssue, pipeline_issues};
use natamux_proto::stages::schema::{Stage, StageBranch, Step};
use serde_json::{Map, Value};
use ui::prelude::*;

use super::editor_catalog::unique_issues;
use super::editor_draft::{
    Draft, format_settings, max_send_backs_of, parse_draft, tracker_of, with_create_issue,
    with_max_send_backs, with_steps,
};
use super::editor_find::{find_branch, find_stage};

/// The draft changed: the dialog works out again whether it differs from the file.
pub struct DraftEdited;

pub struct PipelineEditor {
    text: String,
    /// The settings of `text`; the default pipeline with no other keys while `error` is set.
    draft: Draft,
    /// Why `text` cannot be read as settings. The person fixes it on the JSON tab.
    error: Option<String>,
    issues: Vec<PipelineIssue>,
    /// The stage or branch the user picked; None until then.
    picked: Option<String>,
    /// The stage picker takes the place of the details (Add stage).
    pub picking: bool,
}

impl EventEmitter<DraftEdited> for PipelineEditor {}

impl PipelineEditor {
    /// `text` is the file as read, or `{}`. A text the tab cannot read is kept as it is.
    pub fn new(text: &str) -> Self {
        let mut editor = PipelineEditor {
            text: String::new(),
            draft: empty_draft(),
            error: None,
            issues: Vec::new(),
            picked: None,
            picking: false,
        };
        editor.take_text(text);
        editor
    }

    fn take_text(&mut self, text: &str) {
        self.text = text.to_owned();
        match parse_draft(text) {
            Ok(draft) => {
                self.issues = issues_of(&draft.steps);
                self.draft = draft;
                self.error = None;
            }
            Err(error) => {
                self.issues = Vec::new();
                self.draft = empty_draft();
                self.error = Some(error);
            }
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// The settings of the draft, or None while the text cannot be read.
    pub fn raw(&self) -> Option<&Map<String, Value>> {
        self.error.is_none().then_some(&self.draft.raw)
    }

    /// Why the text cannot be read; None while it can.
    pub fn parse_error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// The tracker of the draft; the defaults while the text cannot be read.
    pub fn tracker(&self) -> TrackerSettings {
        self.raw().map(tracker_of).unwrap_or_default()
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

    /// New steps: the draft text takes them, the problems are worked out again. Ignored while the
    /// text cannot be read.
    pub fn edit(&mut self, steps: Vec<Step>, cx: &mut Context<Self>) {
        if self.error.is_some() {
            return;
        }
        let raw = with_steps(&self.draft.raw, &steps);
        self.changed(&format_settings(&raw), cx);
    }

    /// `tracker.createIssue`. Ignored while the text cannot be read.
    pub fn set_create_issue(&mut self, value: &str, cx: &mut Context<Self>) {
        if self.error.is_some() {
            return;
        }
        let raw = with_create_issue(&self.draft.raw, value);
        self.changed(&format_settings(&raw), cx);
    }

    /// `agents.maxSendBacks` of the draft; None (no limit) while the text cannot be read.
    pub fn max_send_backs(&self) -> Option<usize> {
        self.raw().and_then(max_send_backs_of)
    }

    /// `agents.maxSendBacks`, None for no limit. Ignored while the text cannot be read.
    pub fn set_max_send_backs(&mut self, most: Option<usize>, cx: &mut Context<Self>) {
        if self.error.is_some() {
            return;
        }
        let raw = with_max_send_backs(&self.draft.raw, most);
        self.changed(&format_settings(&raw), cx);
    }

    /// A whole new text (the JSON tab, Import, Reset, Reload). Any text is kept; one that cannot
    /// be read shows its error on the JSON tab.
    pub fn set_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.changed(text, cx);
    }

    fn changed(&mut self, text: &str, cx: &mut Context<Self>) {
        if text == self.text {
            return;
        }
        self.take_text(text);
        cx.emit(DraftEdited);
        cx.notify();
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
