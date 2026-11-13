//! The tabs of Project settings (`ProjectSettingsDialog.tsx`): General, Stages and JSON over one
//! draft, the body of each tab, and the alerts above them (layout-b 5.9, 7).

use gpui::{DefiniteLength, Entity, FocusHandle};
use ui::prelude::*;

use super::general_tab::render_general;
use super::issues_tab::IssuesTab;
use super::json_tab::JsonTab;
use super::project_settings_page::{ProjectSettingsPage, StagesTab};
use super::settings_tab_rules::{SettingsTab, error_count, shown_tab, tab_blocked};
use crate::components::button_style::action_button;
use crate::components::settings_card::{SeparatorInset, SettingsCard, foot_text};
use crate::components::settings_tile::{AlertKind, SettingsAlert};
use crate::stages::pipeline_editor::PipelineEditor;

/// The bars of a loading card, as parts of its width.
const SKELETON_WIDTHS: [f32; 3] = [0.6, 0.45, 0.7];

/// The tab the person picked, and the two tabs that are not Stages (General holds the issues).
pub(super) struct SettingsTabs {
    pub(super) picked: SettingsTab,
    /// The segmented tabs are one tab stop; the arrows move over the tabs.
    pub(super) focus: FocusHandle,
    pub(super) issues: Entity<IssuesTab>,
    pub(super) json: Entity<JsonTab>,
}

impl SettingsTabs {
    pub(super) fn new(
        editor: &Entity<PipelineEditor>,
        window: &mut Window,
        cx: &mut Context<ProjectSettingsPage>,
    ) -> Self {
        let pipeline = editor.clone();
        SettingsTabs {
            picked: SettingsTab::Stages,
            focus: cx.focus_handle().tab_stop(true),
            issues: cx.new(|cx| IssuesTab::new(editor.clone(), cx)),
            json: cx.new(|cx| JsonTab::new(pipeline, window, cx)),
        }
    }

    /// The Tab stops of the tabs and of the JSON tab; General and Stages have their own.
    pub(super) fn stops(&self, shown: SettingsTab, cx: &App) -> Vec<FocusHandle> {
        let mut stops = vec![self.focus.clone()];
        match shown {
            SettingsTab::General | SettingsTab::Stages => {}
            SettingsTab::Json => stops.push(self.json.read(cx).stop(cx)),
        }
        stops
    }
}

/// A loading group: a title bar, then a card of three rows of bars.
fn skeleton_group(cx: &App) -> impl IntoElement + use<> {
    let fill = cx.theme().colors().ghost_element_hover;
    let bar = move |width: DefiniteLength| {
        div()
            .h(rems_from_px(10.0_f32))
            .w(width)
            .rounded(rems_from_px(4.0_f32))
            .bg(fill)
    };
    let rows = SKELETON_WIDTHS.map(|width| {
        h_flex()
            .h(rems_from_px(40.0_f32))
            .mx(rems_from_px(12.0_f32))
            .child(bar(relative(width)))
    });
    v_flex()
        .child(
            h_flex()
                .h(rems_from_px(24.0_f32))
                .mt(rems_from_px(20.0_f32))
                .mb(rems_from_px(6.0_f32))
                .px(rems_from_px(12.0_f32))
                .child(bar(rems_from_px(80.0_f32).into())),
        )
        .child(SettingsCard::new(SeparatorInset::Text).children(rows))
}

impl ProjectSettingsPage {
    /// The tab on screen: JSON that cannot be read keeps the person on the JSON tab.
    pub(super) fn shown_tab(&self, cx: &App) -> SettingsTab {
        match &self.stages {
            Some(stages) => shown_tab(
                stages.editor.read(cx).parse_error().is_none(),
                stages.tabs.picked,
            ),
            None => SettingsTab::Stages,
        }
    }

    /// A tab the JSON blocks stays unpicked.
    pub fn pick_tab(&mut self, tab: SettingsTab, cx: &mut Context<Self>) {
        let draft_ok = self.draft_ok(cx);
        let Some(stages) = self.stages.as_mut() else {
            return;
        };
        if tab_blocked(tab, draft_ok).is_some() {
            return;
        }
        stages.tabs.picked = tab;
        cx.notify();
    }

    /// The parts of the body under the alerts. Stages has two (a shot scrolls to the second).
    pub(super) fn render_tab_body(
        &self,
        stages: &StagesTab,
        shown: SettingsTab,
        window: &mut Window,
        cx: &mut App,
    ) -> Vec<AnyElement> {
        match shown {
            SettingsTab::General => vec![render_general(stages)],
            SettingsTab::Stages => self.render_stages(stages, window, cx).into(),
            SettingsTab::Json => vec![stages.tabs.json.clone().into_any_element()],
        }
    }

    /// While reading: the line and two loading groups.
    fn render_reading(&self, cx: &App) -> impl IntoElement + use<> {
        v_flex()
            .child(
                div()
                    .mt(rems_from_px(20.0_f32))
                    .mx(rems_from_px(12.0_f32))
                    .child(foot_text("Reading the settings\u{2026}")),
            )
            .child(skeleton_group(cx))
            .child(skeleton_group(cx))
    }

    /// Each alert only when it applies, in this order: read error, broken file, conflict, repo
    /// trust, save failed.
    pub(super) fn render_alerts(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let reading =
            (self.base.is_none() && self.read_error.is_none()).then(|| self.render_reading(cx));
        let read_error = self.read_error.clone().map(|error| {
            SettingsAlert::new(AlertKind::Error, "Could not read the settings").text(error)
        });
        let broken = self.base.as_ref().and_then(|base| base.broken.clone());
        let broken = broken.map(|broken| {
            SettingsAlert::new(
                AlertKind::Error,
                "The file has a problem, so the defaults apply",
            )
            .text(broken)
        });
        let conflict = self.conflict.then(|| self.conflict_alert(cx));
        let trust = self.trust.read(cx).untrusted().then(|| self.trust.clone());
        let failed = self
            .problem
            .clone()
            .map(|problem| SettingsAlert::new(AlertKind::Error, "Save failed").text(problem));
        v_flex()
            .w_full()
            .children(reading)
            .children(read_error)
            .children(broken)
            .children(conflict)
            .children(trust)
            .children(failed)
    }

    fn conflict_alert(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        SettingsAlert::new(
            AlertKind::Warning,
            "The file changed on disk since you opened it",
        )
        .muted_text("Reload drops your changes here; Overwrite replaces the file with them.")
        .action(
            action_button("settings-reload", "Reload")
                .on_click(cx.listener(|this, _, _, cx| this.reload(cx))),
        )
        .action(
            action_button("settings-overwrite", "Overwrite")
                .on_click(cx.listener(|this, _, _, cx| this.overwrite(cx))),
        )
    }

    /// The pipeline problems, and one more while the text cannot be read.
    pub(super) fn errors(&self, cx: &App) -> usize {
        self.stages.as_ref().map_or(0, |stages| {
            let editor = stages.editor.read(cx);
            error_count(editor.parse_error().is_none(), editor.errors())
        })
    }

    pub(super) fn warnings(&self, cx: &App) -> usize {
        self.stages
            .as_ref()
            .map_or(0, |stages| stages.editor.read(cx).warnings())
    }
}
