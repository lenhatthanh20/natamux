//! The Settings dialog (docs/ui-spec.md 4.34, `SettingsDialog.tsx`): theme, UI size, the
//! terminal font and a Settings button per project. Every change applies at once; there is no
//! footer.

use std::rc::Rc;

use gpui::{
    DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, FontWeight, KeyDownEvent,
    Subscription,
};
use ui::{ToggleButtonGroup, ToggleButtonGroupStyle, ToggleButtonSimple, Tooltip, prelude::*};

use super::appearance_rules::{ThemeChoice, UiSize, parse_theme, parse_ui_size};
use super::dashboard::Dashboard;
use super::shortcuts::AddProject;
use crate::components::dialog::{DialogFrame, DialogLayer, DialogSize, DialogView};
use crate::components::input_rules::segment_step;
use crate::projects::project_settings_dialog;
use crate::terminals::terminal_font_dialog;

/// Opens the dialog.
pub fn open(dashboard: &Entity<Dashboard>, window: &mut Window, cx: &mut App) {
    let dashboard = dashboard.clone();
    let view = cx.new(|cx| SettingsDialog::new(dashboard, cx));
    DialogLayer::open(view.clone(), window, cx);
    let focus = view.read(cx).theme_focus.clone();
    window.focus(&focus, cx);
}

pub struct SettingsDialog {
    dashboard: Entity<Dashboard>,
    focus: FocusHandle,
    theme_focus: FocusHandle,
    size_focus: FocusHandle,
    font_focus: FocusHandle,
    /// One stop per project button, or the one of "Add a project". Grown while drawing.
    project_stops: Vec<FocusHandle>,
    _follow: Subscription,
}

impl SettingsDialog {
    fn new(dashboard: Entity<Dashboard>, cx: &mut Context<Self>) -> Self {
        // The segmented controls show what the dashboard holds, also when the palette changes it.
        let follow = cx.observe(&dashboard, |_, _, cx| cx.notify());
        Self {
            dashboard,
            focus: cx.focus_handle(),
            theme_focus: cx.focus_handle().tab_stop(true),
            size_focus: cx.focus_handle().tab_stop(true),
            font_focus: cx.focus_handle(),
            project_stops: Vec::new(),
            _follow: follow,
        }
    }
}

