//! Test only: the diff view as it was on Zed's editor and `buffer_diff` before S6.2, kept for the
//! goldens recorded on the old code (T-DIFF-1 `diff/rows.json`, T-MOUSE-1 `diff/mouse.json`):
//! their checks still run on it, and their generators can write them again. Only what those
//! need: the texts, the hunks, the editors and the empty rows. It goes with the editor stack
//! (S7).

use std::ops::Range;

use buffer_diff::BufferDiff;
use editor::{Anchor, Editor, MultiBuffer};
use gpui::Entity;
use language::{Buffer, Capability};
use text::Point;
use ui::prelude::*;

use super::change_rules::Side;
use super::diff_editors::{add_spacer, read_only_editor, set_excerpts};
use super::diff_rules::{HunkRows, end_spacer, spacers};
use super::diff_view::DiffOptions;
use super::languages::language_for;
use super::whitespace_rules::diff_base;

pub struct ZedSides {
    pub right: Entity<Editor>,
    pub left: Option<Entity<Editor>>,
}

#[derive(Default)]
pub struct ZedDiff {
    pub buffer: Option<Entity<Buffer>>,
    pub diff: Option<Entity<BufferDiff>>,
    pub hunks: Vec<HunkRows>,
    pub sides: Option<ZedSides>,
    /// The key and the options of the shown texts: the same again kept everything.
    shown: Option<(String, DiffOptions)>,
    _update: Option<gpui::Task<()>>,
}

impl ZedDiff {
    /// Shows `modified` against `original` in new editors, as the old view's first show of a
    /// file did. `key` names the texts: the same key and options again change nothing.
    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &mut self,
        key: &str,
        path: &str,
        original: Option<&str>,
        modified: &str,
        options: DiffOptions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let shown = Some((key.to_owned(), options));
        if self.shown == shown {
            return;
        }
        self.shown = shown;
        let buffer = cx.new(|cx| {
            let mut buffer = Buffer::local(modified, cx);
            buffer.set_capability(Capability::ReadOnly, cx);
            buffer
        });
        let snapshot = buffer.read(cx).text_snapshot();
        let diff = cx.new(|cx| BufferDiff::new(&snapshot, None, None, cx));
        let base = diff_base(original, modified, options.ignore_whitespace);
        let update = diff.update(cx, |diff, cx| diff.set_base_text(base, snapshot, cx));
        self.color_by_language(path, &buffer, &diff, cx);
        self.buffer = Some(buffer);
        self.diff = Some(diff);
        self.sides = None;
        self._update = Some(cx.spawn_in(window, async move |this, cx| {
            update.await;
            this.update_in(cx, |this, window, cx| {
                this.read_hunks(cx);
                this.build(options, window, cx);
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

    fn build(&mut self, options: DiffOptions, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(buffer), Some(diff)) = (self.buffer.clone(), self.diff.clone()) else {
            return;
        };
        let new_ranges: Vec<Range<Point>> = self.ranges(|hunk| &hunk.new);
        let right_buffer = cx.new(|cx| {
            let mut multi = MultiBuffer::new(Capability::ReadOnly);
            set_excerpts(&mut multi, &buffer, new_ranges, options.collapse, cx);
            multi.add_diff(diff.clone(), cx);
            if options.side_by_side {
                multi.set_all_diff_hunks_expanded(cx);
                multi.set_show_deleted_hunks(false, cx);
            } else {
                multi.expand_diff_hunks(vec![Anchor::Min..Anchor::Max], cx);
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
        self.add_spacers(&right, left.as_ref(), cx);
        self.sides = Some(ZedSides { right, left });
        cx.notify();
    }

    /// The empty rows that keep the two sides level; none when inline.
    fn add_spacers(
        &self,
        right: &Entity<Editor>,
        left: Option<&Entity<Editor>>,
        cx: &mut App,
    ) {
        let (Some(left), Some(buffer), Some(diff)) = (left, &self.buffer, &self.diff) else {
            return;
        };
        let base = diff.read(cx).base_text_buffer().clone();
        let old_rows = base.read(cx).max_point().row + 1;
        let new_rows = buffer.read(cx).max_point().row + 1;
        for spacer in spacers(&self.hunks)
            .into_iter()
            .chain(end_spacer(&self.hunks, old_rows, new_rows))
        {
            let (editor, side_buffer) = match spacer.side {
                Side::Old => (left, &base),
                Side::New => (right, buffer),
            };
            add_spacer(editor, side_buffer, spacer, cx);
        }
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
}

impl Render for ZedDiff {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(sides) = &self.sides else {
            return div().size_full().into_any_element();
        };
        let border = cx.theme().colors().border;
        match &sides.left {
            Some(left) => h_flex()
                .key_context("NatamuxDiff")
                .size_full()
                .child(div().flex_1().min_w_0().h_full().child(left.clone()))
                .child(div().w_px().h_full().bg(border))
                .child(div().flex_1().min_w_0().h_full().child(sides.right.clone()))
                .into_any_element(),
            None => div()
                .key_context("NatamuxDiff")
                .size_full()
                .child(sides.right.clone())
                .into_any_element(),
        }
    }
}

#[path = "diff_golden_tests.rs"]
mod golden_tests;

#[path = "zed_diff_mouse_tests.rs"]
mod mouse_tests;

#[path = "zed_diff_tests.rs"]
mod tests;
