//! The stage picker (`StagePicker.tsx`): it takes the place of the details while a stage is
//! added. The catalog stages that are not in the pipeline, by phase, and a form for a stage of
//! your own.

use editor::{Editor, EditorEvent};
use gpui::{AnyElement, ClickEvent, Entity, FocusHandle, Focusable, FontWeight, Subscription};
use natamux_proto::stages::capabilities::stage_name;
use natamux_proto::stages::catalog::CatalogStage;
use ui::{IconButton, Tooltip, prelude::*};

use super::editor_catalog::{
    CUSTOM_KINDS, CustomKind, CustomStageInput, add_catalog_stage, add_custom_stage,
    custom_stage_problems, form_id, id_from_name, picker_detail, picker_empty_text, picker_groups,
    visible_problems,
};
use super::editor_labels::kind_label;
use super::kind_icon::kind_icon;
use super::pipeline_editor::PipelineEditor;
use super::stage_fields_view::{
    Choice, SELECT_WIDTH, error_line, field_label, inline_field, open_choices, select,
};
use crate::components::button_style::{action_button, quiet_button};
use crate::components::form::{input_box, text_editor};
use crate::components::tab_order;

/// The form of a stage of your own.
struct CustomForm {
    name: Entity<Editor>,
    id: Entity<Editor>,
    output: Entity<Editor>,
    /// The id typed by hand; None while it follows the name.
    own_id: Option<String>,
    kind: CustomKind,
    tried: bool,
    _edits: Vec<Subscription>,
}

pub struct StagePicker {
    pipeline: Entity<PipelineEditor>,
    search: Entity<Editor>,
    custom: Option<CustomForm>,
    /// The fields drawn last, in order, for the Tab ring of the dialog.
    stops: Vec<FocusHandle>,
    _search_edits: Subscription,
    /// The ids of the rows drawn last, for the flow tests.
    #[cfg(test)]
    drawn: std::cell::RefCell<Vec<String>>,
}

impl StagePicker {
    pub fn new(
        pipeline: Entity<PipelineEditor>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = text_editor("Search stages", None, window, cx);
        let edits = cx.subscribe(&search, |_, _, event: &EditorEvent, cx| {
            if matches!(event, EditorEvent::BufferEdited) {
                cx.notify();
            }
        });
        StagePicker {
            pipeline,
            search,
            custom: None,
            stops: Vec::new(),
            _search_edits: edits,
            #[cfg(test)]
            drawn: Default::default(),
        }
    }

    pub fn tab_stops(&self) -> Vec<FocusHandle> {
        self.stops.clone()
    }

    pub fn search_focus(&self, cx: &App) -> FocusHandle {
        self.search.focus_handle(cx)
    }

    /// The ids of the rows drawn last, for the flow tests.
    #[cfg(test)]
    pub fn drawn_rows(&self) -> Vec<String> {
        self.drawn.borrow().clone()
    }

    fn open_custom(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = text_editor("", None, window, cx);
        let id = text_editor("", None, window, cx);
        let output = text_editor("notes.md", None, window, cx);
        let edits = vec![
            cx.subscribe_in(&name, window, |this, _, event: &EditorEvent, window, cx| {
                if matches!(event, EditorEvent::BufferEdited) {
                    this.name_edited(window, cx);
                }
            }),
            cx.subscribe(&id, |this, _, event: &EditorEvent, cx| {
                if matches!(event, EditorEvent::BufferEdited) {
                    this.id_edited(cx);
                }
            }),
            cx.subscribe(&output, |_, _, event: &EditorEvent, cx| {
                if matches!(event, EditorEvent::BufferEdited) {
                    cx.notify();
                }
            }),
        ];
        let focus = name.focus_handle(cx);
        self.custom = Some(CustomForm {
            name,
            id,
            output,
            own_id: None,
            kind: CustomKind::Agent,
            tried: false,
            _edits: edits,
        });
        window.focus(&focus, cx);
        cx.notify();
    }

