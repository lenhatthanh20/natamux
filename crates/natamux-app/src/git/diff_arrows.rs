//! The revert arrow of the diff (T-0314), as in PhpStorm: the change the cursor is in shows an
//! arrow at its right. Side by side, the last commit's side has `>>` and the working tree's side a
//! revert icon; inline has the revert icon. Each puts the last commit's lines of that change back
//! into the working tree. The Changes tab does the write, so this only asks for it.

use std::ops::Range;
use std::sync::Arc;

use buffer_diff::DiffHunkStatus;
use editor::{Anchor, DiffHunkRenderer, Editor, ToPoint};
use gpui::{Entity, Pixels, WeakEntity};
use text::BufferId;
use ui::{Tooltip, prelude::*};

use super::change_rules::Side;
use super::diff_rules::hunk_at;
use super::diff_view::{DiffEvent, DiffView};

pub const REVERT_TIP: &str = "Revert this change";

struct RevertArrow {
    view: WeakEntity<DiffView>,
    icon: IconName,
}

impl DiffHunkRenderer for RevertArrow {
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
        let view = self.view.clone();
        h_flex()
            .h(line_height)
            .mr_1()
            .px_0p5()
            .rounded_sm()
            .border_1()
            .border_color(cx.theme().colors().border_variant)
            .bg(cx.theme().colors().editor_background)
            .block_mouse_except_scroll()
            .child(
                IconButton::new(("diff-revert", row as usize), self.icon)
                    .icon_size(IconSize::Small)
                    .icon_color(Color::Muted)
                    .disabled(arrow.moving)
                    .tooltip(Tooltip::text(REVERT_TIP))
                    .on_click(move |_, _, cx| {
                        view.update(cx, |view, cx| {
                            view.ask_revert(arrow.index, arrow.version, cx)
                        })
                        .ok();
                    }),
            )
            .into_any_element()
    }
}

/// The hunk an arrow stands for, the read of the texts its rows belong to, and whether a write
/// runs now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Arrow {
    pub index: usize,
    pub version: u64,
    pub moving: bool,
}

impl DiffView {
    /// The editors draw an arrow on the change the cursor is in.
    pub(super) fn add_arrows(
        &self,
        right: &Entity<Editor>,
        left: Option<&Entity<Editor>>,
        cx: &mut Context<Self>,
    ) {
        let view = cx.weak_entity();
        let editors = std::iter::once((right, IconName::Undo))
            .chain(left.map(|left| (left, IconName::ArrowRight)));
        for (editor, icon) in editors {
            let renderer: Arc<dyn DiffHunkRenderer> = Arc::new(RevertArrow {
                view: view.clone(),
                icon,
            });
            editor.update(cx, |editor, cx| {
                editor.set_diff_hunk_renderer(Some(renderer), cx)
            });
        }
    }

    /// The arrow of the hunk at `row` of a buffer (the last commit's or the working tree's).
    /// None while the file cannot be reverted, or while the hunks are not yet those of the shown
    /// texts.
    pub fn arrow_at(&self, buffer: BufferId, row: u32, cx: &App) -> Option<Arrow> {
        if !self.movable || self.hunks_for != Some(self.version) {
            return None;
        }
        let base = self.diff.as_ref()?.read(cx).base_text_buffer().read(cx);
        let side = if base.remote_id() == buffer {
            Side::Old
        } else {
            Side::New
        };
        Some(Arrow {
            index: hunk_at(&self.hunks, side, row)?,
            version: self.version,
            moving: self.moving,
        })
    }

    /// A click on an arrow: the Changes tab reverts the hunk, unless the texts changed since.
    pub fn ask_revert(&mut self, index: usize, version: u64, cx: &mut Context<Self>) {
        if self.moving || self.version != version || self.hunks_for != Some(version) {
            return;
        }
        cx.emit(DiffEvent::RevertHunk { index, version });
    }

    /// Text diffs of files that are not in conflict can be reverted. A render sets it before
    /// `show`.
    pub fn set_movable(&mut self, movable: bool) {
        self.movable = movable;
    }

    /// While a revert or an undo is written, the arrows are off.
    pub fn set_moving(&mut self, moving: bool, cx: &mut Context<Self>) {
        self.moving = moving;
        if let Some(sides) = &self.sides {
            let editors = std::iter::once(&sides.right).chain(sides.left.as_ref());
            for editor in editors {
                editor.update(cx, |_, cx| cx.notify());
            }
        }
    }
}
