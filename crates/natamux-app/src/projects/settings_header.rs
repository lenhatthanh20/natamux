//! The top of Project settings (T-0189): "Projects / <name>" with the close button, then the tab
//! row. The body scrolls under them. The path of `.natamux.json` sits in the bottom bar, with the
//! "..." menu of the actions on the file.

use gpui::{ClickEvent, FontWeight, Pixels, Point, WeakEntity};
use ui::{ContextMenu, ContextMenuEntry, Tab, Tooltip, prelude::*};

use super::project_settings_page::ProjectSettingsPage;
use super::settings_tab_rules::{SettingsTab, export_reason};
use super::settings_text::{file_hint, home_folder, short_path};
use crate::components::menu::MenuLayer;

type Command = fn(&mut ProjectSettingsPage, &mut Window, &mut Context<ProjectSettingsPage>);

/// A menu entry that runs `command` on the page, which may be gone by then.
fn entry(
    label: &'static str,
    page: &WeakEntity<ProjectSettingsPage>,
    command: Command,
) -> ContextMenuEntry {
    let page = page.clone();
    ContextMenuEntry::new(label).handler(move |window, cx| {
        page.update(cx, |this, cx| command(this, window, cx)).ok();
    })
}

/// The "..." menu: Reset to default, Import and Export. Export says why it is off.
fn open_more(
    page: WeakEntity<ProjectSettingsPage>,
    cannot_export: Option<&'static str>,
    position: Point<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let menu = ContextMenu::build(window, cx, move |menu, _, _| {
        let menu = menu
            .item(entry("Reset to default", &page, ProjectSettingsPage::reset))
            .item(entry("Import\u{2026}", &page, ProjectSettingsPage::import))
            .item(
                entry("Export", &page, ProjectSettingsPage::export)
                    .disabled(cannot_export.is_some()),
            );
        match cannot_export {
            Some(reason) => menu.label(reason),
            None => menu,
        }
    });
    MenuLayer::open(menu, position, window, cx);
}

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

    /// The "..." button of the bottom bar, with the menu of the actions on the file.
    pub(super) fn more_button(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let page = cx.weak_entity();
        let cannot_export = export_reason(self.draft_ok(cx));
        IconButton::new("settings-more", IconName::Ellipsis)
            .icon_size(IconSize::Small)
            .tooltip(Tooltip::text("Reset, import or export"))
            .disabled(self.base.is_none())
            .on_click(move |event: &ClickEvent, window, cx| {
                open_more(page.clone(), cannot_export, event.position(), window, cx);
            })
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