    /// The id follows the name until it is edited.
    fn name_edited(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(form) = &self.custom
            && form.own_id.is_none()
        {
            let id = id_from_name(&form.name.read(cx).text(cx));
            form.id
                .update(cx, |editor, cx| editor.set_text(id, window, cx));
        }
        cx.notify();
    }

    fn id_edited(&mut self, cx: &mut Context<Self>) {
        if let Some(form) = &mut self.custom {
            let text = form.id.read(cx).text(cx);
            let followed = form_id(&form.name.read(cx).text(cx), form.own_id.as_deref());
            if text != followed {
                form.own_id = Some(text);
            }
        }
        cx.notify();
    }

    fn input(&self, cx: &App) -> Option<CustomStageInput> {
        let form = self.custom.as_ref()?;
        Some(CustomStageInput {
            name: form.name.read(cx).text(cx),
            id: form.id.read(cx).text(cx),
            kind: form.kind,
            output: form.output.read(cx).text(cx),
        })
    }

    fn add_custom(&mut self, cx: &mut Context<Self>) {
        let Some(input) = self.input(cx) else {
            return;
        };
        if let Some(form) = &mut self.custom {
            form.tried = true;
        }
        let ready = custom_stage_problems(self.pipeline.read(cx).steps(), &input).is_empty();
        if ready {
            self.pipeline.update(cx, |pipeline, cx| {
                let steps = add_custom_stage(pipeline.steps(), &input);
                pipeline.edit(steps, cx);
                pipeline.select(&input.id, cx);
            });
        }
        cx.notify();
    }

    fn render_row(&self, entry: &'static CatalogStage, cx: &App) -> impl IntoElement {
        let hover = cx.theme().colors().ghost_element_hover;
        let (pipeline, id) = (self.pipeline.clone(), entry.stage.id.clone());
        #[cfg(test)]
        self.drawn.borrow_mut().push(id.clone());
        h_flex()
            .id(SharedString::from(format!("pick-{id}")))
            .w_full()
            .gap_2()
            .items_start()
            .px_1p5()
            .py_1p5()
            .rounded_md()
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .on_click(move |_, _, cx| {
                pipeline.update(cx, |pipeline, cx| {
                    let steps = add_catalog_stage(pipeline.steps(), &id, None);
                    pipeline.edit(steps, cx);
                    pipeline.select(&id, cx);
                })
            })
            .child(
                div()
                    .mt_0p5()
                    .child(kind_icon(entry.stage.kind).size(IconSize::Small)),
            )
            .child(
                v_flex()
                    .min_w_0()
                    .gap_0p5()
                    .child(Label::new(stage_name(&entry.stage)).weight(FontWeight::MEDIUM))
                    .child(Label::new(entry.description).size(LabelSize::Small))
                    .child(
                        Label::new(picker_detail(entry))
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    ),
            )
    }

    fn render_phases(&self, query: &str, cx: &App) -> Vec<AnyElement> {
        #[cfg(test)]
        self.drawn.borrow_mut().clear();
        let steps = self.pipeline.read(cx).steps();
        let mut rows: Vec<AnyElement> = picker_groups(steps, query)
            .into_iter()
            .map(|(phase, entries)| {
                v_flex()
                    .gap_0p5()
                    .child(
                        div().px_1p5().child(
                            Label::new(phase.name().to_uppercase())
                                .size(LabelSize::XSmall)
                                .weight(FontWeight::SEMIBOLD)
                                .color(Color::Muted),
                        ),
                    )
                    .children(entries.into_iter().map(|entry| self.render_row(entry, cx)))
                    .into_any_element()
            })
            .collect();
        if let Some(text) = picker_empty_text(steps, query) {
            rows.push(Label::new(text).color(Color::Muted).into_any_element());
        }
        rows
    }
}

