//! The Zed side of `FindTarget` and `BlameTarget`: one `Editor`. It goes with the editor stack.

use std::cell::{Cell, RefCell};
use std::ops::Range;

use editor::display_map::HighlightKey;
use editor::scroll::Autoscroll;
use editor::{DisplayPoint, Editor, EditorEvent, Inlay, MultiBufferOffset, SelectionEffects};
use gpui::{App, Entity, EntityId, FocusHandle, Focusable, Subscription, WeakEntity, Window};
use language::Buffer;
use text::{Point, ToPoint};

use super::code_target::{BlameTarget, FindTarget, RowMenu};
use crate::files::outline_rules::OutlineRow;

fn anchors(
    editor: &Editor,
    ranges: &[Range<usize>],
    cx: &App,
) -> Vec<Range<editor::Anchor>> {
    let snapshot = editor.buffer().read(cx).snapshot(cx);
    ranges
        .iter()
        .map(|range| {
            snapshot.anchor_after(MultiBufferOffset(range.start))
                ..snapshot.anchor_before(MultiBufferOffset(range.end))
        })
        .collect()
}

impl FindTarget for Entity<Editor> {
    fn id(&self) -> EntityId {
        self.entity_id()
    }

    fn text(&self, cx: &App) -> String {
        self.read(cx).text(cx)
    }

    fn one_line_selection(&self, cx: &mut App) -> Option<String> {
        self.update(cx, |editor, cx| {
            let display = editor.display_snapshot(cx);
            let picked = editor.selections.newest::<Point>(&display);
            if picked.is_empty() || picked.start.row != picked.end.row {
                return None;
            }
            let snapshot = editor.buffer().read(cx).snapshot(cx);
            Some(snapshot.text_for_range(picked.start..picked.end).collect())
        })
    }

    fn caret(&self, cx: &mut App) -> usize {
        self.update(cx, |editor, cx| {
            let display = editor.display_snapshot(cx);
            editor
                .selections
                .newest::<MultiBufferOffset>(&display)
                .head()
                .0
        })
    }

    fn show_matches(&self, ranges: &[Range<usize>], active: Option<usize>, cx: &mut App) {
        self.update(cx, |editor, cx| {
            let ranges = anchors(editor, ranges, cx);
            editor.highlight_background(
                HighlightKey::BufferSearchHighlights,
                &ranges,
                move |index, theme| {
                    if Some(*index) == active {
                        theme.colors().search_active_match_background
                    } else {
                        theme.colors().search_match_background
                    }
                },
                cx,
            );
        });
    }

    fn clear_matches(&self, cx: &mut App) {
        self.update(cx, |editor, cx| {
            editor.clear_background_highlights(HighlightKey::BufferSearchHighlights, cx);
        });
    }

    fn select(&self, range: Range<usize>, window: &mut Window, cx: &mut App) {
        self.update(cx, |editor, cx| {
            let range = anchors(editor, &[range], cx);
            let effects = SelectionEffects::scroll(Autoscroll::center());
            editor.change_selections(effects, window, cx, |selections| {
                selections.select_anchor_ranges(range)
            });
        });
    }

    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.read(cx).focus_handle(cx)
    }

    fn on_edited(&self, changed: Box<dyn Fn(&mut App)>, cx: &mut App) -> Subscription {
        cx.subscribe(self, move |_, event: &EditorEvent, cx| {
            if matches!(event, EditorEvent::BufferEdited) {
                changed(cx);
            }
        })
    }
}

/// An editor and the buffer its labels and menu rows belong to.
pub struct ZedBlame {
    editor: WeakEntity<Editor>,
    buffer: Entity<Buffer>,
    /// Inlay ids far above the editor's own, so they never meet.
    next_id: Cell<usize>,
    shown: RefCell<Vec<Inlay>>,
}

impl ZedBlame {
    pub fn new(editor: &Entity<Editor>, buffer: Entity<Buffer>) -> Self {
        ZedBlame {
            editor: editor.downgrade(),
            buffer,
            next_id: Cell::new(usize::MAX / 2),
            shown: RefCell::new(Vec::new()),
        }
    }
}

/// The line of `buffer` at a clicked row; None for a row of another buffer (a deleted line of
/// an inline diff).
fn buffer_row(
    editor: &mut Editor,
    point: DisplayPoint,
    buffer: &Entity<Buffer>,
    cx: &mut gpui::Context<Editor>,
) -> Option<u32> {
    let snapshot = editor.display_snapshot(cx);
    let point = point.to_point(&snapshot);
    let multi = editor.buffer().read(cx).snapshot(cx);
    let (found, point) = multi.point_to_buffer_point(point)?;
    (found.remote_id() == buffer.read(cx).remote_id()).then_some(point.row)
}

impl BlameTarget for ZedBlame {
    fn lines(&self, cx: &App) -> u32 {
        self.buffer.read(cx).max_point().row + 1
    }

    /// The right-click menu of the text. Line numbers get it once the Zed fork opens a custom
    /// menu from the gutter of an editor with no project.
    fn set_menu(&self, menu: RowMenu, cx: &mut App) {
        let Some(editor) = self.editor.upgrade() else {
            return;
        };
        let buffer = self.buffer.clone();
        editor.update(cx, |editor, _| {
            editor.set_custom_context_menu(move |editor, point, window, cx| {
                let row = buffer_row(editor, point, &buffer, cx);
                menu(row, window, cx)
            });
        });
    }

    fn set_labels(&self, labels: Vec<(u32, String)>, cx: &mut App) {
        let Some(editor) = self.editor.upgrade() else {
            return;
        };
        let first_id = self.next_id.get();
        let lines = self.lines(cx) as usize;
        let old: Vec<_> = self.shown.borrow_mut().drain(..).map(|inlay| inlay.id).collect();
        let buffer = self.buffer.clone();
        let inlays: Vec<Inlay> = editor.update(cx, |editor, cx| {
            let multi = editor.buffer().read(cx);
            let inlays = labels
                .into_iter()
                .enumerate()
                .filter_map(|(at, (line, label))| {
                    // A line outside every excerpt (collapsed) has no anchor and no label.
                    let anchor = multi.buffer_point_to_anchor(&buffer, Point::new(line, 0), cx)?;
                    Some(Inlay::debugger(first_id.wrapping_add(at), anchor, label))
                })
                .collect::<Vec<_>>();
            editor.splice_inlays(&old, inlays.clone(), cx);
            inlays
        });
        *self.shown.borrow_mut() = inlays;
        self.next_id.set(first_id.wrapping_add(lines));
    }

    fn clear_labels(&self, cx: &mut App) {
        let ids: Vec<_> = self.shown.borrow_mut().drain(..).map(|inlay| inlay.id).collect();
        if let Some(editor) = self.editor.upgrade() {
            editor.update(cx, |editor, cx| editor.splice_inlays(&ids, Vec::new(), cx));
        }
    }
}

/// The outline items of `buffer`, in file order (tree-sitter, from `Buffer::outline`).
pub fn zed_outline_rows(buffer: &Entity<Buffer>, cx: &App) -> Vec<OutlineRow> {
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
