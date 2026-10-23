//! The tabs of Project settings (`ProjectSettingsDialog.tsx`): Stages, Issues and JSON over one
//! draft, the tab bar, the body of each tab, and the alerts and the footer count around them.

use gpui::{Entity, FocusHandle, KeyDownEvent};
use ui::{Tab, TabBar, TabPosition, prelude::*};

use super::issues_tab::IssuesTab;
use super::json_tab::JsonTab;
use super::project_settings_dialog::{ProjectSettingsDialog, StagesTab};
use super::settings_tab_rules::{SettingsTab, error_count, shown_tab, step_tab};
use super::settings_text::count_label;
use crate::stages::kind_icon::problem_icon;
use crate::stages::pipeline_editor::PipelineEditor;

/// The tab the person picked, and the two tabs that are not Stages.
pub(super) struct SettingsTabs {
    pub(super) picked: SettingsTab,
    /// The tab bar is one tab stop; the arrows move over the tabs.
    pub(super) focus: FocusHandle,
    pub(super) issues: Entity<IssuesTab>,
    pub(super) json: Entity<JsonTab>,
}

impl SettingsTabs {
    pub(super) fn new(
        editor: &Entity<PipelineEditor>,
        window: &mut Window,
        cx: &mut Context<ProjectSettingsDialog>,
    ) -> Self {
        let pipeline = editor.clone();
        SettingsTabs {
            picked: SettingsTab::Stages,
            focus: cx.focus_handle().tab_stop(true),
            issues: cx.new(|cx| IssuesTab::new(editor.clone(), cx)),
            json: cx.new(|cx| JsonTab::new(pipeline, window, cx)),
        }
    }

    /// The Tab stops of the tab bar and of the tab shown, other than Stages (which has its own).
    pub(super) fn stops(&self, shown: SettingsTab, cx: &App) -> Vec<FocusHandle> {
        let mut stops = vec![self.focus.clone()];
        match shown {
            SettingsTab::Stages => {}
            SettingsTab::Issues => stops.push(self.issues.read(cx).stop()),
            SettingsTab::Json => stops.push(self.json.read(cx).stop(cx)),
        }
        stops
    }
}

impl ProjectSettingsDialog {
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

    pub fn pick_tab(&mut self, tab: SettingsTab, cx: &mut Context<Self>) {
        let Some(stages) = self.stages.as_mut() else {
            return;
        };
        stages.tabs.picked = tab;
        stages
            .editor
            .update(cx, |editor, cx| editor.set_picking(false, cx));
        cx.notify();
    }

    pub(super) fn render_tab_bar(&self, shown: SettingsTab, cx: &mut Context<Self>) -> AnyElement {
        let Some(stages) = &self.stages else {
            return div().into_any_element();
        };
        let selected = shown.index();
        let last = SettingsTab::ALL.len() - 1;
        let tabs = SettingsTab::ALL.iter().enumerate().map(|(index, tab)| {
            let tab = *tab;
            let position = match index {
                0 => TabPosition::First,
                _ if index == last => TabPosition::Last,
                _ => TabPosition::Middle(index.cmp(&selected)),
            };
            Tab::new(SharedString::from(format!(
                "project-settings-{}",
                tab.label()
            )))
            .position(position)
            .toggle_state(tab == shown)
            .child(Label::new(tab.label()))
            .on_click(cx.listener(move |this, _, _, cx| this.pick_tab(tab, cx)))
        });
        div()
            .track_focus(&stages.tabs.focus)
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                if let Some(next) = step_tab(shown, &event.keystroke.key) {
                    cx.stop_propagation();
                    this.pick_tab(next, cx);
                }
            }))
            .child(TabBar::new("project-settings-tabs").children(tabs))
            .into_any_element()
    }

    /// The parts of the body under the tab bar. Stages has two (a shot scrolls to the second).
    pub(super) fn render_tab_body(
        &self,
        stages: &StagesTab,
        shown: SettingsTab,
        window: &Window,
        cx: &mut App,
    ) -> Vec<AnyElement> {
        match shown {
            SettingsTab::Stages => self.render_stages(stages, window, cx).into(),
            SettingsTab::Issues => vec![stages.tabs.issues.clone().into_any_element()],
            SettingsTab::Json => vec![stages.tabs.json.clone().into_any_element()],
        }
    }

    pub(super) fn render_alerts(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let line = |text: String| {
            h_flex()
                .gap_1p5()
                .items_start()
                .child(problem_icon().size(IconSize::XSmall).color(Color::Error))
                .child(Label::new(text).size(LabelSize::Small).color(Color::Error))
        };
        let reading = self.base.is_none() && self.read_error.is_none();
        v_flex()
            .gap_2()
            .when(reading, |this| {
                this.child(Label::new("Reading the settings\u{2026}").color(Color::Muted))
            })
            .children(self.read_error.clone().map(line))
            .children(
                self.base
                    .as_ref()
                    .and_then(|base| base.broken.clone())
                    .map(|broken| {
                        line(format!(
                            "The file has a problem, so the defaults apply: {broken}"
                        ))
                    }),
            )
            .when(self.conflict, |this| this.child(self.conflict_box(cx)))
            .children(self.problem.clone().map(line))
    }

    fn conflict_box(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .flex_wrap()
            .gap_2()
            .px_3()
            .py_2()
            .rounded_lg()
            .border_1()
            .border_color(cx.theme().status().warning_border)
            .bg(cx.theme().status().warning_background)
            .child(
                div().min_w_0().flex_1().child(
                    Label::new(
                        "The file changed on disk since you opened it. Reload drops your changes \
                         here; Overwrite replaces the file with them.",
                    )
                    .color(Color::Default),
                ),
            )
            .child(
                Button::new("settings-reload", "Reload")
                    .style(ButtonStyle::Outlined)
                    .on_click(cx.listener(|this, _, _, cx| this.reload(cx))),
            )
            .child(
                Button::new("settings-overwrite", "Overwrite")
                    .style(ButtonStyle::Tinted(ui::TintColor::Error))
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

    pub(super) fn footer_status(&self, cx: &App) -> impl IntoElement {
        let warnings = self
            .stages
            .as_ref()
            .map_or(0, |stages| stages.editor.read(cx).warnings());
        let errors = self.errors(cx);
        h_flex()
            .gap_2()
            .when(errors > 0, |this| {
                this.child(
                    Label::new(count_label(errors, "problem"))
                        .size(LabelSize::Small)
                        .color(Color::Error),
                )
            })
            .when(warnings > 0, |this| {
                this.child(
                    Label::new(count_label(warnings, "warning"))
                        .size(LabelSize::Small)
                        .color(Color::Warning),
                )
            })
    }

    pub(super) fn ghost(
        &self,
        id: &'static str,
        label: &'static str,
        on_click: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> Button {
        Button::new(id, label)
            .style(ButtonStyle::Subtle)
            .disabled(self.base.is_none())
            .on_click(cx.listener(move |this, _, window, cx| on_click(this, window, cx)))
    }
}
