//! The 44 px bottom bar of a project page (layout-b 5.10): saved or unsaved, the problem and
//! warning counts, the "..." menu (Reset to default, Import…, Export), Discard and Save.

use gpui::{ClickEvent, Pixels, Point, WeakEntity};
use ui::{ContextMenu, ContextMenuEntry, Tooltip, prelude::*};

use super::project_settings_page::ProjectSettingsPage;
use super::settings_tab_rules::{FileAction, FileMenuEntry, SaveStatus, file_menu, save_status};
use super::settings_text::count_label;
use crate::components::button_style::{button_icon, quiet_button};
use crate::components::colors::accent;
use crate::components::dialog::primary_button;
use crate::components::icons::icon_path;
use crate::components::menu::MenuLayer;
use crate::components::settings_tile::bottom_bar;

fn run(
    action: FileAction,
    page: &mut ProjectSettingsPage,
    window: &mut Window,
    cx: &mut Context<ProjectSettingsPage>,
) {
    match action {
        FileAction::Reset => page.reset(window, cx),
        FileAction::Import => page.import(window, cx),
        FileAction::Export => page.export(window, cx),
    }
}

/// The "..." menu; each entry runs on the page, which may be gone by then.
fn open_file_menu(
    page: WeakEntity<ProjectSettingsPage>,
    draft_ok: bool,
    position: Point<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let menu = ContextMenu::build(window, cx, move |mut menu, _, _| {
        for entry in file_menu(draft_ok) {
            menu = match entry {
                FileMenuEntry::Label(text) => menu.label(text),
                FileMenuEntry::Item {
                    action,
                    label,
                    icon,
                    disabled,
                } => {
                    let page = page.clone();
                    menu.item(
                        ContextMenuEntry::new(label)
                            .custom_icon_path(icon_path(icon))
                            .icon_position(IconPosition::Start)
                            .icon_color(Color::Muted)
                            .disabled(disabled)
                            .handler(move |window, cx| {
                                page.update(cx, |page, cx| run(action, page, window, cx))
                                    .ok();
                            }),
                    )
                }
            };
        }
        menu
    });
    MenuLayer::open(menu, position, window, cx);
}

/// A 12 px icon and a 12 px label side by side.
fn icon_text(
    icon: impl IntoElement,
    text: impl Into<SharedString>,
    color: Color,
) -> impl IntoElement {
    h_flex()
        .flex_none()
        .gap(rems_from_px(6.0_f32))
        .child(icon)
        .child(Label::new(text).size(LabelSize::Small).color(color))
}

impl ProjectSettingsPage {
    /// "Unsaved changes" with the accent dot, or "Saved" with a check; then the counts.
    fn bar_status(&self, cx: &App) -> impl IntoElement + use<> {
        let status = save_status(self.dirty(cx), self.file_broken());
        let mark = match status {
            SaveStatus::Unsaved => div()
                .flex_none()
                .size(rems_from_px(6.0_f32))
                .rounded_full()
                .bg(accent(cx))
                .into_any_element(),
            SaveStatus::Saved => Icon::new(IconName::Check)
                .size(IconSize::XSmall)
                .color(Color::Muted)
                .into_any_element(),
        };
        let errors = self.errors(cx);
        let warnings = self.warnings(cx);
        h_flex()
            .flex_none()
            .gap(rems_from_px(12.0_f32))
            .child(icon_text(mark, status.label(), Color::Muted))
            .when(errors > 0, |bar| {
                let icon = button_icon("circle-alert")
                    .size(IconSize::XSmall)
                    .color(Color::Error);
                bar.child(icon_text(
                    icon,
                    count_label(errors, "problem"),
                    Color::Error,
                ))
            })
            .when(warnings > 0, |bar| {
                let icon = button_icon("triangle-alert")
                    .size(IconSize::XSmall)
                    .color(Color::Warning);
                bar.child(icon_text(
                    icon,
                    count_label(warnings, "warning"),
                    Color::Warning,
                ))
            })
    }

    fn more_button(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let page = cx.weak_entity();
        let draft_ok = self.draft_ok(cx);
        IconButton::new("settings-more", IconName::Ellipsis)
            .icon_size(IconSize::Small)
            .style(ButtonStyle::Outlined)
            .tooltip(Tooltip::text("Reset, import or export"))
            .disabled(self.saving)
            .on_click(move |event: &ClickEvent, window, cx| {
                open_file_menu(page.clone(), draft_ok, event.position(), window, cx);
            })
    }

    /// Under the scroll area once the settings are read; none while reading.
    pub(super) fn render_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.base.as_ref()?;
        let blocked = self.save_blocked(cx);
        let save = primary_button(
            "Save",
            &self.submit,
            blocked.is_some() || self.saving,
            cx.listener(|this, _, window, cx| this.save(window, cx)),
        )
        .tooltip(Tooltip::text(blocked.unwrap_or("Save .natamux.json")));
        let dirty = self.dirty(cx);
        let discard = quiet_button("settings-discard", "Discard")
            .track_focus(&self.discard)
            .disabled(!dirty || self.saving)
            .when(!dirty, |button| {
                button.tooltip(Tooltip::text("Nothing to discard"))
            })
            .on_click(cx.listener(|this, _, window, cx| this.request_discard(window, cx)));
        Some(
            bottom_bar(cx)
                .child(self.bar_status(cx))
                .child(div().flex_1())
                .child(self.more_button(cx))
                .child(discard)
                .child(save)
                .into_any_element(),
        )
    }
}
