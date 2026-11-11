//! The stage sheet (T-0313, mockup B "Prompt first"): a sheet that fills the Settings popup and
//! edits one stage of the draft. The header has the kind tile, the name, the first line and the
//! stepper to the stage before and after; the body the prompt (or the run fields) at the left and
//! the 296 px settings column at the right; the footer Remove and Done. Every edit writes the
//! draft at once.

use gpui::{
    AnyElement, ClickEvent, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable,
    ScrollHandle, Subscription,
};
use natamux_proto::projects::repo::SkillList;
use natamux_proto::stages::capabilities::stage_name;
use natamux_proto::stages::pipeline_rules::{IssueLevel, PipelineIssue};
use natamux_proto::stages::schema::StageKind;
use ui::{ContextMenu, prelude::*};

use super::advanced_rules::{SheetGroup, group_has_issue};
use super::editor_find::find_stage;
use super::editor_moves::remove_stage;
use super::list_rules::{kind_tile, kind_tile_icon};
use super::pipeline_editor::PipelineEditor;
use super::sheet_rules::sheet_intro;
use super::stage_details_fields::StageFields;
use super::stage_fields_view::change;
use crate::components::button_style::{button_icon, quiet_button};
use crate::components::confirm::Confirm;
use crate::components::dialog::{DialogFrame, DialogLayer, DialogSize, DialogView, primary_button};
use crate::components::settings_tile::{AlertKind, SettingsAlert, SettingsTile, TileSize};

pub type Skills = Option<Result<SkillList, String>>;

/// Opens the sheet of stage `id` with the focus in Name.
pub fn open(
    pipeline: &Entity<PipelineEditor>,
    id: &str,
    skills: Skills,
    window: &mut Window,
    cx: &mut App,
) -> Entity<StageSheet> {
    let pipeline = pipeline.clone();
    let sheet = cx.new(|cx| StageSheet::new(pipeline, id, skills, window, cx));
    let name = sheet.read(cx).fields.name.editor.focus_handle(cx);
    DialogLayer::open(sheet.clone(), window, cx);
    window.focus(&name, cx);
    sheet
}

/// Which folding groups are open when a stage shows: Done check, and any group with a problem.
fn groups_open_at_first(pipeline: &Entity<PipelineEditor>, id: &str, cx: &App) -> [bool; 3] {
    let editor = pipeline.read(cx);
    let issues = editor.issues_of(Some(id));
    let stage = find_stage(editor.steps(), id);
    SheetGroup::ALL.map(|group| {
        group.open_at_first() || stage.is_some_and(|stage| group_has_issue(stage, &issues, group))
    })
}

pub struct StageSheet {
    pub(super) pipeline: Entity<PipelineEditor>,
    pub(super) id: String,
    pub(super) fields: StageFields,
    pub(super) skills: Skills,
    pub(super) skill_menu: Option<(Entity<ContextMenu>, Subscription)>,
    /// Open or folded, by `SheetGroup::index`.
    pub(super) open: [bool; 3],
    pub(super) fold_focus: [FocusHandle; 3],
    pub(super) prev_focus: FocusHandle,
    pub(super) next_focus: FocusHandle,
    pub(super) column_scroll: ScrollHandle,
    pub(super) left_scroll: ScrollHandle,
    focus: FocusHandle,
    remove_focus: FocusHandle,
    done_focus: FocusHandle,
    _watch: Subscription,
}

impl StageSheet {
    fn new(
        pipeline: Entity<PipelineEditor>,
        id: &str,
        skills: Skills,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let fields = StageFields::new(&pipeline, id, window, cx);
        let watch = cx.observe_in(&pipeline, window, |this, pipeline, window, cx| {
            let steps = pipeline.read(cx).steps().to_vec();
            // Removed, or gone with an Import or Reset: there is nothing left to edit.
            if find_stage(&steps, &this.id).is_none() {
                cx.emit(DismissEvent);
                return;
            }
            this.fields.sync(&steps, window, cx);
            cx.notify();
        });
        let open = groups_open_at_first(&pipeline, id, cx);
        StageSheet {
            pipeline,
            id: id.to_owned(),
            fields,
            skills,
            skill_menu: None,
            open,
            fold_focus: [cx.focus_handle(), cx.focus_handle(), cx.focus_handle()],
            prev_focus: cx.focus_handle(),
            next_focus: cx.focus_handle(),
            column_scroll: ScrollHandle::new(),
            left_scroll: ScrollHandle::new(),
            focus: cx.focus_handle(),
            remove_focus: cx.focus_handle(),
            done_focus: cx.focus_handle(),
            _watch: watch,
        }
    }

    pub fn set_skills(&mut self, skills: Result<SkillList, String>, cx: &mut Context<Self>) {
        self.skills = Some(skills);
        cx.notify();
    }

