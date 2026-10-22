//! Main for an open task, until screen 2 brings the workspace view: the phase 1 terminal pane on
//! the task's first agent terminal. The terminal keeps Esc for its agent; Mod+[ (macOS), View >
//! All tasks or the sidebar go back.

use std::sync::Arc;

use gpui::{App, Entity, FocusHandle, Focusable};
use natamux_client::TerminalStatus;
use natamux_proto::tasks::task::Task;
use natamux_proto::terminals::list::TerminalList;
use ui::prelude::*;

use super::remote_terminal::{RemoteTerminal, Target};
use crate::app::source::Source;

pub const SCREEN_2_NOTE: &str =
    "Open this task in the dashboard for now. The native workspace view comes with screen 2.";

pub struct TerminalPane {
    source: Option<Arc<Source>>,
    task: Option<Task>,
    terminal: Option<Entity<RemoteTerminal>>,
    /// The name of the shown terminal.
    target: Option<String>,
    /// The read of the task's terminals, to find its first agent terminal.
    reading: Option<gpui::Task<()>>,
    visible: bool,
    focus: FocusHandle,
}

impl TerminalPane {
    pub fn new(source: Option<Arc<Source>>, cx: &mut Context<Self>) -> Self {
        TerminalPane {
            source,
            task: None,
            terminal: None,
            target: None,
            reading: None,
            visible: true,
            focus: cx.focus_handle(),
        }
    }

    /// Shows the task's first agent terminal. The old socket closes; its shell keeps running.
    pub fn show(&mut self, task: Task, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        self.set_task(task, window, cx);
        self.focus_terminal(window, cx);
    }

    /// The task list changed: take the new state of the shown task. A task that finished drops
    /// its socket and says so; one that got a workspace connects.
    pub fn refresh(&mut self, tasks: &[Task], window: &mut Window, cx: &mut Context<Self>) {
        let shown = self.task.as_ref().map(|task| task.id.clone());
        if let Some(task) = shown.and_then(|id| tasks.iter().find(|task| task.id == id)) {
            self.set_task(task.clone(), window, cx);
        }
    }

    /// Whether the pane is on screen. A hidden terminal gives up the shell's size.
    pub fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        self.visible = visible;
        if let Some(terminal) = &self.terminal {
            terminal.update(cx, |terminal, _| terminal.set_visible(visible));
        }
    }

    fn set_task(&mut self, task: Task, window: &mut Window, cx: &mut Context<Self>) {
        let live = self.source.is_some() && task.has_live_terminal();
        let had = self.terminal.is_some() || self.reading.is_some();
        if needs_new_terminal(self.task.as_ref(), &task, had, live) {
            self.terminal = None;
            self.target = None;
            self.reading = None;
            if live {
                self.read_terminals(&task.id, window, cx);
            }
        }
        if self.task.as_ref() != Some(&task) {
            self.task = Some(task);
            cx.notify();
        }
    }

    /// Reads which terminals run agents, then connects to the first one.
    fn read_terminals(&mut self, task_id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(source) = self.source.clone() else {
            return;
        };
        let answer = source.list_terminals(task_id);
        let task_id = task_id.to_string();
        self.reading = Some(cx.spawn_in(window, async move |this, cx| {
            let list = answer.await.ok();
            this.update_in(cx, |this, window, cx| {
                this.reading = None;
                let still_shown = this.task.as_ref().is_some_and(|task| task.id == task_id);
                if let Some(name) = first_agent_terminal(list.as_ref()).filter(|_| still_shown) {
                    this.connect(task_id, name, window, cx);
                }
                cx.notify();
            })
            .ok();
        }));
    }

    fn connect(
        &mut self,
        task_id: String,
        name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(source) = self.source.clone() else {
            return;
        };
        let target = Target {
            task_id,
            terminal: name.clone(),
        };
        let visible = self.visible;
        let terminal = cx.new(|cx| {
            let mut terminal = RemoteTerminal::new(source, target, window, cx);
            terminal.set_visible(visible);
            terminal
        });
        cx.observe(&terminal, |_, _, cx| cx.notify()).detach();
        self.terminal = Some(terminal);
        self.target = Some(name);
        // The pane took the focus when the task opened; the terminal gets it now.
        if self.focus.is_focused(window) {
            self.focus_terminal(window, cx);
        }
    }

    fn focus_terminal(&self, window: &mut Window, cx: &mut App) {
        if let Some(terminal) = &self.terminal {
            let focus = terminal.read(cx).view.focus_handle(cx);
            window.focus(&focus, cx);
        }
    }

    fn note(&self) -> Option<&'static str> {
        self.task.as_ref()?;
        if self.terminal.is_some() || self.reading.is_some() {
            return None;
        }
        Some(if self.source.is_none() {
            "No connection to the daemon."
        } else {
            SCREEN_2_NOTE
        })
    }
}

/// The first terminal with an agent, in the daemon's order. None: the task has no agent
/// terminal, or its terminals could not be read.
pub fn first_agent_terminal(list: Option<&TerminalList>) -> Option<String> {
    let list = list?;
    list.terminals
        .iter()
        .find(|name| list.agents.contains(name))
        .or_else(|| list.agents.first())
        .cloned()
}