impl Render for StagePicker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.search.read(cx).text(cx);
        let pipeline = self.pipeline.clone();
        let close = IconButton::new("close-stage-list", IconName::Close)
            .icon_size(IconSize::Small)
            .tooltip(Tooltip::text("Close the stage list"))
            .on_click(move |_, _, cx| {
                pipeline.update(cx, |pipeline, cx| pipeline.set_picking(false, cx))
            });
        // The search, then the form's fields: the order Tab takes.
        tab_order::begin();
        let search = input_box(&self.search, cx).into_any_element();
        let custom = match &self.custom {
            Some(_) => self.render_form(cx).into_any_element(),
            None => Button::new("custom-stage", "Custom stage")
                .style(ButtonStyle::Outlined)
                .start_icon(Icon::new(IconName::Plus).size(IconSize::Small))
                .on_click(cx.listener(|this, _, window, cx| this.open_custom(window, cx)))
                .into_any_element(),
        };
        self.stops = tab_order::end();
        v_flex()
            .gap_3()
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        div().flex_1().child(
                            Label::new("Add stage")
                                .size(LabelSize::Large)
                                .weight(FontWeight::SEMIBOLD),
                        ),
                    )
                    .child(close),
            )
            .child(search)
            .children(self.render_phases(&query, cx))
            .child(
                div()
                    .pt_3()
                    .border_t_1()
                    .border_color(cx.theme().colors().border)
                    .child(custom),
            )
    }
}

impl StagePicker {
    fn kind_menu(&self, cx: &mut Context<Self>) -> Button {
        let kind = self
            .custom
            .as_ref()
            .map_or(CustomKind::Agent, |form| form.kind);
        let this = cx.weak_entity();
        select("custom-kind", kind_label(kind.stage_kind())).on_click(
            move |event: &ClickEvent, window, cx| {
                let choices = CUSTOM_KINDS.iter().map(|choice| {
                    let (this, choice) = (this.clone(), *choice);
                    Choice::new(
                        kind_label(choice.stage_kind()),
                        Some(choice == kind),
                        move |_, cx| {
                            this.update(cx, |this, cx| {
                                if let Some(form) = &mut this.custom {
                                    form.kind = choice;
                                }
                                cx.notify();
                            })
                            .ok();
                        },
                    )
                });
                open_choices(choices.collect(), event.position(), window, cx);
            },
        )
    }

    fn render_form(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (Some(form), Some(input)) = (&self.custom, self.input(cx)) else {
            return div().into_any_element();
        };
        let problems = custom_stage_problems(self.pipeline.read(cx).steps(), &input);
        let shown = visible_problems(&problems, &input, form.tried);
        let (name, id, output) = (form.name.clone(), form.id.clone(), form.output.clone());
        let agent = form.kind == CustomKind::Agent;
        let kind = self.kind_menu(cx);
        let back = quiet_button("custom-back", "Back to the list").on_click(cx.listener(
            |this, _, _, cx| {
                this.custom = None;
                cx.notify();
            },
        ));
        let add = action_button("custom-add", "Add custom stage")
            .start_icon(
                Icon::new(IconName::Plus)
                    .size(IconSize::Small)
                    .color(Color::Muted),
            )
            .on_click(cx.listener(|this, _, _, cx| this.add_custom(cx)));
        let app: &App = cx;
        let group = |label: &'static str, editor: &Entity<Editor>, problem: Option<String>| {
            v_flex()
                .gap_1p5()
                .child(field_label(label))
                .child(input_box(editor, app))
                .children(problem.map(error_line))
        };
        v_flex()
            .gap_3()
            .child(
                Label::new("CUSTOM STAGE")
                    .size(LabelSize::XSmall)
                    .weight(FontWeight::SEMIBOLD)
                    .color(Color::Muted),
            )
            .child(group("Name", &name, shown.name))
            .child(group("Id", &id, shown.id))
            .child(inline_field(
                "Kind",
                None,
                None,
                div().w(rems_from_px(SELECT_WIDTH)).child(kind),
            ))
            .when(agent, |this| {
                this.child(group("Output file", &output, shown.output))
            })
            .child(
                Label::new("It waits for your approval. You can change that.")
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
            .child(h_flex().justify_end().gap_2().child(back).child(add))
            .into_any_element()
    }
}

#[cfg(test)]
#[path = "stage_picker_tests.rs"]
mod tests;
