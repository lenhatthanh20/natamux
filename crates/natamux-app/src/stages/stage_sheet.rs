//! The stage sheet (layout-b 5.3): a 640 px Settings sheet that edits one stage of the draft.
//! The header has the kind tile and Basic / Advanced; the body the groups of the tab, the footer
//! Remove and Done. Every edit writes the draft at once, as the fields did in the details pane.

use gpui::{
    AnyElement, ClickEvent, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable,
    Subscription,
};
use natamux_proto::projects::repo::SkillList;
use natamux_proto::stages::capabilities::stage_name;
use natamux_proto::stages::pipeline_rules::IssueLevel;
use natamux_proto::stages::schema::{Stage, Step};
use ui::{ContextMenu, prelude::*};

use super::advanced_rules::{advanced_has_issue, changed_count};
use super::editor_find::find_stage;
use super::editor_moves::remove_stage;
use super::list_rules::{kind_tile, kind_tile_icon};
use super::pipeline_editor::PipelineEditor;
use super::sheet_rules::{advanced_label, sheet_intro};
use super::stage_details_fields::StageFields;
use super::stage_fields_view::change;
use crate::components::button_style::{button_icon, quiet_button};
use crate::components::confirm::Confirm;
use crate::components::dialog::{DialogFrame, DialogLayer, DialogSize, DialogView, primary_button};
use crate::components::segmented::Segmented;
use crate::components::settings_card::foot_text;
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

pub struct StageSheet {
    pub(super) pipeline: Entity<PipelineEditor>,
    pub(super) id: String,
    pub(super) fields: StageFields,
    pub(super) skills: Skills,
    pub(super) skill_menu: Option<(Entity<ContextMenu>, Subscription)>,
    advanced: bool,
    tabs_focus: FocusHandle,
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
        // A stage with a problem in an Advanced field opens there.
        let advanced = {
            let editor = pipeline.read(cx);
            find_stage(editor.steps(), id)
                .is_some_and(|stage| advanced_has_issue(stage, &editor.issues_of(Some(id))))
        };
        StageSheet {
            pipeline,
            id: id.to_owned(),
            fields,
            skills,
            skill_menu: None,
            advanced,
            tabs_focus: cx.focus_handle().tab_stop(true),
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

    /// The groups of the open tab.
    fn groups(&mut self, stage: &Stage, steps: &[Step], cx: &mut Context<Self>) -> Vec<AnyElement> {
        if self.advanced {
            self.advanced_groups(stage, steps, cx)
        } else {
            self.basic_groups(stage, steps, cx)
        }
    }

    fn tabs(&self, stage: &Stage, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let this = cx.weak_entity();
        let options = [
            (SharedString::from("Basic"), IconName::Settings),
            (
                advanced_label(changed_count(stage)).into(),
                IconName::ListTree,
            ),
        ];
        let tabs = Segmented::new(
            "stage-sheet-tabs",
            &self.tabs_focus,
            options,
            usize::from(self.advanced),
            move |index, _, cx| {
                this.update(cx, |this, cx| {
                    this.advanced = index == 1;
                    cx.notify();
                })
                .ok();
            },
        );
        div().mr(rems_from_px(4.0_f32)).child(tabs)
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
        DialogSize::Large
    }

    fn tab_ring(&self, cx: &App) -> Vec<FocusHandle> {
        let mut ring = vec![self.tabs_focus.clone()];
        ring.extend(self.field_stops(self.advanced, cx));
        ring.extend([self.remove_focus.clone(), self.done_focus.clone()]);
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

impl Render for StageSheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
        let groups = self.groups(&stage, &steps, cx);
        let alerts = issues.into_iter().map(|issue| {
            let kind = match issue.level {
                IssueLevel::Error => AlertKind::Error,
                IssueLevel::Warning => AlertKind::Warning,
            };
            SettingsAlert::new(kind, issue.message)
        });
        let tile = SettingsTile::icon(
            button_icon(kind_tile_icon(stage.kind)),
            kind_tile(&stage),
            TileSize::Row,
        );
        let remove = quiet_button("stage-remove", "Remove")
            .start_icon(button_icon("trash").color(Color::Error))
            .color(Color::Error)
            .track_focus(&self.remove_focus)
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.remove_ask(window, cx)));
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
        .sheet()
        .lead(tile)
        .header_end(self.tabs(&stage, cx))
        .footer_note("Edits go to the draft. Save on the page.")
        .footer(remove)
        .footer(done)
        .child(
            div()
                .pt(rems_from_px(12.0_f32))
                .child(foot_text(sheet_intro(&stage))),
        )
        .children(alerts)
        .children(groups);
        div()
            .key_context("Dialog")
            .track_focus(&self.focus)
            .child(frame)
            .into_any_element()
    }
}
