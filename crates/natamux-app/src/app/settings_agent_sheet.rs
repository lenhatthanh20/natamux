//! The agent sheet of Settings > Agents (layout-b 4.4): a 520 px dialog that edits one custom
//! agent of the page's draft (id, base, command). Done puts it in the draft; Save on the page
//! sends the list. The rules the page and the sheet share are here too.

use editor::{Editor, EditorEvent};
use gpui::{
    ClickEvent, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, Subscription,
    WeakEntity,
};
use natamux_core::formats::is_slug;
use natamux_proto::agents::custom::{CustomAgent, OTHER_BASE};
use natamux_proto::agents::start::{DEFAULT_PROGRAMS, LAUNCHABLE_AGENTS};
use ui::{ButtonLike, prelude::*};

use super::super::settings_view::SettingsView;
use crate::agents::agent_choices::{AGENT_CHOICES, DEFAULT_AGENT};
use crate::components::button_style::{button_icon, quiet_button};
use crate::components::colors::TileColor;
use crate::components::confirm::Confirm;
use crate::components::dialog::{
    DialogFrame, DialogLayer, DialogView, cancel_button, primary_button,
};
use crate::components::form::{SettingsInput, input_style, text_editor};
use crate::components::settings_card::{SeparatorInset, SettingsCard, SettingsGroup, SettingsRow};
use crate::components::settings_tile::{SettingsTile, TileSize};
use crate::stages::stage_fields_view::{Choice, SELECT_WIDTH, open_choices, select};

const OTHER_LABEL: &str = "Other CLI";
const OTHER_NOTE: &str = "No hooks: Natamux sees the bell, natamux ask and natamux done.";
const ID_HELP: &str = "The name tasks and stages use.";
const COMMAND_HELP: &str = "A program, or a function or alias of your shell, such as cq.";
const FOOTER_NOTE: &str = "Save on the page sends it to the daemon.";
const REMOVE_TEXT: &str = "Stages that use it go back to the task's agent. Nothing changes on the daemon until you click Save.";
/// The daemon's own messages (`natamux-core` `custom_agents`), so the sheet and Save agree.
const ID_FORMAT: &str = "An agent id is lower case words joined by \"-\"";
const ID_LONG: &str = "An agent id has at most 32 characters";
const NO_COMMAND: &str = "Enter the command";
const COMMAND_LONG: &str = "The command has at most 500 characters";
const INPUT_WIDTH: f32 = 200.;

/// (base, label) of the "Based on" select: the built-in agents, then any other CLI.
pub(super) fn base_options() -> Vec<(String, String)> {
    let built_in = AGENT_CHOICES
        .iter()
        .map(|choice| (choice.id.to_owned(), choice.label.to_owned()));
    built_in
        .chain([(OTHER_BASE.to_owned(), OTHER_LABEL.to_owned())])
        .collect()
}

/// The program a built-in agent starts with, such as `cursor-agent` for Cursor.
pub(super) fn default_program(id: &str) -> &'static str {
    DEFAULT_PROGRAMS
        .iter()
        .find(|(agent, _)| *agent == id)
        .map_or("", |(_, program)| program)
}

pub(super) fn base_label(base: &str) -> String {
    base_options()
        .into_iter()
        .find(|(id, _)| id == base)
        .map_or_else(|| base.to_owned(), |(_, label)| label)
}

/// The tile of a custom agent: a terminal for any other CLI, a bot for a built-in base.
pub(super) fn agent_tile(base: &str) -> (&'static str, TileColor) {
    if base == OTHER_BASE {
        ("terminal", TileColor::Cyan)
    } else {
        ("bot", TileColor::Blue)
    }
}

pub(super) fn tile(base: &str) -> SettingsTile {
    let (icon, color) = agent_tile(base);
    SettingsTile::icon(button_icon(icon), color, TileSize::Row)
}

/// The footnote under the sheet's card: what Natamux does with the base.
pub(super) fn base_footnote(base: &str) -> String {
    if base == OTHER_BASE {
        OTHER_NOTE.to_owned()
    } else {
        format!(
            "Natamux starts it like {}, with its hooks.",
            base_label(base)
        )
    }
}

pub(super) fn sheet_title(editing: bool, id: &str) -> String {
    if editing {
        id.to_owned()
    } else {
        "Add agent".to_owned()
    }
}

/// What is wrong with the sheet's agent, field by field.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct SheetErrors {
    pub id: Option<String>,
    pub command: Option<String>,
}

impl SheetErrors {
    pub fn is_empty(&self) -> bool {
        self.id.is_none() && self.command.is_none()
    }
}