    /// The stepper: the sheet edits stage `id` now. Its fields are made again, since each one
    /// reads and writes its stage by id; edits of the stage before stay in the draft.
    pub(super) fn go_to(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if find_stage(self.pipeline.read(cx).steps(), id).is_none() {
            return;
        }
        self.close_skills(cx);
        self.id = id.to_owned();
        self.fields = StageFields::new(&self.pipeline, id, window, cx);
        self.open = groups_open_at_first(&self.pipeline, id, cx);
        self.column_scroll = ScrollHandle::new();
        self.left_scroll = ScrollHandle::new();
        let name = self.fields.name.editor.focus_handle(cx);
        window.focus(&name, cx);
        cx.notify();
    }

    pub(super) fn toggle_group(&mut self, group: SheetGroup, cx: &mut Context<Self>) {
        let open = &mut self.open[group.index()];
        *open = !*open;
        cx.notify();
    }

    fn remove_ask(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(stage) = find_stage(self.pipeline.read(cx).steps(), &self.id) else {
            return;
        };
        let (pipeline, id) = (self.pipeline.clone(), self.id.clone());
        Confirm::new(
            format!("Remove {}?", stage_name(stage)),
            "Its prompt and settings are lost.",
            "Remove",
            move |_, cx| change(&pipeline, cx, |steps| remove_stage(steps, &id)),
        )
        .sheet()
        .open_over(window, cx);
    }

    fn remove_button(&self, cx: &mut Context<Self>) -> Button {
        quiet_button("stage-remove", "Remove")
            .start_icon(button_icon("trash").color(Color::Error))
            .color(Color::Error)
            .track_focus(&self.remove_focus)
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.remove_ask(window, cx)))
    }
}

impl EventEmitter<DismissEvent> for StageSheet {}

impl Focusable for StageSheet {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl DialogView for StageSheet {
    fn size(&self) -> DialogSize {
        DialogSize::Fill
    }

    fn tab_ring(&self, cx: &App) -> Vec<FocusHandle> {
        let mut ring = self.stepper_stops(cx);
        ring.extend(self.field_stops(cx));
        let steps = self.pipeline.read(cx).steps();
        let context = find_stage(steps, &self.id)
            .is_some_and(|stage| stage.kind == StageKind::Context);
        if !context {
            ring.push(self.remove_focus.clone());
        }
        ring.push(self.done_focus.clone());
        ring
    }

    /// Mod+Enter is Done: the edits are in the draft already.
    fn submit(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }

    /// Esc closes the skill menu first.
    fn cancel(&mut self, _: &mut Window, cx: &mut Context<Self>) -> bool {
        self.close_skills(cx)
    }
}

/// The alerts of the stage's issues, at the top of the left pane.
fn alerts(issues: Vec<PipelineIssue>) -> Vec<AnyElement> {
    issues
        .into_iter()
        .map(|issue| {
            let kind = match issue.level {
                IssueLevel::Error => AlertKind::Error,
                IssueLevel::Warning => AlertKind::Warning,
            };
            SettingsAlert::new(kind, issue.message).into_any_element()
        })
        .collect()
}

impl Render for StageSheet {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (stage, steps, issues) = {
            let editor = self.pipeline.read(cx);
            (
                find_stage(editor.steps(), &self.id).cloned(),
                editor.steps().to_vec(),
                editor.issues_of(Some(self.id.as_str())),
            )
        };
        let Some(stage) = stage else {
            return div().track_focus(&self.focus).into_any_element();
        };
        let tile = SettingsTile::icon(
            button_icon(kind_tile_icon(stage.kind)),
            kind_tile(&stage),
            TileSize::Row,
        );
        let left = self.left_pane(&stage, &steps, alerts(issues), cx);
        let column = self.right_column(&stage, &steps, window, cx);
        let done = primary_button(
            "Done",
            &self.done_focus,
            false,
            cx.listener(|_, _, _, cx| cx.emit(DismissEvent)),
        );
        let frame = DialogFrame::new(
            stage_name(&stage),
            cx.listener(|_, _, _, cx| cx.emit(DismissEvent)),
        )
        .fill()
        .lead(tile)
        .intro(sheet_intro(&stage))
        .header_end(self.stepper(&steps, cx));
        // The context stage opens every pipeline, so it cannot be removed (T-0333).
        let frame = if stage.kind == StageKind::Context {
            frame
        } else {
            frame.footer_start(self.remove_button(cx))
        };
        let frame = frame
            .footer_note("Edits go to the draft. Save on the page.")
            .footer(done)
            .child(left)
            .child(column);
        div()
            .key_context("Dialog")
            .track_focus(&self.focus)
            .size_full()
            .child(frame)
            .into_any_element()
    }
}

#[cfg(test)]
#[path = "stage_sheet_tests.rs"]
mod tests;
