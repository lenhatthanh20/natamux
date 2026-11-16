//! The buttons at the right of the change the cursor is in (T-0314, T-0316). The working tree's
//! side has Comment (the comment box, quoting the change's lines) and the revert icon; the last
//! commit's side of side by side has only `>>`, which reverts too. `diff_view.rs` places them.

use gpui::{Pixels, WeakEntity};
use ui::{Tooltip, prelude::*};

use super::diff_arrows::Arrow;
use super::diff_view::DiffView;
use crate::app::shortcuts::CommentOnDiff;

pub const REVERT_TIP: &str = "Revert this change";
pub const COMMENT_TIP: &str = "Comment on this change";

/// The buttons of the change `arrow` stands for, one row high. `new_side`: the working tree's
/// side, where Comment shows and the revert icon is Undo, not `>>`. `row` keeps the ids apart.
pub(super) fn hunk_buttons(
    view: WeakEntity<DiffView>,
    new_side: bool,
    arrow: Arrow,
    row: usize,
    line_height: Pixels,
    cx: &App,
) -> AnyElement {
    let icon = if new_side {
        IconName::Undo
    } else {
        IconName::ArrowRight
    };
    let weak = view.clone();
    let revert = IconButton::new(("diff-revert", row), icon)
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
    let comment = new_side.then(|| {
        IconButton::new(("diff-comment", row), IconName::Chat)
            .icon_size(IconSize::Small)
            .icon_color(Color::Muted)
            .tooltip(Tooltip::text(COMMENT_TIP))
            .on_click(move |_, window, cx| {
                view.update(cx, |view, cx| view.comment_hunk(arrow.index, window, cx))
                    .ok();
            })
    });
    h_flex()
        .h(line_height)
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

impl DiffView {
    /// Comment: the change's working-tree lines are picked (the line after it for a deletion),
    /// and the Changes tab opens the comment box with them, as Mod+Shift+A does.
    pub fn comment_hunk(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(hunk), Some(diff)) = (self.hunks.get(index), &self.diff) else {
            return;
        };
        // The last line with text: a final line break leaves an empty row at the end.
        let breaks = diff.new.matches('\n').count() as u32;
        let last_line = (breaks + u32::from(!diff.new.ends_with('\n') && !diff.new.is_empty()))
            .max(1);
        let first = (hunk.new.start + 1).min(last_line);
        let last = hunk.new.end.clamp(first, last_line);
        self.select_lines(first, last, window, cx);
        if let Some(sides) = &self.sides {
            sides.right.focus(window, cx);
        }
        window.dispatch_action(Box::new(CommentOnDiff), cx);
    }
}

#[cfg(test)]
#[path = "diff_hunk_tests.rs"]
mod tests;
