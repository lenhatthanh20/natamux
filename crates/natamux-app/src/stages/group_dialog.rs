//! The Side by side dialog, "Run stages side by side" (docs/ui-spec.md 4.31): an optional group
//! name and a checklist of the agent and manual stages that are not in a group. 2 to 4 stages of
//! one place (the top level or one path) make a group. Opened on a group (Edit group…), it shows
//! the group's stages checked and offers Split group.

use std::collections::HashMap;

use editor::Editor;
use gpui::{AnyElement, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable};
use natamux_proto::stages::capabilities::stage_name;
use natamux_proto::stages::schema::{Stage, StageGroup, Step};
use ui::{ButtonLike, Checkbox, Tooltip, prelude::*};

use super::editor_catalog::is_custom;
use super::editor_draft::MAX_GROUP;
use super::editor_find::{find_place, group_of, item_at};
use super::editor_labels::join_names;
use super::group_rules::{GroupPlace, edit_group, group_choices, make_group, split_group};
use super::kind_icon::kind_icon;
use super::pipeline_editor::PipelineEditor;
use super::source_tag::source_tag;
use super::stage_fields_view::{field_label, help_line};
use crate::components::button_style::quiet_button;
use crate::components::dialog::{
    DialogFrame, DialogLayer, DialogView, cancel_button, primary_button,
};
use crate::components::form::{field, input_box, text_editor};

const OTHER_PLACE: &str = "A group runs in one place: the top level or one path";
const KEEP_TWO: &str = "A group keeps 2 stages. Split group runs each on its own";

/// A new group, from the `+ Add` menu.
pub fn open_new(pipeline: &Entity<PipelineEditor>, window: &mut Window, cx: &mut App) {
    show(pipeline, None, window, cx);
}

/// The group `id`, from its "…" menu.
pub fn open_edit(pipeline: &Entity<PipelineEditor>, id: &str, window: &mut Window, cx: &mut App) {
    show(pipeline, Some(id.to_owned()), window, cx);
}

fn show(
    pipeline: &Entity<PipelineEditor>,
    editing: Option<String>,
    window: &mut Window,
    cx: &mut App,
) {
    let steps = pipeline.read(cx).steps().to_vec();
    let group = editing
        .as_deref()
        .and_then(|id| find_group(&steps, id))
        .cloned();
    if editing.is_some() && group.is_none() {
        return;
    }
    let picked: Vec<String> = group
        .as_ref()
        .map(|group| {
            group
                .parallel
                .iter()
                .map(|stage| stage.id.clone())
                .collect()
        })
        .unwrap_or_default();
    let name_text = group.and_then(|group| group.name).unwrap_or_default();
    let pipeline = pipeline.clone();
    let dialog = cx.new(|cx| {
        let name = text_editor("", None, window, cx);
        name.update(cx, |editor, cx| editor.set_text(name_text, window, cx));
        let dialog = GroupDialog {
            pipeline,
            editing,
            name,
            picked,
            rows: HashMap::new(),
            focus: cx.focus_handle(),
            split_focus: cx.focus_handle(),
            cancel_focus: cx.focus_handle(),
            submit_focus: cx.focus_handle(),
        };
        dialog.name_hint(window, cx);
        dialog
    });
    let first = dialog.read(cx).name.focus_handle(cx);
    DialogLayer::open(dialog, window, cx);
    window.focus(&first, cx);
}

/// A group at the top level or in a path.
fn find_group<'a>(steps: &'a [Step], id: &str) -> Option<&'a StageGroup> {
    match item_at(steps, find_place(steps, id)?) {
        Some(Step::Group(group)) => Some(group),
        _ => None,
    }
}

pub struct GroupDialog {
    pipeline: Entity<PipelineEditor>,
    /// The group being edited; None for a new one.
    editing: Option<String>,
    name: Entity<Editor>,
    /// The checked stage ids, in the order they were checked.
    picked: Vec<String>,
    /// One focus stop per checklist row, by stage id.
    rows: HashMap<String, FocusHandle>,
    focus: FocusHandle,
    split_focus: FocusHandle,
    cancel_focus: FocusHandle,
    submit_focus: FocusHandle,
}

