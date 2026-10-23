//! The text diff of the Changes tab (docs/ui-spec.md 4.24, `MonacoDiff.tsx` and the whole-file
//! body of `FileBody.tsx`), on Zed's editor and `buffer_diff`: read-only editors over the
//! working-tree text, with the last commit's text as the diff base.
//!
//! Inline is one editor with the deleted lines shown between the new ones. Side by side is two
//! editors: the last commit at the left (its deleted lines marked), the working tree at the
//! right (its added lines marked). Empty rows keep the two sides level, and a scroll of one side
//! scrolls the other. Zed's own split view needs a project and a workspace, which this app has
//! not. "Collapse unchanged lines" shows 3 lines around each change.

use std::ops::Range;
use std::sync::Arc;

use buffer_diff::BufferDiff;
use editor::scroll::Autoscroll;
use editor::{Editor, EditorEvent, MultiBuffer, SelectionEffects};
use gpui::{Entity, EventEmitter, Focusable, Subscription};
use language::{Buffer, Capability};
use text::Point;
use ui::prelude::*;

use super::change_rules::{
    LineChange, Side, Step, change_anchor, change_at, step_change, whole_lines,
};
use super::comment_rules::Quote;
use super::diff_editors::{add_spacer, read_only_editor, set_excerpts};
use super::diff_rules::{HunkRows, line_change, spacers};
use super::languages::language_for;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiffOptions {
    pub side_by_side: bool,
    pub collapse: bool,
    pub wrap: bool,
}

/// What the Changes tab follows: the changes were worked out (the toolbar's counts and
/// navigator), or the cursor or the selection moved (the comment box's label).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffEvent {
    Ready,
    CursorMoved,
}

struct Sides {
    /// The working tree at the right, or alone when inline.
    right: Entity<Editor>,
    left: Option<Entity<Editor>>,
    _subscriptions: Vec<Subscription>,
}

pub struct DiffView {
    key: Option<String>,
    version: u64,
    options: Option<DiffOptions>,
    buffer: Option<Entity<Buffer>>,
    diff: Option<Entity<BufferDiff>>,
    hunks: Vec<HunkRows>,
    sides: Option<Sides>,
    syncing: bool,
    _update: Option<gpui::Task<()>>,
}

impl EventEmitter<DiffEvent> for DiffView {}

impl DiffView {
    pub fn new() -> Self {
        DiffView {
            key: None,
            version: 0,
            options: None,
            buffer: None,
            diff: None,
            hunks: Vec::new(),
            sides: None,
            syncing: false,
            _update: None,
        }
    }

