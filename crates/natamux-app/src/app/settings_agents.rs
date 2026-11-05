//! The Agents page of Settings (T-0249, layout-b 4.3): the custom agents as a card list, each row
//! opening the agent sheet, and the built-in agents, read only. The list is a draft until Save in
//! the bottom bar sends the whole list.

use gpui::{ClickEvent, Entity, FocusHandle, FontWeight};
use natamux_client::ClientError;
use natamux_proto::agents::custom::CustomAgent;
use ui::{Tooltip, prelude::*};

use super::dashboard::Dashboard;
use super::settings_view::SettingsView;
use super::source::reason;
use crate::agents::agent_choices::{AGENT_CHOICES, custom_agents, custom_agents_known};
use crate::components::button_style::{action_button, button_icon};
use crate::components::colors::{TileColor, accent};
use crate::components::settings_card::{
    SeparatorInset, SettingsCard, SettingsGroup, SettingsRow, foot_text,
};
use crate::components::settings_tile::{
    AlertKind, SettingsAlert, SettingsTile, TileSize, bottom_bar,
};
use crate::components::toast_rules::ToastTone;
use crate::components::toasts::toast;

#[path = "settings_agent_sheet.rs"]
mod agent_sheet;
use agent_sheet::{base_label, default_program, tile};
// For the `--shot` state that opens the sheet.
pub(crate) use agent_sheet::open as open_agent_sheet;

const CUSTOM_FOOT: &str = "Start a built-in agent with your own command, or add any other agent CLI. Changes stay here until you click Save.";
const BUILT_IN_FOOT: &str =
    "Read only. Pick one as \"Based on\" to start it with your own command.";
const NOT_READ: &str = "The custom agents could not be read from the daemon, so Save is off.";
const NOT_READ_TIP: &str = "The custom agents could not be read";

/// The id a row shows: a new row has none yet.
fn row_label(agent: &CustomAgent) -> &str {
    if agent.id.is_empty() {
        "new-agent"
    } else {
        &agent.id
    }
}

/// (base, command) of a row's help line: "Claude Code · cq".
fn row_help(agent: &CustomAgent) -> (String, &str) {
    let command = if agent.command.is_empty() {
        "no command"
    } else {
        &agent.command
    };
    (base_label(&agent.base), command)
}

/// The bottom bar's status line.
fn status_text(changed: bool) -> &'static str {
    if changed {
        "Unsaved changes to custom agents"
    } else {
        "Custom agents saved on the daemon"
    }
}

/// Save is on when the draft differs from the daemon's list, the list was read, and no save runs.
fn save_enabled(changed: bool, saving: bool, known: bool) -> bool {
    changed && !saving && known
}

pub struct AgentRows {
    dashboard: Entity<Dashboard>,
    /// The custom agents as the page shows them; Save sends them.
    draft: Vec<CustomAgent>,
    /// The list the draft was made from, or the last one saved.
    shown: Vec<CustomAgent>,
    saving: bool,
    problem: Option<String>,
    /// One focus stop per row, grown while drawing.
    row_stops: Vec<FocusHandle>,
    add: FocusHandle,
    save: FocusHandle,
}

impl AgentRows {
    pub fn new(dashboard: Entity<Dashboard>, cx: &mut Context<SettingsView>) -> Self {
        Self {
            dashboard,
            draft: Vec::new(),
            shown: Vec::new(),
            saving: false,
            problem: None,
            row_stops: Vec::new(),
            add: cx.focus_handle(),
            save: cx.focus_handle(),
        }
    }

    fn draft(&self) -> &[CustomAgent] {
        &self.draft
    }

    /// The draft differs from the daemon's list: the nav's unsaved dot and the bar's status.
    pub fn unsaved(&self) -> bool {
        self.draft != self.shown
    }

    /// Shows the list the dashboard holds, unless the person changed the draft: the list may come
    /// after the page was made.
    pub fn sync(&mut self, _window: &mut Window, _cx: &mut Context<SettingsView>) {
        let held = custom_agents();
        if held == self.shown || self.saving || self.unsaved() {
            return;
        }
        self.draft = held.clone();
        self.shown = held;
    }

    /// Add agent, the rows, then Save, in the order they are drawn.
    pub fn stops(&self, _cx: &App) -> Vec<FocusHandle> {
        let mut stops = vec![self.add.clone()];
        stops.extend(self.row_stops.iter().take(self.draft.len()).cloned());
        stops.push(self.save.clone());
        stops
    }

    /// The sheet's Done: row `index` changes, or a new row comes last.
    fn put(&mut self, index: Option<usize>, agent: CustomAgent, cx: &mut Context<SettingsView>) {
        match index.and_then(|at| self.draft.get_mut(at)) {
            Some(row) => *row = agent,
            None => self.draft.push(agent),
        }
        cx.notify();
    }

    fn remove(&mut self, index: usize, cx: &mut Context<SettingsView>) {
        if index < self.draft.len() {
            self.draft.remove(index);
            cx.notify();
        }
    }

    /// `PUT /custom-agents`; the daemon's error shows on the page.
    fn save(&mut self, cx: &mut Context<SettingsView>) {
        let Some(source) = self.dashboard.read(cx).source().cloned() else {
            return;
        };
        if self.saving || !custom_agents_known() {
            return;
        }
        self.saving = true;
        self.problem = None;
        let sent = self.draft.clone();
        let answer = source.save_custom_agents(sent.clone());
        cx.spawn(async move |view, cx| {
            let answer = answer.await;
            view.update(cx, |view, cx| view.agents.saved(&sent, answer, cx))
                .ok();
        })
        .detach();
        cx.notify();
    }

