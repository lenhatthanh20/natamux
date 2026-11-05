//! The stage picker (`StagePicker.tsx`) of the Add stage dialog: a search field and the catalog
//! stages that are not in the pipeline, by phase. Up and Down move the active row, Enter or a
//! click adds its stage.

use editor::{Editor, EditorEvent};
use gpui::{
    AnyElement, Entity, EntityInputHandler, EventEmitter, FocusHandle, Focusable, FontWeight,
    ScrollHandle,
};
use natamux_proto::stages::capabilities::stage_name;
use natamux_proto::stages::catalog::CatalogStage;
use ui::{ScrollAxes, prelude::*};

use super::editor_catalog::{add_catalog_stage, picker_detail, picker_empty_text, picker_groups};
use super::kind_icon::kind_icon;
use super::pipeline_editor::PipelineEditor;
use crate::components::form::{input_box, text_editor};
use crate::components::scrollbars::scroll_area;

/// A stage was added and picked: the dialog closes.
pub struct StageAdded;

pub struct StagePicker {
    pipeline: Entity<PipelineEditor>,
    search: Entity<Editor>,
    /// The row Enter adds, as an index into the shown stages.
    active: usize,
    scroll: ScrollHandle,
    _search_edits: gpui::Subscription,
}

impl EventEmitter<StageAdded> for StagePicker {}

impl StagePicker {
    pub fn new(
        pipeline: Entity<PipelineEditor>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = text_editor("Search stages", None, window, cx);
        let edits = cx.subscribe(&search, |this: &mut Self, _, event: &EditorEvent, cx| {
            if matches!(event, EditorEvent::BufferEdited) {
                this.active = 0;
                cx.notify();
            }
        });
        StagePicker {
            pipeline,
            search,
            active: 0,
            scroll: ScrollHandle::new(),
            _search_edits: edits,
        }
    }

    pub fn search_focus(&self, cx: &App) -> FocusHandle {
        self.search.focus_handle(cx)
    }

    /// The search field is composing text: its keys stay with it.
    pub fn composing(this: &Entity<Self>, window: &mut Window, cx: &mut App) -> bool {
        let search = this.read(cx).search.clone();
        search.update(cx, |editor, cx| {
            editor.marked_text_range(window, cx).is_some()
        })
    }

    /// The stages shown, in list order.
    fn shown(&self, cx: &App) -> Vec<&'static CatalogStage> {
        let query = self.search.read(cx).text(cx);
        picker_groups(self.pipeline.read(cx).steps(), &query)
            .into_iter()
            .flat_map(|(_, entries)| entries)
            .collect()
    }

    /// Up (-1) or Down (1), wrapping.
    pub fn move_active(&mut self, by: i32, cx: &mut Context<Self>) {
        let count = self.shown(cx).len();
        if count == 0 {
            return;
        }
        let at = self.active.min(count - 1);
        self.active = if by < 0 {
            (at + count - 1) % count
        } else {
            (at + 1) % count
        };
        self.scroll.scroll_to_item(self.child_of(self.active, cx));
        cx.notify();
    }

    /// The child of the list that shows the shown stage `index`: phase titles are children too.
    fn child_of(&self, index: usize, cx: &App) -> usize {
        let query = self.search.read(cx).text(cx);
        let mut child = 0;
        let mut seen = 0;
        for (_, entries) in picker_groups(self.pipeline.read(cx).steps(), &query) {
            child += 1;
            if index < seen + entries.len() {
                return child + index - seen;
            }
            child += entries.len();
            seen += entries.len();
        }
        child
    }

    /// Enter: adds the active stage.
    pub fn add_active(&mut self, cx: &mut Context<Self>) {
        let shown = self.shown(cx);
        if let Some(entry) = shown.get(self.active.min(shown.len().saturating_sub(1))) {
            let id = entry.stage.id.clone();
            self.add(&id, cx);
        }
    }

    fn add(&mut self, id: &str, cx: &mut Context<Self>) {
        self.pipeline.update(cx, |pipeline, cx| {
            let steps = add_catalog_stage(pipeline.steps(), id, None);
            pipeline.edit(steps, cx);
            pipeline.select(id, cx);
        });
        cx.emit(StageAdded);
    }

    fn render_row(
        &self,
        entry: &'static CatalogStage,
        active: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors();
        let (hover, selected) = (colors.ghost_element_hover, colors.ghost_element_selected);
        let id = entry.stage.id.clone();
        h_flex()
            .id(SharedString::from(format!("pick-{id}")))
            .w_full()
            .gap_2()
            .items_start()
            .px_1p5()
            .py_1p5()
            .rounded_md()
            .cursor_pointer()
            .when(active, |this| this.bg(selected))
            .when(!active, |this| this.hover(move |style| style.bg(hover)))
            .on_click(cx.listener(move |this, _, _, cx| this.add(&id, cx)))
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
            .into_any_element()
    }

    /// Phase titles and rows as children of one list, so a row can be scrolled to.
    fn render_list(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let query = self.search.read(cx).text(cx);
        let steps = self.pipeline.read(cx).steps().to_vec();
        let mut rows: Vec<AnyElement> = Vec::new();
        let mut index = 0;
        for (phase, entries) in picker_groups(&steps, &query) {
            rows.push(
                div()
                    .px_1p5()
                    .pt_1()
                    .child(
                        Label::new(phase.name().to_uppercase())
                            .size(LabelSize::XSmall)
                            .weight(FontWeight::SEMIBOLD)
                            .color(Color::Muted),
                    )
                    .into_any_element(),
            );
            for entry in entries {
                rows.push(self.render_row(entry, index == self.active, cx));
                index += 1;
            }
        }
        if let Some(text) = picker_empty_text(&steps, &query) {
            rows.push(Label::new(text).color(Color::Muted).into_any_element());
        }
        rows
    }
}

impl Render for StagePicker {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = self.render_list(cx);
        let list = v_flex()
            .id("stage-picker-list")
            .max_h(rems_from_px(360.0_f32))
            .gap_0p5()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .children(rows);
        v_flex()
            .gap_2()
            .child(input_box(&self.search, cx))
            .child(scroll_area(
                list,
                ScrollAxes::Vertical,
                &self.scroll,
                window,
                cx,
            ))
    }
}
