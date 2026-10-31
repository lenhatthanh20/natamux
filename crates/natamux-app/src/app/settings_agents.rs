//! The Agents page of Settings (T-0249): the built-in agents, read only, and the custom agents as
//! rows (id, base, command). The rows are a draft until Save sends the whole list.

use editor::{Editor, EditorEvent};
use gpui::{ClickEvent, Entity, FocusHandle, Focusable, FontWeight, Hsla, Subscription};
use natamux_client::ClientError;
use natamux_proto::agents::custom::{CustomAgent, OTHER_BASE};
use natamux_proto::agents::start::DEFAULT_PROGRAMS;
use ui::{Tooltip, prelude::*};

use super::dashboard::Dashboard;
use super::settings_view::SettingsView;
use super::source::reason;
use crate::agents::agent_choices::{
    AGENT_CHOICES, DEFAULT_AGENT, custom_agents, custom_agents_known,
};
use crate::components::agent_badge::AgentIcon;
use crate::components::button_style::action_button;
use crate::components::form::{input_box, text_editor};
use crate::stages::stage_fields_view::{
    Choice, SELECT_WIDTH, error_line, help_line, labelled, open_choices, select,
};

const COMMAND_HELP: &str = "A program, or a function or alias of your shell, such as cq.";
const OTHER_NOTE: &str = "No hooks: Natamux sees the bell, natamux ask and natamux done.";
const OTHER_LABEL: &str = "Other CLI";
const NOT_READ: &str = "The custom agents could not be read from the daemon, so Save is off.";

struct Row {
    id: Entity<Editor>,
    command: Entity<Editor>,
    base: String,
    _typing: [Subscription; 2],
}

pub struct AgentRows {
    dashboard: Entity<Dashboard>,
    rows: Vec<Row>,
    /// The list the rows were made from, or the last one saved.
    shown: Vec<CustomAgent>,
    saving: bool,
    problem: Option<String>,
    add: FocusHandle,
    save: FocusHandle,
}

/// (base, label) of the "Based on" select: the built-in agents, then any other CLI.
fn base_options() -> Vec<(String, String)> {
    let built_in = AGENT_CHOICES
        .iter()
        .map(|choice| (choice.id.to_owned(), choice.label.to_owned()));
    built_in
        .chain([(OTHER_BASE.to_owned(), OTHER_LABEL.to_owned())])
        .collect()
}

/// The program a built-in agent starts with, such as `cursor-agent` for Cursor.
fn default_program(id: &str) -> &'static str {
    DEFAULT_PROGRAMS
        .iter()
        .find(|(agent, _)| *agent == id)
        .map_or("", |(_, program)| program)
}

fn base_label(base: &str) -> String {
    base_options()
        .into_iter()
        .find(|(id, _)| id == base)
        .map_or_else(|| base.to_owned(), |(_, label)| label)
}

fn make_row(agent: &CustomAgent, window: &mut Window, cx: &mut Context<SettingsView>) -> Row {
    let id = text_editor("claude-qwen", None, window, cx);
    let command = text_editor("cq", None, window, cx);
    id.update(cx, |editor, cx| {
        editor.set_text(agent.id.clone(), window, cx)
    });
    command.update(cx, |editor, cx| {
        editor.set_text(agent.command.clone(), window, cx)
    });
    let typing = [&id, &command].map(|editor| {
        cx.subscribe(editor, |_, _, event: &EditorEvent, cx| {
            if matches!(event, EditorEvent::BufferEdited) {
                cx.notify();
            }
        })
    });
    Row {
        id,
        command,
        base: agent.base.clone(),
        _typing: typing,
    }
}

impl AgentRows {
    pub fn new(dashboard: Entity<Dashboard>, cx: &mut Context<SettingsView>) -> Self {
        Self {
            dashboard,
            rows: Vec::new(),
            shown: Vec::new(),
            saving: false,
            problem: None,
            add: cx.focus_handle(),
            save: cx.focus_handle(),
        }
    }

    fn draft(&self, cx: &App) -> Vec<CustomAgent> {
        let text = |editor: &Entity<Editor>| editor.read(cx).text(cx).trim().to_owned();
        self.rows
            .iter()
            .map(|row| CustomAgent {
                id: text(&row.id),
                base: row.base.clone(),
                command: text(&row.command),
            })
            .collect()
    }

    /// Shows the list the dashboard holds, unless the person changed the rows: the list may come
    /// after the page was made.
    pub fn sync(&mut self, window: &mut Window, cx: &mut Context<SettingsView>) {
        let held = custom_agents();
        if held == self.shown || self.saving || self.draft(cx) != self.shown {
            return;
        }
        self.rows = held
            .iter()
            .map(|agent| make_row(agent, window, cx))
            .collect();
        self.shown = held;
    }

    /// The rows' fields, then Add agent and Save, in the order they are drawn.
    pub fn stops(&self, cx: &App) -> Vec<FocusHandle> {
        let mut stops = Vec::new();
        for row in &self.rows {
            stops.push(row.id.focus_handle(cx));
            stops.push(row.command.focus_handle(cx));
        }
        stops.extend([self.add.clone(), self.save.clone()]);
        stops
    }

    fn add_row(&mut self, window: &mut Window, cx: &mut Context<SettingsView>) {
        let agent = CustomAgent {
            base: DEFAULT_AGENT.to_owned(),
            ..CustomAgent::default()
        };
        self.rows.push(make_row(&agent, window, cx));
        cx.notify();
    }

    fn remove(&mut self, index: usize, cx: &mut Context<SettingsView>) {
        if index < self.rows.len() {
            self.rows.remove(index);
            cx.notify();
        }
    }

