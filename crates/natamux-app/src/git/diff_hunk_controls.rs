//! The buttons at the right of the change the cursor is in (T-0314, T-0316). The working tree's
//! side has Comment (the comment box, quoting the change's lines) and the revert icon; the last
//! commit's side of side by side has only `>>`, which reverts too.

use std::ops::Range;

use buffer_diff::DiffHunkStatus;
use editor::{Anchor, DiffHunkRenderer, Editor, ToPoint};
use gpui::{Entity, Focusable, Pixels, WeakEntity};
use ui::{Tooltip, prelude::*};

use super::diff_view::DiffView;
use crate::app::shortcuts::CommentOnDiff;

pub const REVERT_TIP: &str = "Revert this change";
pub const COMMENT_TIP: &str = "Comment on this change";

pub(super) struct HunkControls {
    pub(super) view: WeakEntity<DiffView>,
    /// The working tree's side: Comment shows, and the revert icon is Undo, not `>>`.
    pub(super) new_side: bool,
}

impl DiffHunkRenderer for HunkControls {
    fn render_hunk_controls(
        &self,
        row: u32,
        _status: &DiffHunkStatus,
        hunk_range: Range<Anchor>,
        _is_created_file: bool,
        line_height: Pixels,
        editor: &Entity<Editor>,
        _window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let Some(view) = self.view.upgrade() else {
            return gpui::Empty.into_any_element();
        };
        let multi = editor.read(cx).buffer().read(cx).snapshot(cx);
        let start = hunk_range.start.to_point(&multi);
        let Some((buffer, point)) = multi.point_to_buffer_point(start) else {
            return gpui::Empty.into_any_element();
        };
        let Some(arrow) = view.read(cx).arrow_at(buffer.remote_id(), point.row, cx) else {
            return gpui::Empty.into_any_element();
        };
        let icon = if self.new_side {
            IconName::Undo
        } else {
            IconName::ArrowRight
        };
        let weak = self.view.clone();
        let revert = IconButton::new(("diff-revert", row as usize), icon)
            .icon_size(IconSize::Small)
            .icon_color(Color::Muted)
            .disabled(arrow.moving)
            .tooltip(Tooltip::text(REVERT_TIP))
            .on_click(move |_, _, cx| {
                weak.update(cx, |view, cx| {
                    view.ask_revert(arrow.index, arrow.version, cx)
                })
                .ok();
            });
        let weak = self.view.clone();
        let comment = self.new_side.then(|| {
            IconButton::new(("diff-comment", row as usize), IconName::Chat)
                .icon_size(IconSize::Small)
                .icon_color(Color::Muted)
                .tooltip(Tooltip::text(COMMENT_TIP))
                .on_click(move |_, window, cx| {
                    weak.update(cx, |view, cx| view.comment_hunk(arrow.index, window, cx))
                        .ok();
                })
        });
        h_flex()
            .h(line_height)
            .mr_1()
            .px_0p5()
            .gap_0p5()
            .rounded_sm()
            .border_1()
            .border_color(cx.theme().colors().border_variant)
            .bg(cx.theme().colors().editor_background)
            .block_mouse_except_scroll()
            .children(comment)
            .child(revert)
            .into_any_element()
    }
}

impl DiffView {
    /// Comment: the change's working-tree lines are picked (the line after it for a deletion),
    /// and the Changes tab opens the comment box with them, as Mod+Shift+A does.
    pub fn comment_hunk(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(hunk), Some(buffer)) = (self.hunks.get(index), &self.buffer) else {
            return;
        };
        // The last line with text: a final line break leaves an empty row at the end.
        let end = buffer.read(cx).max_point();
        let last_line = (end.row + u32::from(end.column > 0)).max(1);
        let first = (hunk.new.start + 1).min(last_line);
        let last = hunk.new.end.clamp(first, last_line);
        self.select_lines(first, last, window, cx);
        if let Some(sides) = &self.sides {
            window.focus(&sides.right.read(cx).focus_handle(cx), cx);
        }
        window.dispatch_action(Box::new(CommentOnDiff), cx);
    }
}

#[cfg(test)]
#[path = "diff_hunk_tests.rs"]
mod tests;