    /// Shows `modified` against `original` (None: the file is new). `key` names the task and the
    /// file, `version` the read of its texts: the same key with a newer read keeps the scroll
    /// position. Cheap when nothing changed, so a render may call it.
    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &mut self,
        key: &str,
        path: &str,
        version: u64,
        original: Option<&str>,
        modified: &str,
        options: DiffOptions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let same_file = self.key.as_deref() == Some(key);
        if same_file && self.version == version {
            if self.options != Some(options) {
                self.options = Some(options);
                self.build(window, cx);
            }
            return;
        }
        let scroll = same_file
            .then_some(self.sides.as_ref())
            .flatten()
            .map(|sides| {
                sides
                    .right
                    .update(cx, |editor, cx| editor.scroll_position(cx))
            });
        self.key = Some(key.to_owned());
        self.version = version;
        self.options = Some(options);
        let buffer = cx.new(|cx| {
            let mut buffer = Buffer::local(modified, cx);
            buffer.set_capability(Capability::ReadOnly, cx);
            buffer
        });
        let snapshot = buffer.read(cx).text_snapshot();
        let diff = cx.new(|cx| BufferDiff::new(&snapshot, None, None, cx));
        let base: Option<Arc<str>> = original.map(Arc::from);
        let update = diff.update(cx, |diff, cx| diff.set_base_text(base, snapshot, cx));
        self.color_by_language(path, &buffer, &diff, cx);
        self.buffer = Some(buffer);
        self.diff = Some(diff);
        self._update = Some(cx.spawn_in(window, async move |this, cx| {
            update.await;
            this.update_in(cx, |this, window, cx| {
                this.read_hunks(cx);
                this.build(window, cx);
                if let (Some(position), Some(sides)) = (scroll, this.sides.as_ref()) {
                    sides.right.update(cx, |editor, cx| {
                        editor.set_scroll_position(position, window, cx);
                    });
                }
                cx.emit(DiffEvent::Ready);
            })
            .ok();
        }));
    }

    /// Both sides take the language of the file's extension once it is loaded, for colours.
    fn color_by_language(
        &self,
        path: &str,
        buffer: &Entity<Buffer>,
        diff: &Entity<BufferDiff>,
        cx: &mut Context<Self>,
    ) {
        let Some(language) = language_for(path, cx) else {
            return;
        };
        let (buffer, base) = (buffer.clone(), diff.read(cx).base_text_buffer().clone());
        cx.spawn(async move |_, cx| {
            if let Some(language) = language.await {
                buffer.update(cx, |buffer, cx| {
                    buffer.set_language(Some(language.clone()), cx)
                });
                base.update(cx, |base, cx| base.set_language(Some(language), cx));
            }
        })
        .detach();
    }

    /// Another body shows: drop the editors, so a later show starts fresh.
    pub fn clear(&mut self) {
        *self = DiffView::new();
    }

    fn read_hunks(&mut self, cx: &App) {
        let (Some(buffer), Some(diff)) = (&self.buffer, &self.diff) else {
            return;
        };
        let text = buffer.read(cx).text_snapshot();
        let snapshot = diff.read(cx).snapshot(cx);
        let base = snapshot.base_text();
        self.hunks = snapshot
            .hunks(&text)
            .map(|hunk| {
                let old_start = base.offset_to_point(hunk.diff_base_byte_range.start);
                let old_end = base.offset_to_point(hunk.diff_base_byte_range.end);
                let old_end_row = old_end.row + u32::from(old_end.column > 0);
                let new_end_row = hunk.range.end.row + u32::from(hunk.range.end.column > 0);
                HunkRows {
                    old: old_start.row..old_end_row.max(old_start.row),
                    new: hunk.range.start.row..new_end_row.max(hunk.range.start.row),
                }
            })
            .collect();
    }

    /// The diff of the shown texts is worked out and drawn.
    pub fn ready(&self) -> bool {
        self.sides.is_some()
    }

    /// The changes in Monaco's shape, for the counts and the navigator.
    pub fn changes(&self) -> Vec<LineChange> {
        self.hunks.iter().map(line_change).collect()
    }

    fn build(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(buffer), Some(diff), Some(options)) =
            (self.buffer.clone(), self.diff.clone(), self.options)
        else {
            return;
        };
        let new_ranges: Vec<Range<Point>> = self.ranges(|hunk| &hunk.new);
        let right_buffer = cx.new(|cx| {
            let mut multi = MultiBuffer::new(Capability::ReadOnly);
            set_excerpts(&mut multi, &buffer, new_ranges, options.collapse, cx);
            multi.add_diff(diff.clone(), cx);
            multi.set_all_diff_hunks_expanded(cx);
            if options.side_by_side {
                multi.set_show_deleted_hunks(false, cx);
            }
            multi
        });
        let right = read_only_editor(right_buffer, &buffer, options.wrap, window, cx);
        let left = options.side_by_side.then(|| {
            let base = diff.read(cx).base_text_buffer().clone();
            let old_ranges = self.ranges(|hunk| &hunk.old);
            let left_buffer = cx.new(|cx| {
                let mut multi = MultiBuffer::new(Capability::ReadOnly);
                set_excerpts(&mut multi, &base, old_ranges, options.collapse, cx);
                multi.add_inverted_diff(diff.clone(), buffer.clone(), cx);
                multi.set_all_diff_hunks_expanded(cx);
                multi
            });
            read_only_editor(left_buffer, &base, options.wrap, window, cx)
        });
        let mut subscriptions = vec![cx.subscribe_in(&right, window, Self::on_editor)];
        if let Some(left) = &left {
            subscriptions.push(cx.subscribe_in(left, window, Self::on_editor));
            for spacer in spacers(&self.hunks) {
                let (editor, side_buffer) = match spacer.side {
                    Side::Old => (left, diff.read(cx).base_text_buffer().clone()),
                    Side::New => (&right, buffer.clone()),
                };
                add_spacer(editor, &side_buffer, spacer, cx);
            }
        }
        self.sides = Some(Sides {
            right,
            left,
            _subscriptions: subscriptions,
        });
        cx.notify();
    }

    /// The rows of one side around each change, or the whole file.
    fn ranges(&self, rows: impl Fn(&HunkRows) -> &Range<u32>) -> Vec<Range<Point>> {
        self.hunks
            .iter()
            .map(|hunk| {
                let rows = rows(hunk);
                Point::new(rows.start, 0)..Point::new(rows.end, 0)
            })
            .collect()
    }

    fn on_editor(
        &mut self,
        editor: &Entity<Editor>,
        event: &EditorEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            EditorEvent::ScrollPositionChanged { .. } => self.follow_scroll(editor, window, cx),
            EditorEvent::SelectionsChanged { .. } => cx.emit(DiffEvent::CursorMoved),
            _ => {}
        }
    }

    /// A scroll of one side scrolls the other to the same row, as both have the same rows.
    fn follow_scroll(
        &mut self,
        from: &Entity<Editor>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(sides) = &self.sides else {
            return;
        };
        let Some(left) = &sides.left else {
            return;
        };
        if self.syncing {
            return;
        }
        let to = if from == left { &sides.right } else { left }.clone();
        let position = from.update(cx, |editor, cx| editor.scroll_position(cx));
        self.syncing = true;
        to.update(cx, |editor, cx| {
            editor.set_scroll_position(position, window, cx);
        });
        self.syncing = false;
    }

    /// The line of the working tree under the cursor, 1-based.
    pub fn cursor_line(&self, cx: &mut App) -> Option<u32> {
        let right = self.sides.as_ref()?.right.clone();
        right.update(cx, |editor, cx| {
            let snapshot = editor.display_snapshot(cx);
            let head = editor.selections.newest::<Point>(&snapshot).head();
            let multi = editor.buffer().read(cx).snapshot(cx);
            let (_, point) = multi.point_to_buffer_point(head)?;
            Some(point.row + 1)
        })
    }

    /// Alt+F5 and Shift+Alt+F5: the cursor goes to the next or the previous change, which
    /// scrolls to the top. Changes wrap at both ends.
    pub fn step_change(&mut self, by: Step, window: &mut Window, cx: &mut Context<Self>) {
        let changes = self.changes();
        let line = self.cursor_line(cx);
        let Some(index) = step_change(&changes, line, by) else {
            return;
        };
        let (Some(sides), Some(buffer)) = (&self.sides, &self.buffer) else {
            return;
        };
        let row = change_anchor(&changes[index]).0.saturating_sub(1);
        let buffer = buffer.clone();
        sides.right.update(cx, |editor, cx| {
            let anchor =
                editor
                    .buffer()
                    .read(cx)
                    .buffer_point_to_anchor(&buffer, Point::new(row, 0), cx);
            if let Some(anchor) = anchor {
                let effects = SelectionEffects::scroll(Autoscroll::top_relative(0.));
                editor.change_selections(effects, window, cx, |selections| {
                    selections.select_anchor_ranges([anchor..anchor])
                });
            }
        });
    }

    /// The change the cursor is in, for "2 of 7".
    pub fn change_index(&self, cx: &mut App) -> Option<usize> {
        let line = self.cursor_line(cx)?;
        change_at(&self.changes(), line)
    }

    /// The whole lines picked in either side, for Ask agent. None without a selection.
    pub fn selection(&self, window: &Window, cx: &mut App) -> Option<Quote> {
        let sides = self.sides.as_ref()?;
        let left_focused = sides
            .left
            .as_ref()
            .is_some_and(|left| left.read(cx).focus_handle(cx).contains_focused(window, cx));
        let (editor, side) = match (&sides.left, left_focused) {
            (Some(left), true) => (left.clone(), Side::Old),
            _ => (sides.right.clone(), Side::New),
        };
        editor.update(cx, |editor, cx| {
            let snapshot = editor.display_snapshot(cx);
            let picked = editor.selections.newest::<Point>(&snapshot);
            if picked.is_empty() {
                return None;
            }
            let multi = editor.buffer().read(cx).snapshot(cx);
            let (text, start) = multi.point_to_buffer_point(picked.start)?;
            let (_, end) = multi.point_to_buffer_point(picked.end)?;
            let (first, last) = whole_lines(start.row + 1, end.row + 1, end.column + 1);
            let range = Point::new(first - 1, 0)..Point::new(last - 1, text.line_len(last - 1));
            Some(Quote {
                side,
                start: first,
                end: last,
                text: text.text_for_range(range).collect(),
            })
        })
    }

    /// Picks whole lines `first` to `last` (1-based) of the working tree, as a drag does.
    pub fn select_lines(
        &mut self,
        first: u32,
        last: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (Some(sides), Some(buffer)) = (&self.sides, &self.buffer) else {
            return;
        };
        let buffer = buffer.clone();
        let (first, last) = (first.saturating_sub(1), last.saturating_sub(1));
        let end_column = buffer.read(cx).text_snapshot().line_len(last);
        sides.right.update(cx, |editor, cx| {
            let multi = editor.buffer().read(cx);
            let start = multi.buffer_point_to_anchor(&buffer, Point::new(first, 0), cx);
            let end = multi.buffer_point_to_anchor(&buffer, Point::new(last, end_column), cx);
            if let (Some(start), Some(end)) = (start, end) {
                editor.change_selections(SelectionEffects::no_scroll(), window, cx, |selections| {
                    selections.select_anchor_ranges([start..end])
                });
            }
        });
    }

    /// The editor that takes the keyboard: the working tree.
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        if let Some(sides) = &self.sides {
            let handle = sides.right.read(cx).focus_handle(cx);
            window.focus(&handle, cx);
        }
    }
}

impl Render for DiffView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(sides) = &self.sides else {
            return div().size_full().into_any_element();
        };
        let border = cx.theme().colors().border;
        match &sides.left {
            Some(left) => h_flex()
                .size_full()
                .child(div().flex_1().min_w_0().h_full().child(left.clone()))
                .child(div().w_px().h_full().bg(border))
                .child(div().flex_1().min_w_0().h_full().child(sides.right.clone()))
                .into_any_element(),
            None => div()
                .size_full()
                .child(sides.right.clone())
                .into_any_element(),
        }
    }
}