impl GroupDialog {
    fn choices(&self, cx: &App) -> Vec<(GroupPlace, Stage)> {
        group_choices(self.pipeline.read(cx).steps(), self.editing.as_deref())
    }

    /// The place of the checked stages, once one is checked.
    fn picked_place(&self, choices: &[(GroupPlace, Stage)]) -> Option<GroupPlace> {
        let first = self.picked.first()?;
        choices
            .iter()
            .find(|(_, stage)| &stage.id == first)
            .map(|(place, _)| place.clone())
    }

    /// The name field shows the checked stages' names, as the list names a group without one.
    fn name_hint(&self, window: &mut Window, cx: &mut Context<Self>) {
        let names: Vec<String> = self
            .choices(cx)
            .iter()
            .filter(|(_, stage)| self.picked.contains(&stage.id))
            .map(|(_, stage)| stage_name(stage))
            .collect();
        let hint = if names.is_empty() {
            "Optional".to_owned()
        } else {
            join_names(&names)
        };
        self.name.update(cx, |editor, cx| {
            editor.set_placeholder_text(&hint, window, cx)
        });
    }

    fn toggle(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(at) = self.picked.iter().position(|each| each == id) {
            self.picked.remove(at);
        } else {
            self.picked.push(id.to_owned());
        }
        self.name_hint(window, cx);
        cx.notify();
    }

    fn row_focus(&mut self, id: &str, cx: &mut Context<Self>) -> FocusHandle {
        self.rows
            .entry(id.to_owned())
            .or_insert_with(|| cx.focus_handle())
            .clone()
    }

    /// Why the row of `stage` cannot change now, or None when it can.
    fn row_block(
        &self,
        stage: &Stage,
        place: &GroupPlace,
        picked_place: Option<&GroupPlace>,
    ) -> Option<&'static str> {
        let checked = self.picked.contains(&stage.id);
        if checked {
            return (self.editing.is_some() && self.picked.len() <= 2).then_some(KEEP_TWO);
        }
        if picked_place.is_some_and(|picked| picked != place) {
            return Some(OTHER_PLACE);
        }
        (self.picked.len() >= MAX_GROUP).then_some("A group holds 4 stages at most")
    }

    fn split(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.editing.clone() else {
            return;
        };
        self.pipeline.update(cx, |pipeline, cx| {
            let steps = split_group(pipeline.steps(), &id);
            pipeline.edit(steps, cx);
        });
        cx.emit(DismissEvent);
    }
}

impl EventEmitter<DismissEvent> for GroupDialog {}

impl Focusable for GroupDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl DialogView for GroupDialog {
    fn tab_ring(&self, cx: &App) -> Vec<FocusHandle> {
        let mut ring = vec![self.name.focus_handle(cx)];
        ring.extend(
            self.choices(cx)
                .iter()
                .filter_map(|(_, stage)| self.rows.get(&stage.id).cloned()),
        );
        if self.editing.is_some() {
            ring.push(self.split_focus.clone());
        }
        ring.extend([self.cancel_focus.clone(), self.submit_focus.clone()]);
        ring
    }

    fn submit(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.picked.len() < 2 {
            return;
        }
        let (name, picked) = (self.name.read(cx).text(cx), self.picked.clone());
        let editing = self.editing.clone();
        self.pipeline.update(cx, |pipeline, cx| {
            let steps = match &editing {
                Some(id) => edit_group(pipeline.steps(), id, &picked, &name),
                None => make_group(pipeline.steps(), &picked, &name),
            };
            if steps == pipeline.steps() {
                return;
            }
            pipeline.edit(steps, cx);
            // A group has no details of its own: its first stage is picked.
            let first = picked
                .iter()
                .find_map(|id| group_of(pipeline.steps(), id))
                .and_then(|group| group.parallel.first())
                .map(|stage| stage.id.clone());
            if let Some(first) = first {
                pipeline.select(&first, cx);
            }
        });
        cx.emit(DismissEvent);
    }
}