/// A new socket is needed for another task, or when the task gained or lost its live terminal.
fn needs_new_terminal(shown: Option<&Task>, task: &Task, has_terminal: bool, live: bool) -> bool {
    let same = shown.is_some_and(|shown| shown.id == task.id);
    !same || has_terminal != live
}

fn status_text(status: &TerminalStatus) -> Option<(String, Color)> {
    match status {
        TerminalStatus::Open => None,
        TerminalStatus::Connecting => Some(("Connecting…".into(), Color::Muted)),
        TerminalStatus::Reconnecting => {
            Some(("Connection lost. Reconnecting…".into(), Color::Warning))
        }
        TerminalStatus::Ended { code: Some(code) } => {
            Some((format!("The shell ended with code {code}."), Color::Muted))
        }
        TerminalStatus::Ended { code: None } => Some(("The shell ended.".into(), Color::Muted)),
        TerminalStatus::Failed(reason) => Some((reason.clone(), Color::Error)),
    }
}

impl Focusable for TerminalPane {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for TerminalPane {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let header = self.task.as_ref().map(|task| {
            h_flex()
                .px_3()
                .py_1p5()
                .gap_2()
                .border_b_1()
                .border_color(colors.border)
                .child(Label::new(task.id.clone()).color(Color::Muted))
                .child(Label::new(task.title.clone()).truncate())
                .child(div().flex_grow_1())
                .children(self.target.as_ref().map(|name| {
                    Label::new(format!("Terminal {name}"))
                        .size(LabelSize::Small)
                        .color(Color::Muted)
                }))
        });
        let status = self
            .terminal
            .as_ref()
            .and_then(|terminal| status_text(terminal.read(cx).status()))
            .map(|(text, color)| {
                div()
                    .px_3()
                    .py_1()
                    .border_t_1()
                    .border_color(colors.border)
                    .child(Label::new(text).size(LabelSize::Small).color(color))
            });
        let body = match (&self.terminal, self.task.is_some()) {
            (Some(terminal), _) => div()
                .flex_grow_1()
                .min_h_0()
                .child(terminal.read(cx).view.clone())
                .into_any_element(),
            (None, true) => div()
                .p_4()
                .children(self.note().map(|note| Label::new(note).color(Color::Muted)))
                .into_any_element(),
            (None, false) => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(Label::new("Pick a task to see its agent's terminal.").color(Color::Muted))
                .into_any_element(),
        };
        v_flex()
            .track_focus(&self.focus)
            .size_full()
            .bg(colors.editor_background)
            .children(header)
            .child(body)
            .children(status)
    }
}

#[cfg(test)]
mod tests {
    use natamux_proto::tasks::task::TaskState;

    use super::*;

    fn task(id: &str, state: TaskState) -> Task {
        Task {
            id: id.into(),
            title: "A task".into(),
            state,
            stage: "implement".into(),
            workspace: Some("/w".into()),
            agent: None,
            parent_id: None,
            repo: None,
            updated_at: "2026-10-22T00:00:00.000Z".into(),
            ..Task::default()
        }
    }

    #[test]
    fn keeps_the_socket_while_the_same_task_stays_live() {
        let running = task("T-1", TaskState::Running);
        let review = task("T-1", TaskState::Review);
        assert!(!needs_new_terminal(Some(&running), &review, true, true));
    }

    #[test]
    fn a_task_that_finishes_drops_its_socket_and_one_that_starts_connects() {
        let running = task("T-1", TaskState::Running);
        let done = task("T-1", TaskState::Done);
        assert!(needs_new_terminal(Some(&running), &done, true, false));
        assert!(needs_new_terminal(Some(&done), &running, false, true));
        assert!(!needs_new_terminal(Some(&done), &done, false, false));
    }

    fn list(terminals: &[&str], agents: &[&str]) -> TerminalList {
        TerminalList {
            terminals: terminals.iter().map(|name| name.to_string()).collect(),
            agents: agents.iter().map(|name| name.to_string()).collect(),
            ..TerminalList::default()
        }
    }

    #[test]
    fn opens_the_first_agent_terminal_in_the_daemons_order() {
        let terminals = list(&["5", "1", "2"], &["2", "1"]);
        assert_eq!(first_agent_terminal(Some(&terminals)).as_deref(), Some("1"));
        assert_eq!(
            first_agent_terminal(Some(&list(&[], &["3"]))).as_deref(),
            Some("3")
        );
    }

    #[test]
    fn a_task_with_no_agent_terminal_opens_none() {
        assert_eq!(first_agent_terminal(Some(&list(&["5"], &[]))), None);
        assert_eq!(first_agent_terminal(None), None);
    }

    #[test]
    fn another_task_gets_its_own_socket() {
        let one = task("T-1", TaskState::Running);
        let two = task("T-2", TaskState::Running);
        assert!(needs_new_terminal(Some(&one), &two, true, true));
        assert!(needs_new_terminal(None, &two, false, true));
    }
}
