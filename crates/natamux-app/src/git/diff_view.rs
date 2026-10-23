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
use editor::display_map::CustomBlockId;
use editor::{Editor, EditorEvent, MultiBuffer};
use gpui::{Entity, EventEmitter, Subscription};
use language::{Buffer, Capability};
use text::Point;
use ui::prelude::*;

use super::change_rules::{LineChange, Side};
use super::diff_editors::{add_spacer, read_only_editor, refit_excerpts, set_excerpts};
use super::diff_rules::{DiffRefresh, HunkRows, diff_refresh, line_change, spacers};
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

pub(super) struct Sides {
    /// The working tree at the right, or alone when inline.
    pub(super) right: Entity<Editor>,
    pub(super) left: Option<Entity<Editor>>,
    /// The empty rows of side by side, taken away when a newer read changes the hunks.
    spacer_blocks: Vec<(Entity<Editor>, CustomBlockId)>,
    _subscriptions: Vec<Subscription>,
}

pub struct DiffView {
    key: Option<String>,
    version: u64,
    options: Option<DiffOptions>,
    pub(super) buffer: Option<Entity<Buffer>>,
    diff: Option<Entity<BufferDiff>>,
    hunks: Vec<HunkRows>,
    pub(super) sides: Option<Sides>,
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
    /// file, `version` the read of its texts: a newer read of the same file changes the texts
    /// inside the editors, so the cursor, the selection and the scroll stay. Cheap when nothing
    /// changed, so a render may call it.
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
        let same_read = same_file && self.version == version;
        match diff_refresh(same_file, same_read, self.options == Some(options)) {
            DiffRefresh::Keep => {}
            DiffRefresh::Update if self.sides.is_some() => {
                self.version = version;
                self.update_texts(original, modified, window, cx);
            }
            _ if same_read => {
                self.options = Some(options);
                self.build(window, cx);
            }
            _ => self.load(
                key,
                path,
                version,
                (original, modified),
                options,
                window,
                cx,
            ),
        }
    }

    /// New buffers for `texts` (original, modified); the same file keeps its scroll position.
    #[allow(clippy::too_many_arguments)]
    fn load(
        &mut self,
        key: &str,
        path: &str,
        version: u64,
        (original, modified): (Option<&str>, &str),
        options: DiffOptions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let same_file = self.key.as_deref() == Some(key);
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

    /// A newer read of the shown file: the buffer takes only the edits between the texts, so
    /// anchors outside them (the cursor, the selection) stay; the base is diffed in place too.
    fn update_texts(
        &mut self,
        original: Option<&str>,
        modified: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (Some(buffer), Some(diff)) = (self.buffer.clone(), self.diff.clone()) else {
            return;
        };
        let edits = buffer.read(cx).diff(modified.to_owned(), cx);
        let base: Option<Arc<str>> = original.map(Arc::from);
        self._update = Some(cx.spawn_in(window, async move |this, cx| {
            let edits = edits.await;
            let Ok(update) = this.update(cx, |_, cx| {
                let snapshot = buffer.update(cx, |buffer, cx| {
                    buffer.set_capability(Capability::ReadWrite, cx);
                    buffer.apply_diff(edits, cx);
                    buffer.set_capability(Capability::ReadOnly, cx);
                    buffer.text_snapshot()
                });
                diff.update(cx, |diff, cx| diff.set_base_text(base, snapshot, cx))
            }) else {
                return;
            };
            update.await;
            this.update(cx, |this, cx| {
                this.read_hunks(cx);
                this.refit(cx);
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
        }
        let spacer_blocks = self.add_spacers(&right, left.as_ref(), cx);
        self.sides = Some(Sides {
            right,
            left,
            spacer_blocks,
            _subscriptions: subscriptions,
        });
        cx.notify();
    }

    /// The empty rows that keep the two sides level; none when inline.
    fn add_spacers(
        &self,
        right: &Entity<Editor>,
        left: Option<&Entity<Editor>>,
        cx: &mut App,
    ) -> Vec<(Entity<Editor>, CustomBlockId)> {
        let (Some(left), Some(buffer), Some(diff)) = (left, &self.buffer, &self.diff) else {
            return Vec::new();
        };
        let base = diff.read(cx).base_text_buffer().clone();
        spacers(&self.hunks)
            .into_iter()
            .filter_map(|spacer| {
                let (editor, side_buffer) = match spacer.side {
                    Side::Old => (left, &base),
                    Side::New => (right, buffer),
                };
                let id = add_spacer(editor, side_buffer, spacer, cx)?;
                Some((editor.clone(), id))
            })
            .collect()
    }

    /// After a newer read: the excerpts and the empty rows follow the new changes, in the same
    /// editors.
    fn refit(&mut self, cx: &mut Context<Self>) {
        let (Some(buffer), Some(diff), Some(options)) =
            (self.buffer.clone(), self.diff.clone(), self.options)
        else {
            return;
        };
        let new_ranges = self.ranges(|hunk| &hunk.new);
        let old_ranges = self.ranges(|hunk| &hunk.old);
        let Some(sides) = self.sides.as_mut() else {
            return;
        };
        let (right, left) = (sides.right.clone(), sides.left.clone());
        for (editor, id) in std::mem::take(&mut sides.spacer_blocks) {
            editor.update(cx, |editor, cx| {
                editor.remove_blocks([id].into_iter().collect(), None, cx)
            });
        }
        refit_excerpts(&right, &buffer, new_ranges, options.collapse, cx);
        if let Some(left) = &left {
            let base = diff.read(cx).base_text_buffer().clone();
            refit_excerpts(left, &base, old_ranges, options.collapse, cx);
        }
        let blocks = self.add_spacers(&right, left.as_ref(), cx);
        if let Some(sides) = self.sides.as_mut() {
            sides.spacer_blocks = blocks;
        }
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