    /// `sent` is the draft Save sent: edits made while it ran stay in the draft.
    fn saved(
        &mut self,
        sent: &[CustomAgent],
        answer: Result<Vec<CustomAgent>, ClientError>,
        cx: &mut Context<SettingsView>,
    ) {
        self.saving = false;
        match answer {
            Ok(list) => {
                if self.draft == sent {
                    self.draft = list.clone();
                }
                self.shown = list.clone();
                self.dashboard
                    .update(cx, |dashboard, cx| dashboard.use_custom_agents(list, cx));
                toast("Saved the custom agents.", ToastTone::Success, cx);
            }
            Err(error) => self.problem = Some(reason(&error)),
        }
        cx.notify();
    }

    /// The page body, to sit in the scroll area under the header: alerts, then the two groups.
    pub fn content(&mut self, cx: &mut Context<SettingsView>) -> AnyElement {
        while self.row_stops.len() < self.draft.len() {
            self.row_stops.push(cx.focus_handle());
        }
        let save_failed = self.problem.clone().map(|text| {
            SettingsAlert::new(AlertKind::Error, "Could not save the custom agents").text(text)
        });
        let not_read = (!custom_agents_known()).then(|| {
            SettingsAlert::new(AlertKind::Error, "Could not read the custom agents")
                .muted_text(NOT_READ)
        });
        v_flex()
            .w_full()
            .children(save_failed)
            .children(not_read)
            .child(self.custom_group(cx))
            .child(built_in_group(cx))
            .into_any_element()
    }

    fn custom_group(&self, cx: &mut Context<SettingsView>) -> SettingsGroup {
        let view = cx.entity();
        let add_view = view.clone();
        let add = action_button("agents-add", "Add agent")
            .start_icon(button_icon("plus"))
            .track_focus(&self.add)
            .on_click(move |_: &ClickEvent, window, cx| {
                agent_sheet::open(&add_view, None, window, cx)
            });
        let mut card = SettingsCard::new(SeparatorInset::Tile);
        if self.draft.is_empty() {
            card = card.child(
                SettingsRow::new("agents-none", "No custom agents yet.").label_color(Color::Muted),
            );
        }
        for (index, agent) in self.draft.iter().enumerate() {
            let row_view = view.clone();
            let open = move |window: &mut Window, cx: &mut App| {
                agent_sheet::open(&row_view, Some(index), window, cx)
            };
            let row = custom_row(index, agent, cx).on_press(self.row_stops.get(index), open);
            card = card.child(row);
        }
        SettingsGroup::new("Custom agents")
            .count(self.draft.len().to_string())
            .action(add)
            .child(card)
            .footnote(CUSTOM_FOOT)
    }

    /// The 44 px bar under the page: the status at the left, Save at the right.
    pub fn bottom_bar(&self, cx: &mut Context<SettingsView>) -> AnyElement {
        let changed = self.unsaved();
        let known = custom_agents_known();
        let mark = if changed {
            div()
                .flex_none()
                .size(rems_from_px(6.0_f32))
                .rounded_full()
                .bg(accent(cx))
                .into_any_element()
        } else {
            button_icon("check")
                .size(IconSize::Custom(rems_from_px(12.0_f32)))
                .into_any_element()
        };
        let save = action_button("agents-save", "Save")
            .track_focus(&self.save)
            .disabled(!save_enabled(changed, self.saving, known))
            .when(!known, |save| save.tooltip(Tooltip::text(NOT_READ_TIP)))
            .on_click(cx.listener(|view, _: &ClickEvent, _, cx| view.agents.save(cx)));
        bottom_bar(cx)
            .child(
                h_flex()
                    .min_w_0()
                    .gap(rems_from_px(6.0_f32))
                    .child(mark)
                    .child(foot_text(status_text(changed)).truncate()),
            )
            .child(div().flex_1())
            .child(save)
            .into_any_element()
    }
}

fn custom_row(index: usize, agent: &CustomAgent, cx: &App) -> SettingsRow {
    let (base, command) = row_help(agent);
    let help = h_flex()
        .min_w_0()
        .gap_1()
        .child(foot_text(base))
        .child(foot_text("·"))
        .child(
            Label::new(command.to_owned())
                .size(LabelSize::Custom(rems_from_px(11.0_f32)))
                .color(Color::Muted)
                .buffer_font(cx)
                .truncate(),
        );
    let chevron = Icon::new(IconName::ChevronRight)
        .size(IconSize::Custom(rems_from_px(14.0_f32)))
        .color(Color::Placeholder);
    SettingsRow::new(("agents-row", index), row_label(agent).to_owned())
        .label_weight(FontWeight::MEDIUM)
        .lead(tile(&agent.base))
        .help_child(help)
        .control(chevron)
}

fn built_in_group(cx: &App) -> SettingsGroup {
    let rows = AGENT_CHOICES.iter().map(|choice| {
        let lead = SettingsTile::icon(button_icon("bot"), TileColor::Gray, TileSize::Row);
        let program = Label::new(default_program(choice.id))
            .size(LabelSize::Small)
            .color(Color::Muted)
            .buffer_font(cx);
        let id = SharedString::from(format!("agents-built-in-{}", choice.id));
        SettingsRow::new(id, choice.label)
            .lead(lead)
            .control(program)
    });
    SettingsGroup::new("Built-in agents")
        .count(AGENT_CHOICES.len().to_string())
        .child(SettingsCard::new(SeparatorInset::Tile).children(rows))
        .footnote(BUILT_IN_FOOT)
}

#[cfg(test)]
#[path = "settings_agents_tests.rs"]
mod tests;