impl GroupDialog {
    fn render_row(
        &mut self,
        stage: &Stage,
        block: Option<&'static str>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = stage.id.clone();
        let focus = self.row_focus(&id, cx);
        let checked = self.picked.contains(&id);
        let (toggle_id, box_id) = (id.clone(), SharedString::from(format!("group-box-{id}")));
        let mut row = ButtonLike::new(SharedString::from(format!("group-pick-{id}")))
            .full_width()
            .disabled(block.is_some())
            .track_focus(&focus)
            .on_click(cx.listener(move |this, _, window, cx| this.toggle(&toggle_id, window, cx)))
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap(rems_from_px(10.0_f32))
                    .px(rems_from_px(8.0_f32))
                    .py(rems_from_px(5.0_f32))
                    .child(
                        Checkbox::new(box_id, checked.into())
                            .disabled(block.is_some())
                            .visualization_only(true),
                    )
                    .child(
                        kind_icon(stage.kind)
                            .size(IconSize::Custom(rems_from_px(15.0_f32)))
                            .color(if is_custom(stage) {
                                Color::Accent
                            } else {
                                Color::Muted
                            }),
                    )
                    .child(
                        div().min_w_0().flex_1().child(
                            Label::new(stage_name(stage))
                                .weight(gpui::FontWeight::MEDIUM)
                                .truncate(),
                        ),
                    )
                    .child(source_tag(is_custom(stage), cx)),
            );
        if let Some(block) = block {
            row = row.tooltip(Tooltip::text(block));
        }
        row.into_any_element()
    }

    /// The checklist: the top-level stages, then the stages of each path under its name.
    fn render_list(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let choices = self.choices(cx);
        let picked_place = self.picked_place(&choices);
        let mut rows = Vec::new();
        let mut last: Option<GroupPlace> = None;
        for (place, stage) in &choices {
            if last.as_ref() != Some(place) {
                if let GroupPlace::Path { name, .. } = place {
                    rows.push(
                        div()
                            .mx(rems_from_px(8.0_f32))
                            .mt(rems_from_px(10.0_f32))
                            .mb(rems_from_px(2.0_f32))
                            .child(
                                Label::new(name.to_uppercase())
                                    .size(LabelSize::Custom(rems_from_px(10.5_f32)))
                                    .weight(gpui::FontWeight::MEDIUM)
                                    .color(Color::Placeholder),
                            )
                            .into_any_element(),
                    );
                }
                last = Some(place.clone());
            }
            let block = self.row_block(stage, place, picked_place.as_ref());
            rows.push(self.render_row(stage, block, cx));
        }
        if rows.is_empty() {
            let empty = help_line("No agent or manual stage is free to run side by side.");
            rows.push(empty.into_any_element());
        }
        rows
    }
}

impl Render for GroupDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = self.render_list(cx);
        let count = self.picked.len();
        let editing = self.editing.is_some();
        let mut frame = DialogFrame::new(
            "Run stages side by side",
            cx.listener(|_, _, _, cx| cx.emit(DismissEvent)),
        );
        if editing {
            frame = frame.footer_start(
                quiet_button("group-split", "Split group")
                    .color(Color::Error)
                    .track_focus(&self.split_focus)
                    .tooltip(Tooltip::text("Every stage runs on its own, in order"))
                    .on_click(cx.listener(|this, _, _, cx| this.split(cx))),
            );
        }
        let submit = if editing {
            "Save group"
        } else {
            "Run side by side"
        };
        let frame = frame
            .footer(cancel_button(
                "Cancel",
                &self.cancel_focus,
                cx.listener(|_, _, _, cx| cx.emit(DismissEvent)),
            ))
            .footer(primary_button(
                submit,
                &self.submit_focus,
                count < 2,
                cx.listener(|this, _, window, cx| this.submit(window, cx)),
            ))
            .child(field("Group name (optional)", input_box(&self.name, cx)))
            .child(
                h_flex()
                    .child(div().flex_1().child(field_label("Pick 2 to 4 stages")))
                    .child(field_label(format!("{count} of {MAX_GROUP}"))),
            )
            .child(v_flex().children(rows))
            .child(div().mt(rems_from_px(-2.0_f32)).child(help_line(
                "They move next to the first picked stage and start together.",
            )));
        div()
            .key_context("Dialog")
            .track_focus(&self.focus)
            .child(frame)
    }
}
