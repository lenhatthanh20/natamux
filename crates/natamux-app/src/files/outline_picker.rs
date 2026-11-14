//! Go to symbol (Mod+Shift+O, T-0316): a dialog that lists the outline of the file (tree-sitter,
//! from `Buffer::outline`), filtered as you type. Up and Down move, Enter or a click picks, and
//! the cursor goes to the start of the item. Used by the editor tab and by the working tree of
//! the diff.

use std::rc::Rc;

use editor::{Editor, EditorEvent};
use gpui::{
    DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, KeyDownEvent, ScrollStrategy,
    Subscription, UniformListScrollHandle, uniform_list,
};
use language::Buffer;
use text::ToPoint;
use ui::{ListItem, ListItemSpacing, prelude::*};

use super::outline_rules::{OutlineRow, empty_text, outline_rows};
use crate::components::dialog::{DialogFrame, DialogLayer, DialogSize, DialogView};
use crate::components::form::{input_box, text_editor};

type OnPick = Rc<dyn Fn(u32, u32, &mut Window, &mut App)>;

/// Rows shown at once before the list scrolls.
const MAX_SHOWN: f32 = 12.;

/// The outline items of `buffer`, in file order.
pub fn outline_items(buffer: &Entity<Buffer>, cx: &App) -> Vec<OutlineRow> {
    let snapshot = buffer.read(cx).snapshot();
    snapshot
        .outline(None)
        .items
        .into_iter()
        .map(|item| {
            let start = item.range.start.to_point(&snapshot);
            OutlineRow {
                text: item.text.to_string(),
                depth: item.depth,
                row: start.row,
                column: start.column,
            }
        })
        .collect()
}

/// Opens the picker on `buffer`; `on_pick` gets the 0-based row and column of the picked item.
pub fn open_outline(
    buffer: &Entity<Buffer>,
    on_pick: impl Fn(u32, u32, &mut Window, &mut App) + 'static,
    window: &mut Window,
    cx: &mut App,
) {
    let items = outline_items(buffer, cx);
    let picker = cx.new(|cx| {
        let input = text_editor("Search symbols", None, window, cx);
        let edits = cx.subscribe(
            &input,
            |this: &mut OutlinePicker, _, event: &EditorEvent, cx| {
                if matches!(event, EditorEvent::BufferEdited) {
                    this.active = 0;
                    cx.notify();
                }
            },
        );
        OutlinePicker {
            input,
            items,
            active: 0,
            on_pick: Rc::new(on_pick),
            scroll: UniformListScrollHandle::new(),
            focus: cx.focus_handle(),
            _edits: edits,
        }
    });
    let first = picker.read(cx).input.focus_handle(cx);
    DialogLayer::open(picker, window, cx);
    window.focus(&first, cx);
}

pub struct OutlinePicker {
    input: Entity<Editor>,
    items: Vec<OutlineRow>,
    active: usize,
    on_pick: OnPick,
    scroll: UniformListScrollHandle,
    focus: FocusHandle,
    _edits: Subscription,
}

impl OutlinePicker {
    fn rows(&self, cx: &App) -> Vec<OutlineRow> {
        outline_rows(&self.items, &self.input.read(cx).text(cx))
    }

    fn pick(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.rows(cx).get(index).cloned() else {
            return;
        };
        cx.emit(DismissEvent);
        let on_pick = self.on_pick.clone();
        window.defer(cx, move |window, cx| {
            on_pick(row.row, row.column, window, cx)
        });
    }

    fn step(&mut self, down: bool, cx: &mut Context<Self>) {
        let count = self.rows(cx).len();
        if count == 0 {
            return;
        }
        self.active = if down {
            (self.active + 1) % count
        } else {
            (self.active + count - 1) % count
        };
        self.scroll
            .scroll_to_item(self.active, ScrollStrategy::Nearest);
        cx.notify();
    }

    fn on_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keys = &event.keystroke;
        if keys.modifiers.modified() {
            return;
        }
        match keys.key.as_str() {
            "down" => self.step(true, cx),
            "up" => self.step(false, cx),
            "enter" => self.pick(self.active, window, cx),
            _ => return,
        }
        cx.stop_propagation();
    }

    fn render_list(&self, rows: Vec<OutlineRow>, cx: &mut Context<Self>) -> AnyElement {
        if rows.is_empty() {
            return div()
                .py_2()
                .child(
                    Label::new(empty_text(&self.items))
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
                .into_any_element();
        }
        let active = self.active;
        let shown = (rows.len() as f32).min(MAX_SHOWN);
        uniform_list(
            "outline-rows",
            rows.len(),
            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                range
                    .filter_map(|index| Some((index, rows.get(index)?.clone())))
                    .map(|(index, row)| this.render_row(index, &row, index == active, cx))
                    .collect()
            }),
        )
        .track_scroll(&self.scroll)
        .h(rems_from_px(28. * shown))
        .into_any_element()
    }

    fn render_row(
        &self,
        index: usize,
        row: &OutlineRow,
        active: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        ListItem::new(("outline-row", index))
            .inset(true)
            .spacing(ListItemSpacing::Sparse)
            .toggle_state(active)
            .on_click(cx.listener(move |this, _, window, cx| this.pick(index, window, cx)))
            .child(
                div().pl(rems_from_px(12. * row.depth as f32)).child(
                    Label::new(row.text.clone())
                        .size(LabelSize::Small)
                        .buffer_font(cx)
                        .truncate(),
                ),
            )
            .into_any_element()
    }
}

impl EventEmitter<DismissEvent> for OutlinePicker {}

impl Focusable for OutlinePicker {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl DialogView for OutlinePicker {
    fn size(&self) -> DialogSize {
        DialogSize::Medium
    }

    fn tab_ring(&self, cx: &App) -> Vec<FocusHandle> {
        vec![self.input.focus_handle(cx)]
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pick(self.active, window, cx);
    }
}

impl Render for OutlinePicker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = self.rows(cx);
        let frame = DialogFrame::new(
            "Go to symbol",
            cx.listener(|_, _, _, cx| cx.emit(DismissEvent)),
        )
        .body_padded()
        .child(input_box(&self.input, cx))
        .child(self.render_list(rows, cx));
        div()
            .key_context("Dialog")
            .track_focus(&self.focus)
            .capture_key_down(cx.listener(Self::on_key))
            .child(frame)
    }
}