/// The daemon's rules for one row, checked before it reaches the draft. `others` is the rest of
/// the draft: an id is taken by a built-in agent, `other`, or another row.
pub(super) fn sheet_errors(agent: &CustomAgent, others: &[CustomAgent]) -> SheetErrors {
    let id = agent.id.as_str();
    let taken = LAUNCHABLE_AGENTS.contains(&id)
        || id == OTHER_BASE
        || others.iter().any(|other| other.id == id);
    let id_error = if id.chars().count() > 32 {
        Some(ID_LONG.to_owned())
    } else if !is_slug(id) {
        Some(ID_FORMAT.to_owned())
    } else if taken {
        Some(format!("The agent id \"{id}\" is taken"))
    } else {
        None
    };
    let command = agent.command.as_str();
    let command_error = if command.is_empty() {
        Some(NO_COMMAND.to_owned())
    } else if command.chars().count() > 500 {
        Some(COMMAND_LONG.to_owned())
    } else {
        None
    };
    SheetErrors {
        id: id_error,
        command: command_error,
    }
}

/// Commands and ids read in the buffer font, at 13 px as the other inputs.
fn set_mono(editor: &Entity<Editor>, cx: &mut App) {
    let font = theme::theme_settings(cx).buffer_font(cx).family.clone();
    editor.update(cx, |editor, _| {
        editor.set_text_style_refinement(input_style(Some(font), Some(13.0)))
    });
}

/// Opens the sheet on row `index` of the page's draft, or on a new agent when None.
pub(crate) fn open(
    view: &Entity<SettingsView>,
    index: Option<usize>,
    window: &mut Window,
    cx: &mut App,
) {
    let draft = view.read(cx).agents.draft().to_vec();
    let agent = match index {
        Some(at) => match draft.get(at) {
            Some(agent) => agent.clone(),
            None => return,
        },
        None => CustomAgent {
            base: DEFAULT_AGENT.to_owned(),
            ..CustomAgent::default()
        },
    };
    let others: Vec<CustomAgent> = draft
        .into_iter()
        .enumerate()
        .filter(|(at, _)| Some(*at) != index)
        .map(|(_, agent)| agent)
        .collect();
    let view = view.downgrade();
    let sheet = cx.new(|cx| AgentSheet::new(view, index, others, &agent, window, cx));
    let first = sheet.read(cx).id.focus_handle(cx);
    DialogLayer::open(sheet, window, cx);
    window.focus(&first, cx);
}

pub struct AgentSheet {
    view: WeakEntity<SettingsView>,
    /// The draft row being edited; None for a new agent.
    index: Option<usize>,
    others: Vec<CustomAgent>,
    id: Entity<Editor>,
    command: Entity<Editor>,
    base: String,
    /// Done was pressed: the errors show from now on.
    tried: bool,
    focus: FocusHandle,
    base_focus: FocusHandle,
    remove_focus: FocusHandle,
    cancel_focus: FocusHandle,
    done_focus: FocusHandle,
    _typing: [Subscription; 2],
}

impl AgentSheet {
    fn new(
        view: WeakEntity<SettingsView>,
        index: Option<usize>,
        others: Vec<CustomAgent>,
        agent: &CustomAgent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let id = text_editor("claude-qwen", None, window, cx);
        let command = text_editor("cq", None, window, cx);
        for (editor, text) in [(&id, &agent.id), (&command, &agent.command)] {
            set_mono(editor, cx);
            editor.update(cx, |editor, cx| editor.set_text(text.clone(), window, cx));
        }
        let typing = [&id, &command].map(|editor| {
            cx.subscribe(editor, |_, _, event: &EditorEvent, cx| {
                if matches!(event, EditorEvent::BufferEdited) {
                    cx.notify();
                }
            })
        });
        Self {
            view,
            index,
            others,
            id,
            command,
            base: agent.base.clone(),
            tried: false,
            focus: cx.focus_handle(),
            base_focus: cx.focus_handle(),
            remove_focus: cx.focus_handle(),
            cancel_focus: cx.focus_handle(),
            done_focus: cx.focus_handle(),
            _typing: typing,
        }
    }

    fn agent(&self, cx: &App) -> CustomAgent {
        let text = |editor: &Entity<Editor>| editor.read(cx).text(cx).trim().to_owned();
        CustomAgent {
            id: text(&self.id),
            base: self.base.clone(),
            command: text(&self.command),
        }
    }

