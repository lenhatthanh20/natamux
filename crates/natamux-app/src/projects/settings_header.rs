//! The top of Project settings (T-0189): "Projects / <name>" with the close button, then the tab
//! row. The body scrolls under them. The path of `.natamux.json` sits in the bottom bar, with the
//! buttons that act on the file.

use gpui::{ClickEvent, FontWeight};
use ui::{Tab, Tooltip, prelude::*};

use super::project_settings_page::ProjectSettingsPage;
use super::settings_tab_rules::SettingsTab;
use super::settings_text::{file_hint, home_folder, short_path};

impl ProjectSettingsPage {
    /// The path in the buffer font (home as `~`), with the full path and why to commit it on
    /// hover. None until the file is read.
    pub(super) fn path_label(&self, cx: &App) -> Option<AnyElement> {
        let path = &self.base.as_ref()?.path;
        let label = Label::new(short_path(path, home_folder().as_deref()))
            .size(LabelSize::Small)
            .color(Color::Muted)
            .buffer_font(cx)
            .truncate();
        Some(
            div()
                .id("settings-file-path")
                .min_w_0()
                .overflow_hidden()
                .tooltip(Tooltip::text(file_hint(path)))
                .child(label)
                .into_any_element(),
        )
    }

    /// The header row, then the tab row.
    pub(super) fn render_header(&self, shown: SettingsTab, cx: &mut Context<Self>) -> AnyElement {
        let close = IconButton::new("settings-close", IconName::Close)
            .icon_size(IconSize::Small)
            .tooltip(Tooltip::text("Close settings"))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.dashboard
                    .update(cx, |dashboard, cx| dashboard.close_settings(cx))
            }));
        let name = Label::new(self.repo_name.clone())
            .weight(FontWeight::SEMIBOLD)
            .truncate();
        let title = h_flex()
            .min_w_0()
            .gap_1()
            .child(Label::new("Projects /").color(Color::Muted))
            .child(div().min_w_0().child(name));
        let tabs = self.render_tab_bar(shown, cx);
        let row = h_flex()
            .flex_none()
            .h(Tab::container_height(cx))
            .pl_6()
            .pr_2()
            .gap_3()
            .border_b_1()
            .border_color(cx.theme().colors().border_variant)
            .child(title)
            .child(div().flex_1())
            .child(close);
        v_flex()
            .flex_none()
            .child(row)
            .child(tabs)
            .into_any_element()
    }
}
