//! The read-only editors of the diff view: their excerpts (the whole file, or 3 lines around
//! each change), their settings (no runnables, breakpoints or hunk buttons), and the empty rows
//! that keep the two sides of a side by side diff level.

use std::ops::Range;
use std::sync::Arc;

use editor::display_map::{
    BlockContext, BlockPlacement, BlockProperties, BlockStyle, CustomBlockId, RenderBlock,
};
use editor::{Anchor, Editor, HiddenDiffHunkRenderer, MultiBuffer};
use gpui::Entity;
use language::Buffer;
use language::language_settings::SoftWrap;
use text::Point;
use ui::prelude::*;

use super::diff_rules::Spacer;

/// Lines kept around each change while unchanged lines are collapsed (Monaco's context).
const CONTEXT_LINES: u32 = 3;

pub fn set_excerpts(
    multi: &mut MultiBuffer,
    buffer: &Entity<Buffer>,
    ranges: Vec<Range<Point>>,
    collapse: bool,
    cx: &mut Context<MultiBuffer>,
) {
    if collapse && !ranges.is_empty() {
        multi.set_excerpts_for_buffer(buffer.clone(), ranges, CONTEXT_LINES, cx);
    } else {
        let all = Point::zero()..buffer.read(cx).max_point();
        multi.set_excerpts_for_buffer(buffer.clone(), [all], 0, cx);
    }
}

/// The excerpts of an editor's multibuffer again, after a newer read changed the hunks.
pub fn refit_excerpts(
    editor: &Entity<Editor>,
    buffer: &Entity<Buffer>,
    ranges: Vec<Range<Point>>,
    collapse: bool,
    cx: &mut App,
) {
    let multi = editor.read(cx).buffer().clone();
    multi.update(cx, |multi, cx| {
        set_excerpts(multi, buffer, ranges, collapse, cx)
    });
}

/// A read-only editor of `multi`, which shows `buffer`. The file's name is in the toolbar, so the
/// editor has no buffer header ("untitled", as the buffer has no file).
pub fn read_only_editor(
    multi: Entity<MultiBuffer>,
    buffer: &Entity<Buffer>,
    wrap: bool,
    window: &mut Window,
    cx: &mut App,
) -> Entity<Editor> {
    let buffer_id = buffer.read(cx).remote_id();
    cx.new(|cx| {
        let mut editor = Editor::for_multibuffer(multi, None, window, cx);
        editor.disable_header_for_buffer(buffer_id, cx);
        editor.set_read_only(true);
        editor.set_diff_hunk_renderer(Some(Arc::new(HiddenDiffHunkRenderer)), cx);
        plain_switches(&mut editor, wrap, window, cx);
        editor
    })
}

/// The switches every editor of the app shares: no runnables, breakpoints, code actions, guides,
/// predictions or minimap, and the soft wrap.
pub fn plain_switches(
    editor: &mut Editor,
    wrap: bool,
    window: &mut Window,
    cx: &mut Context<Editor>,
) {
    editor.set_show_runnables(false, cx);
    editor.set_show_breakpoints(false, cx);
    editor.set_show_code_actions(false, cx);
    editor.set_show_bookmarks(false, cx);
    editor.set_show_diff_review_button(false, cx);
    editor.set_show_wrap_guides(false, cx);
    editor.set_show_indent_guides(false, cx);
    editor.set_show_edit_predictions(Some(false), window, cx);
    editor.set_minimap_visibility(editor::MinimapVisibility::Disabled, window, cx);
    editor.set_soft_wrap_mode(
        if wrap {
            SoftWrap::EditorWidth
        } else {
            SoftWrap::None
        },
        cx,
    );
}

/// Empty rows above `before_row` of one side, or below its last row at the end of the file (or
/// when no excerpt holds the row). The id lets a newer read of the file take the rows away again.
pub fn add_spacer(
    editor: &Entity<Editor>,
    buffer: &Entity<Buffer>,
    spacer: Spacer,
    cx: &mut App,
) -> Option<CustomBlockId> {
    let last_row = buffer.read(cx).max_point().row;
    editor.update(cx, |editor, cx| {
        let row = spacer.before_row.min(last_row);
        let anchor =
            editor
                .buffer()
                .read(cx)
                .buffer_point_to_anchor(buffer, Point::new(row, 0), cx);
        // No excerpt holds the row: below the end still keeps the two sides level.
        let placement = match anchor {
            Some(anchor) if spacer.before_row <= last_row => BlockPlacement::Above(anchor),
            Some(anchor) => BlockPlacement::Below(anchor),
            None => BlockPlacement::Below(Anchor::Max),
        };
        editor
            .insert_blocks(
                [BlockProperties {
                    placement,
                    height: Some(spacer.rows),
                    style: BlockStyle::Spacer,
                    render: empty_rows(),
                    priority: 0,
                }],
                None,
                cx,
            )
            .pop()
    })
}

/// Empty rows as tall as their block. The editor resizes a block to what it draws once it shows,
/// so an empty element would shrink the block to one row (T-0230).
pub fn empty_rows() -> RenderBlock {
    Arc::new(|cx: &mut BlockContext| {
        div()
            .h(cx.line_height * cx.height as f32)
            .into_any_element()
    })
}