    fn base_menu(&self, cx: &mut Context<Self>) -> ButtonLike {
        let current = self.base.clone();
        let this = cx.weak_entity();
        select("agent-sheet-base", base_label(&self.base))
            .track_focus(&self.base_focus)
            .on_click(move |event: &ClickEvent, window, cx| {
                let choices = base_options().into_iter().map(|(base, label)| {
                    let (this, checked) = (this.clone(), base == current);
                    Choice::new(label, Some(checked), move |_, cx| {
                        this.update(cx, |this, cx| {
                            this.base = base.clone();
                            cx.notify();
                        })
                        .ok();
                    })
                });
                open_choices(choices.collect(), event.position(), window, cx);
            })
    }

    /// Asks first; on yes the row leaves the draft and the sheet closes.
    fn ask_remove(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.index else {
            return;
        };
        let (view, sheet) = (self.view.clone(), cx.weak_entity());
        let id = self.agent(cx).id;
        let on_remove = move |_: &mut Window, cx: &mut App| {
            view.update(cx, |view, cx| view.agents.remove(index, cx))
                .ok();
            sheet.update(cx, |_, cx| cx.emit(DismissEvent)).ok();
        };
        Confirm::new(format!("Remove {id}?"), REMOVE_TEXT, "Remove", on_remove)
            .sheet()
            .open_over(window, cx);
    }
}

impl EventEmitter<DismissEvent> for AgentSheet {}

impl Focusable for AgentSheet {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl DialogView for AgentSheet {
    fn tab_ring(&self, cx: &App) -> Vec<FocusHandle> {
        let mut ring = vec![
            self.id.focus_handle(cx),
            self.base_focus.clone(),
            self.command.focus_handle(cx),
        ];
        if self.index.is_some() {
            ring.push(self.remove_focus.clone());
        }
        ring.extend([self.cancel_focus.clone(), self.done_focus.clone()]);
        ring
    }

    /// Done: a right agent goes into the page's draft.
    fn submit(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.tried = true;
        let agent = self.agent(cx);
        if sheet_errors(&agent, &self.others).is_empty() {
            let index = self.index;
            self.view
                .update(cx, |view, cx| view.agents.put(index, agent, cx))
                .ok();
            cx.emit(DismissEvent);
        }
        cx.notify();
    }
}

impl Render for AgentSheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let agent = self.agent(cx);
        let errors = if self.tried {
            sheet_errors(&agent, &self.others)
        } else {
            SheetErrors::default()
        };
        let base = div()
            .w(rems_from_px(SELECT_WIDTH))
            .child(self.base_menu(cx));
        let base_row = SettingsRow::new("agent-sheet-base-row", "Based on").control(base);
        let card = SettingsCard::new(SeparatorInset::Text)
            .child(
                SettingsRow::new("agent-sheet-id", "Id")
                    .help(ID_HELP)
                    .error(errors.id.clone())
                    .control(
                        SettingsInput::new(&self.id)
                            .width(INPUT_WIDTH)
                            .invalid(errors.id.is_some()),
                    ),
            )
            .child(base_row)
            .child(
                SettingsRow::new("agent-sheet-command", "Command")
                    .help(COMMAND_HELP)
                    .error(errors.command.clone())
                    .stacked(true)
                    .control(
                        SettingsInput::new(&self.command)
                            .width(INPUT_WIDTH)
                            .invalid(errors.command.is_some()),
                    ),
            );
        let group = SettingsGroup::new("Agent")
            .first(true)
            .child(card)
            .footnote(base_footnote(&self.base));
        let close = cx.listener(|_, _, _, cx| cx.emit(DismissEvent));
        let cancel = cx.listener(|_, _, _, cx| cx.emit(DismissEvent));
        let done = cx.listener(|this, _, window, cx| this.submit(window, cx));
        let remove = self.index.map(|_| {
            quiet_button("agent-sheet-remove", "Remove agent")
                .start_icon(button_icon("trash").color(Color::Error))
                .color(Color::Error)
                .track_focus(&self.remove_focus)
                .on_click(cx.listener(|this, _, window, cx| this.ask_remove(window, cx)))
        });
        let mut frame = DialogFrame::new(sheet_title(self.index.is_some(), &agent.id), close)
            .sheet()
            .lead(tile(&self.base))
            .footer_note(FOOTER_NOTE)
            .child(group);
        if let Some(remove) = remove {
            frame = frame.footer_start(remove);
        }
        let frame = frame
            .footer(cancel_button("Cancel", &self.cancel_focus, cancel))
            .footer(primary_button("Done", &self.done_focus, false, done));
        div()
            .key_context("Dialog")
            .track_focus(&self.focus)
            .child(frame)
    }
}

#[cfg(test)]
#[path = "settings_agent_sheet_tests.rs"]
mod tests;
