//! The stage picker of the Add stage sheet (layout-b 5.4): a search field and the catalog
//! stages that are not in the pipeline, a group per phase. Up and Down move the active row, Enter
//! or a click adds its stage.

use editor::{Editor, EditorEvent};
use gpui::{
    AnyElement, Entity, EntityInputHandler, EventEmitter, FocusHandle, Focusable, ScrollHandle,
};
use natamux_proto::stages::capabilities::stage_name;
use natamux_proto::stages::catalog::CatalogStage;
use ui::{ScrollAxes, prelude::*};

use super::editor_catalog::{add_catalog_stage, picker_detail, picker_empty_text, picker_groups};
use super::list_rules::{kind_tile, kind_tile_icon};
use super::pipeline_editor::PipelineEditor;
use crate::components::button_style::button_icon;
use crate::components::form::{SettingsInput, text_editor};
use crate::components::scrollbars::scroll_area;
use crate::components::settings_card::{SeparatorInset, SettingsCard, SettingsGroup, foot_text};
use crate::components::settings_tile::{SettingsTile, TileSize};
use crate::components::text_size::row_text;

/// The body of the Add stage sheet is 460 px; the search field and its space take the rest.
const LIST_HEIGHT: f32 = 412.;

/// A stage was added and picked: the dialog closes.
pub struct StageAdded;

pub struct StagePicker {
    pipeline: Entity<PipelineEditor>,
    search: Entity<Editor>,
    /// The row Enter adds, as an index into the shown stages.
    active: usize,
    scroll: ScrollHandle,
    _search_edits: gpui::Subscription,
    /// The ids of the rows drawn last, for the flow tests.
    #[cfg(test)]
    drawn: std::cell::RefCell<Vec<String>>,
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
            #[cfg(test)]
            drawn: Default::default(),
        }
    }

    /// The ids of the rows drawn last, for the flow tests.
    #[cfg(test)]
    pub fn drawn_rows(&self) -> Vec<String> {
        self.drawn.borrow().clone()
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

    /// The child of the list (one group per phase) that holds the shown stage `index`.
    fn child_of(&self, index: usize, cx: &App) -> usize {
        let query = self.search.read(cx).text(cx);
        let mut seen = 0;
        for (child, (_, entries)) in picker_groups(self.pipeline.read(cx).steps(), &query)
            .into_iter()
            .enumerate()
        {
            seen += entries.len();
            if index < seen {
                return child;
            }
        }
        0
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

    /// A clickable row: kind tile, name and description, then kind and approval.
    fn render_row(
        &self,
        entry: &'static CatalogStage,
        active: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors();
        let (hover, selected) = (colors.ghost_element_hover, colors.ghost_element_selected);
        let id = entry.stage.id.clone();
        #[cfg(test)]
        self.drawn.borrow_mut().push(id.clone());
        let tile = SettingsTile::icon(
            button_icon(kind_tile_icon(entry.stage.kind)),
            kind_tile(&entry.stage),
            TileSize::Row,
        );
        h_flex()
            .id(SharedString::from(format!("pick-{id}")))
            .w_full()
            .min_h(rems_from_px(40.0_f32))
            .gap(rems_from_px(16.0_f32))
            .px(rems_from_px(12.0_f32))
            .py(rems_from_px(8.0_f32))
            .cursor_pointer()
            .when(active, |this| this.bg(selected))
            .when(!active, |this| this.hover(move |style| style.bg(hover)))
            .on_click(cx.listener(move |this, _, _, cx| this.add(&id, cx)))
            .child(tile)
            .child(
                v_flex()
                    .min_w_0()
                    .flex_1()
                    .child(
                        Label::new(stage_name(&entry.stage))
                            .size(row_text())
                            .truncate(),
                    )
                    .child(foot_text(entry.description)),
            )
            .child(
                Label::new(picker_detail(entry))
                    .size(LabelSize::Small)
                    .color(Color::Muted)
                    .single_line(),
            )
            .into_any_element()
    }

    /// One group per phase, each a card of rows; the active row is the one Enter adds.
    fn render_list(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let query = self.search.read(cx).text(cx);
        let steps = self.pipeline.read(cx).steps().to_vec();
        let mut groups: Vec<AnyElement> = Vec::new();
        let mut index = 0;
        #[cfg(test)]
        self.drawn.borrow_mut().clear();
        for (phase, entries) in picker_groups(&steps, &query) {
            let mut rows = Vec::new();
            for entry in entries {
                rows.push(self.render_row(entry, index == self.active, cx));
                index += 1;
            }
            groups.push(
                SettingsGroup::new(phase.name())
                    .child(SettingsCard::new(SeparatorInset::Tile).children(rows))
                    .into_any_element(),
            );
        }
        if let Some(text) = picker_empty_text(&steps, &query) {
            groups.push(
                div()
                    .pt(rems_from_px(12.0_f32))
                    .child(foot_text(text))
                    .into_any_element(),
            );
        }
        groups
    }
}

impl Render for StagePicker {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let groups = self.render_list(cx);
        let list = v_flex()
            .id("stage-picker-list")
            .max_h(rems_from_px(LIST_HEIGHT))
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .children(groups);
        v_flex()
            .pt(rems_from_px(12.0_f32))
            .child(SettingsInput::new(&self.search))
            .child(scroll_area(
                list,
                ScrollAxes::Vertical,
                &self.scroll,
                window,
                cx,
            ))
    }
}

#[cfg(test)]
#[path = "stage_picker_tests.rs"]
pub(super) mod tests;