    fn set_base(&mut self, index: usize, base: &str, cx: &mut Context<SettingsView>) {
        if let Some(row) = self.rows.get_mut(index) {
            row.base = base.to_owned();
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
        let answer = source.save_custom_agents(self.draft(cx));
        cx.spawn(async move |view, cx| {
            let answer = answer.await;
            view.update(cx, |view, cx| view.agents.saved(answer, cx))
                .ok();
        })
        .detach();
        cx.notify();
    }

    fn saved(
        &mut self,
        answer: Result<Vec<CustomAgent>, ClientError>,
        cx: &mut Context<SettingsView>,
    ) {
        self.saving = false;
        match answer {
            Ok(list) => {
                self.shown = list.clone();
                self.dashboard
                    .update(cx, |dashboard, cx| dashboard.use_custom_agents(list, cx));
            }
            Err(error) => self.problem = Some(reason(&error)),
        }
        cx.notify();
    }

    /// The page, to sit in the scroll body of the app's pages.
    pub fn content(&mut self, cx: &mut Context<SettingsView>) -> AnyElement {
        let view = cx.entity();
        let border = cx.theme().colors().border_variant;
        let built_in = AGENT_CHOICES.iter().map(|choice| {
            h_flex()
                .w_full()
                .gap_2()
                .py_2()
                .border_b_1()
                .border_color(border)
                .child(AgentIcon::new(Some(choice.id.to_owned())))
                .child(Label::new(choice.label))
                .child(Label::new(default_program(choice.id)).color(Color::Muted))
        });
        let rows: Vec<AnyElement> = self
            .rows
            .iter()
            .enumerate()
            .map(|(index, row)| row_view(index, row, &view, border, cx))
            .collect();
        let add = action_button("agents-add", "Add agent")
            .track_focus(&self.add)
            .on_click(
                cx.listener(|view, _: &ClickEvent, window, cx| view.agents.add_row(window, cx)),
            );
        let known = custom_agents_known();
        let save = action_button("agents-save", "Save")
            .track_focus(&self.save)
            .disabled(self.saving || !known)
            .on_click(cx.listener(|view, _: &ClickEvent, _, cx| view.agents.save(cx)));
        v_flex()
            .child(section("Built-in agents"))
            .children(built_in)
            .child(section("Custom agents"))
            .child(help_line(
                "Start a built-in agent with your own command, or add any other agent CLI.",
            ))
            .children(rows)
            .child(h_flex().gap_2().py_3().child(add).child(save))
            .when(!known, |this| this.child(error_line(NOT_READ)))
            .children(self.problem.clone().map(error_line))
            .into_any_element()
    }
}

fn section(text: &'static str) -> impl IntoElement {
    div().pt_5().pb_1().child(
        Label::new(text)
            .size(LabelSize::XSmall)
            .weight(FontWeight::MEDIUM)
            .color(Color::Muted),
    )
}

fn base_choices(index: usize, current: &str, view: &Entity<SettingsView>) -> Vec<Choice> {
    base_options()
        .into_iter()
        .map(|(base, label)| {
            let view = view.clone();
            let checked = base == current;
            Choice::new(label, Some(checked), move |_, cx| {
                view.update(cx, |view, cx| view.agents.set_base(index, &base, cx));
            })
        })
        .collect()
}

fn row_view(
    index: usize,
    row: &Row,
    view: &Entity<SettingsView>,
    border: Hsla,
    cx: &mut Context<SettingsView>,
) -> AnyElement {
    let (base, menu_view) = (row.base.clone(), view.clone());
    let base_select = select(("agent-base", index), base_label(&row.base)).on_click(
        move |event: &ClickEvent, window, cx| {
            let choices = base_choices(index, &base, &menu_view);
            open_choices(choices, event.position(), window, cx);
        },
    );
    let remove = IconButton::new(("agent-remove", index), IconName::Trash)
        .icon_size(IconSize::Small)
        .tooltip(Tooltip::text("Remove agent"))
        .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| view.agents.remove(index, cx)));
    let id_field =
        div()
            .w(rems_from_px(160.0_f32))
            .child(labelled("Id", None, None, input_box(&row.id, cx)));
    let base_field =
        div()
            .w(rems_from_px(SELECT_WIDTH))
            .child(labelled("Based on", None, None, base_select));
    let help = Some(COMMAND_HELP.into());
    let command_field = div()
        .flex_1()
        .min_w(rems_from_px(200.0_f32))
        .child(labelled("Command", None, help, input_box(&row.command, cx)));
    let remove_slot = div().flex_none().pt(rems_from_px(28.0_f32)).child(remove);
    v_flex()
        .w_full()
        .gap_1()
        .py_3()
        .border_b_1()
        .border_color(border)
        .child(
            h_flex()
                .flex_wrap()
                .items_start()
                .gap_3()
                .child(id_field)
                .child(base_field)
                .child(command_field)
                .child(remove_slot),
        )
        .when(row.base == OTHER_BASE, |this| {
            this.child(help_line(OTHER_NOTE))
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_base_select_lists_the_built_in_agents_then_other_cli() {
        let options = base_options();
        assert_eq!(options.len(), AGENT_CHOICES.len() + 1);
        assert_eq!(options[0].1, "Claude Code");
        assert_eq!(options.last().map(|(id, _)| id.as_str()), Some(OTHER_BASE));
        assert_eq!(base_label("other"), "Other CLI");
        assert_eq!(base_label("codex"), "Codex");
    }

    #[test]
    fn shows_the_default_program_of_each_built_in_agent() {
        let programs: Vec<_> = AGENT_CHOICES
            .iter()
            .map(|choice| default_program(choice.id))
            .collect();
        assert_eq!(
            programs,
            ["claude", "codex", "gemini", "copilot", "cursor-agent"]
        );
    }
}