/// Text-only segmented control. Arrow keys and Home / End pick, as in `Segmented`.
fn segment<const N: usize>(
    name: &'static str,
    focus: &FocusHandle,
    labels: [&'static str; N],
    selected: usize,
    on_change: impl Fn(usize, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let on_change = Rc::new(on_change);
    let buttons: [ToggleButtonSimple; N] = std::array::from_fn(|index| {
        let pick = on_change.clone();
        ToggleButtonSimple::new(labels[index], move |_, window, cx| pick(index, window, cx))
            .selected(index == selected)
    });
    div()
        .id(name)
        .flex_none()
        .track_focus(focus)
        .on_key_down(move |event: &KeyDownEvent, window, cx| {
            if let Some(next) = segment_step(N, selected, &event.keystroke.key) {
                if next != selected {
                    on_change(next, window, cx);
                }
                cx.stop_propagation();
            }
        })
        .child(
            ToggleButtonGroup::single_row(name, buttons)
                .style(ToggleButtonGroupStyle::Outlined)
                .selected_index(selected)
                .auto_width(),
        )
}

fn row(label: impl Into<SharedString>, control: impl IntoElement) -> Div {
    h_flex()
        .min_h(px(28.))
        .gap_3()
        .justify_between()
        .child(Label::new(label))
        .child(control)
}

fn heading(text: &'static str) -> Label {
    Label::new(text)
        .size(LabelSize::XSmall)
        .weight(FontWeight::MEDIUM)
        .color(Color::Muted)
}

impl SettingsDialog {
    fn appearance(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let view = self.dashboard.read(cx).view();
        let theme = ThemeChoice::ALL
            .iter()
            .position(|choice| *choice == parse_theme(&view.theme))
            .unwrap_or(0);
        let size = UiSize::ALL
            .iter()
            .position(|choice| *choice == parse_ui_size(&view.ui_size))
            .unwrap_or(0);
        let (themes, sizes) = (self.dashboard.clone(), self.dashboard.clone());
        let font = cx.listener(|this, _, window, cx| {
            // Opening it replaces this dialog.
            terminal_font_dialog::open(&this.dashboard, window, cx)
        });
        v_flex()
            .gap_1()
            .child(heading("Appearance"))
            .child(row(
                "Theme",
                segment(
                    "settings-theme",
                    &self.theme_focus,
                    ThemeChoice::ALL.map(ThemeChoice::label),
                    theme,
                    move |index, _, cx| {
                        let name = ThemeChoice::ALL[index].name();
                        themes.update(cx, |dashboard, cx| dashboard.set_theme(name, cx))
                    },
                ),
            ))
            .child(row(
                "UI size",
                segment(
                    "settings-ui-size",
                    &self.size_focus,
                    UiSize::ALL.map(UiSize::label),
                    size,
                    move |index, _, cx| {
                        let name = UiSize::ALL[index].name();
                        sizes.update(cx, |dashboard, cx| dashboard.set_ui_size(name, cx))
                    },
                ),
            ))
            .child(row(
                "Terminal",
                // Stand-in: the Lucide `type` icon is not in the app yet.
                Button::new("settings-terminal-font", "Terminal font…")
                    .style(ButtonStyle::Outlined)
                    .start_icon(Icon::new(IconName::FontSize).size(IconSize::Small))
                    .track_focus(&self.font_focus)
                    .on_click(font),
            ))
    }

    fn projects(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let repos: Vec<(String, String)> = self
            .dashboard
            .read(cx)
            .data
            .repos
            .iter()
            .map(|repo| (repo.id.clone(), repo.name.clone()))
            .collect();
        while self.project_stops.len() < repos.len().max(1) {
            self.project_stops.push(cx.focus_handle());
        }
        let body = if repos.is_empty() {
            row(
                "No projects yet",
                Button::new("settings-add-project", "Add a project")
                    .style(ButtonStyle::Outlined)
                    .size(ButtonSize::Compact)
                    .track_focus(&self.project_stops[0])
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(AddProject), cx)),
            )
            .into_any_element()
        } else {
            let rows = repos.into_iter().enumerate().map(|(index, (id, name))| {
                let tip = format!("Settings of {name}");
                let button = Button::new(("settings-project", index), "Settings")
                    .style(ButtonStyle::Subtle)
                    .size(ButtonSize::Compact)
                    .tooltip(Tooltip::text(tip))
                    .track_focus(&self.project_stops[index])
                    .on_click(cx.listener(move |this, _, window, cx| {
                        // Opening it replaces this dialog.
                        project_settings_dialog::open(&this.dashboard, &id, window, cx);
                    }));
                row(name, button)
            });
            v_flex().children(rows).into_any_element()
        };
        v_flex().gap_1().child(heading("Projects")).child(body)
    }
}

impl EventEmitter<DismissEvent> for SettingsDialog {}

impl Focusable for SettingsDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl DialogView for SettingsDialog {
    fn size(&self) -> DialogSize {
        DialogSize::Medium
    }

    fn tab_ring(&self, cx: &App) -> Vec<FocusHandle> {
        let repos = self.dashboard.read(cx).data.repos.len().max(1);
        let mut ring = vec![
            self.theme_focus.clone(),
            self.size_focus.clone(),
            self.font_focus.clone(),
        ];
        ring.extend(self.project_stops.iter().take(repos).cloned());
        ring
    }

    fn submit(&mut self, _: &mut Window, _: &mut Context<Self>) {}
}

impl Render for SettingsDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let close = cx.listener(|_, _, _, cx| cx.emit(DismissEvent));
        let appearance = self.appearance(cx);
        let projects = self.projects(cx);
        div().track_focus(&self.focus).child(
            DialogFrame::new("Settings", close)
                .child(appearance)
                .child(projects),
        )
    }
}
