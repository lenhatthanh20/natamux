//! The 72 px header of a project page (layout-b 5): the letter avatar, the name, the summary with
//! the path of `.natamux.json`, the General / Stages / JSON segmented tabs and the close button.

use ui::prelude::*;

use super::project_settings_page::ProjectSettingsPage;
use super::settings_tab_rules::{JSON_FIRST, SettingsTab};
use super::settings_text::{SUMMARY_NOTE, file_hint, home_folder, short_path};
use crate::components::segmented::Segmented;
use crate::components::settings_tile::{PageHeader, SettingsTile, TileSize};

/// Each tab's width while some are off, so their covers line up.
const BLOCKED_TAB_WIDTH: f32 = 82.;

/// The icon of a tab. Zed has no sliders, layout or braces icon: these stand in.
fn tab_icon(tab: SettingsTab) -> IconName {
    match tab {
        SettingsTab::General => IconName::Settings,
        SettingsTab::Stages => IconName::ListTree,
        SettingsTab::Json => IconName::FileCode,
    }
}

impl ProjectSettingsPage {
    /// The mono path (home as `~`) and the note, in one line that truncates.
    fn summary(&self, cx: &App) -> Option<(AnyElement, String)> {
        let path = &self.base.as_ref()?.path;
        let shown = Label::new(short_path(path, home_folder().as_deref()))
            .size(LabelSize::Custom(rems_from_px(11_f32)))
            .color(Color::Muted)
            .buffer_font(cx)
            .truncate();
        let note = Label::new(SUMMARY_NOTE)
            .size(LabelSize::Small)
            .color(Color::Muted);
        let line = h_flex()
            .min_w_0()
            .overflow_hidden()
            .child(div().min_w_0().overflow_hidden().child(shown))
            .child(div().flex_none().child(note));
        Some((line.into_any_element(), file_hint(path)))
    }

    /// The segmented tabs; while the JSON cannot be read only JSON shows and the others say why.
    fn tabs(&self, shown: SettingsTab, cx: &mut Context<Self>) -> Option<AnyElement> {
        let stages = self.stages.as_ref()?;
        let page = cx.weak_entity();
        let option = |tab: SettingsTab| (SharedString::from(tab.label()), tab_icon(tab));
        let options = SettingsTab::ALL.map(option);
        let segmented = Segmented::new(
            "project-settings-tabs",
            &stages.tabs.focus,
            options,
            shown.index(),
            move |index, _, cx| {
                let Some(tab) = SettingsTab::ALL.get(index).copied() else {
                    return;
                };
                page.update(cx, |page, cx| page.pick_tab(tab, cx)).ok();
            },
        );
        // Broken JSON: General and Stages look off and say why (layout-b 5, state "JSON error").
        let segmented = if self.draft_ok(cx) {
            segmented
        } else {
            [SettingsTab::General, SettingsTab::Stages]
                .into_iter()
                .fold(segmented.option_width(BLOCKED_TAB_WIDTH), |tabs, tab| {
                    tabs.disabled(tab.index(), JSON_FIRST)
                })
        };
        Some(div().flex_none().child(segmented).into_any_element())
    }

    /// Tile, name and summary, the tabs once the settings are read, then the close button.
    pub(super) fn render_header(&self, shown: SettingsTab, cx: &mut Context<Self>) -> AnyElement {
        let board = self.dashboard.clone();
        let header = PageHeader::new(
            SettingsTile::avatar(&self.repo_name, TileSize::Header),
            self.repo_name.clone(),
            move |_, cx| board.update(cx, |board, cx| board.close_settings(cx)),
        );
        let header = match self.summary(cx) {
            Some((line, tip)) => header.summary_child(line).summary_tooltip(tip),
            None => header,
        };
        match self.tabs(shown, cx) {
            Some(tabs) => header.end(tabs).into_any_element(),
            None => header.into_any_element(),
        }
    }
}
