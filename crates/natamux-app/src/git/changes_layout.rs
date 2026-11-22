//! The two layouts of the Changes screen: the file panel beside the diff when main is wide, and
//! everything in one column (a file picker on top) when it is narrow.

use ui::prelude::*;

use super::ChangesView;
use crate::git::diff_rules::files_width;

impl ChangesView {
    pub(super) fn render_wide(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let main = self.width.read(cx).unwrap_or(0.);
        let panel = files_width(self.dashboard.read(cx).view().files_width, main);
        h_flex()
            .flex_1()
            .min_h_0()
            .child(
                v_flex()
                    .w(px(panel))
                    .flex_none()
                    .h_full()
                    .border_r_1()
                    .border_color(cx.theme().colors().border)
                    .child(self.render_files(panel, window, cx))
                    .child(self.render_commit_box(window, cx)),
            )
            .child(self.resize_handle(panel))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(self.render_toolbar(cx))
                    .child(self.render_body(window, cx))
                    .child(self.render_comment_box(window, cx)),
            )
            .into_any_element()
    }

    pub(super) fn render_narrow(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        v_flex()
            .flex_1()
            .min_h_0()
            .child(self.render_picker(cx))
            .child(self.render_toolbar(cx))
            .child(self.render_body(window, cx))
            .child(self.render_comment_box(window, cx))
            .child(self.render_commit_box(window, cx))
            .into_any_element()
    }
}
