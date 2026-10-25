//! One terminal pane of the workspace or the Agents grid (docs/ui-spec.md 4.23,
//! `features/terminals/TerminalPane.tsx` and `TerminalView.tsx`): the header, the terminal, and
//! what lies over it: the connection strip, the restore cover, the skeleton while history is
//! written, the find bar, "Jump to latest", and the bar of an ended terminal.

use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{ClickEvent, Entity};
use natamux_client::TerminalStatus;
use natamux_proto::tasks::task::Task;
use natamux_proto::terminals::list::TerminalList;
use ui::{Tooltip, prelude::*};

use super::pane_drop::with_drop;
use super::pane_header::PaneHeader;
use super::remote_terminal::RemoteTerminal;
use super::terminal_find::TerminalFind;
use super::terminal_screen::terminal_screen;
use crate::app::shortcuts::{FindInTerminal, MaximizePane};
use crate::components::button_style::{action_button, button_icon, quiet_button};
use crate::components::icons::icon_path;
use crate::components::skeleton::SkeletonList;
use crate::components::spinner::Spinner;

type OnClick = Rc<dyn Fn(&mut Window, &mut App)>;

/// `.hm-late-in`: a quick connect shows no strip.
const STRIP_DELAY: Duration = Duration::from_millis(200);

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

// Kept from the phase 1 pane (spec code map); the terminal cache now drops finished tasks.
#[cfg_attr(not(test), allow(dead_code))]
/// A new socket is needed for another task, or when the task gained or lost its live terminal.
fn needs_new_terminal(shown: Option<&Task>, task: &Task, has_terminal: bool, live: bool) -> bool {
    let same = shown.is_some_and(|shown| shown.id == task.id);
    !same || has_terminal != live
}

/// The ended bar's text: the exit code when it is not 0, unless the task is finished, where the
/// finished banner says how it ended.
pub fn ended_text(code: Option<i32>, task_finished: bool) -> String {
    match code {
        Some(code) if code != 0 && !task_finished => {
            format!("The terminal ended. Exit code {code}.")
        }
        _ => "The terminal ended.".into(),
    }
}

/// The strip at the top of the body while the terminal is not open.
fn strip_text(status: &TerminalStatus) -> Option<&'static str> {
    match status {
        TerminalStatus::Connecting => Some("Connecting\u{2026}"),
        TerminalStatus::Reconnecting => Some("Reconnecting\u{2026}"),
        TerminalStatus::Failed(_) => Some("The terminal could not start."),
        TerminalStatus::Open | TerminalStatus::Ended { .. } => None,
    }
}

#[derive(IntoElement)]
pub struct TerminalPane {
    key: SharedString,
    terminal: Entity<RemoteTerminal>,
    header: PaneHeader,
    focused: bool,
    /// "Restoring Claude Code…": the terminal comes back after a restart.
    restoring: Option<String>,
    task_finished: bool,
    find: Option<Entity<TerminalFind>>,
    on_find: Option<OnClick>,
    on_maximize: Option<OnClick>,
    on_new_terminal: Option<OnClick>,
}

impl TerminalPane {
    pub fn new(
        key: impl Into<SharedString>,
        terminal: Entity<RemoteTerminal>,
        header: PaneHeader,
    ) -> Self {
        TerminalPane {
            key: key.into(),
            terminal,
            header,
            focused: false,
            restoring: None,
            task_finished: false,
            find: None,
            on_find: None,
            on_maximize: None,
            on_new_terminal: None,
        }
    }

    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    pub fn restoring(mut self, label: Option<String>) -> Self {
        self.restoring = label;
        self
    }

    pub fn task_finished(mut self, finished: bool) -> Self {
        self.task_finished = finished;
        self
    }

    /// The open find bar, and what Mod+F in this terminal does.
    pub fn find(
        mut self,
        find: Option<Entity<TerminalFind>>,
        open: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.find = find;
        self.on_find = Some(Rc::new(open));
        self
    }

    /// Mod+Shift+Enter in this pane.
    pub fn on_maximize(mut self, run: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_maximize = Some(Rc::new(run));
        self
    }

    /// Open a new terminal, under an ended one or a terminal that could not start.
    pub fn on_new_terminal(mut self, run: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_new_terminal = Some(Rc::new(run));
        self
    }

    fn new_terminal_button(&self, id: &str, label: &'static str) -> Option<impl IntoElement> {
        let run = self.on_new_terminal.clone()?;
        Some(
            quiet_button(SharedString::from(format!("{}-{id}", self.key)), label)
                .label_size(LabelSize::Small)
                .tooltip(Tooltip::text(
                    "Starts a plain terminal in the task's workspace. It does not start the agent.",
                ))
                .on_click(move |_: &ClickEvent, window, cx| run(window, cx)),
        )
    }
}

/// When the terminal left the open state, so the strip waits 200 ms before it shows: a fast
/// connect shows no flash (`.hm-late-in`).
struct StripClock(Option<Instant>);

/// The strip shows a failure at once, and connecting or reconnecting after this long.
pub fn strip_shows(failed: bool, waited: Duration) -> bool {
    failed || waited >= STRIP_DELAY
}

impl TerminalPane {
    /// "Connecting…", "Reconnecting…" (after 200 ms) or "The terminal could not start."
    fn strip(&self, status: &TerminalStatus, window: &mut Window, cx: &mut App) -> Option<Div> {
        let clock = window.use_keyed_state(
            SharedString::from(format!("{}-strip", self.key)),
            cx,
            |_, _| StripClock(None),
        );
        let Some(text) = strip_text(status) else {
            clock.update(cx, |clock, _| clock.0 = None);
            return None;
        };
        let since = clock.update(cx, |clock, _| *clock.0.get_or_insert_with(Instant::now));
        let failed = matches!(status, TerminalStatus::Failed(_));
        if !strip_shows(failed, since.elapsed()) {
            // Draws again until the delay has passed; at most 200 ms of frames.
            window.request_animation_frame();
            return None;
        }
        let colors = cx.theme().colors();
        Some(
            h_flex()
                .h(px(28.))
                .flex_none()
                .px_2p5()
                .gap_1p5()
                .border_b_1()
                .border_color(colors.border)
                .when(!failed, |strip| strip.child(Spinner::new(IconSize::XSmall)))
                .child(Label::new(text).size(LabelSize::Small).color(if failed {
                    Color::Error
                } else {
                    Color::Muted
                }))
                .when(failed, |strip| {
                    strip.children(self.new_terminal_button("restart", "Start a new terminal"))
                }),
        )
    }

    /// "Restoring Claude Code…" over the whole body; the terminal keeps its size under it.
    fn cover(&self, cx: &App) -> Option<Div> {
        let label = self.restoring.clone()?;
        Some(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .gap_2()
                .bg(cx.theme().colors().terminal_background)
                .child(Spinner::new(IconSize::Medium))
                .child(Label::new(label).size(LabelSize::Small).color(Color::Muted)),
        )
    }

    fn jump(&self) -> Div {
        let terminal = self.terminal.clone();
        div().absolute().bottom_2().right_3().child(
            action_button(
                SharedString::from(format!("{}-jump", self.key)),
                "Jump to latest",
            )
            .start_icon(button_icon("arrow-down"))
            .label_size(LabelSize::Small)
            .on_click(move |_, _, cx| {
                terminal.update(cx, |terminal, cx| terminal.jump_to_latest(cx))
            }),
        )
    }

    /// The bar of a terminal that ended: dimmed screen above, Open a new terminal here.
    fn ended_bar(&self, code: Option<i32>, cx: &App) -> Div {
        let colors = cx.theme().colors();
        h_flex()
            .h(px(32.))
            .flex_none()
            .px_2p5()
            .gap_2()
            .bg(colors.surface_background)
            .border_t_1()
            .border_color(colors.border)
            .child(
                Icon::from_path(icon_path("square-terminal"))
                    .size(IconSize::Small)
                    .color(Color::Muted),
            )
            .child(
                Label::new(ended_text(code, self.task_finished))
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
            .children(self.new_terminal_button("again", "Open a new terminal"))
    }

    /// The terminal and what lies over it: the skeleton while history is written, the restore
    /// cover, Jump to latest and the find bar.
    fn body(&self, ended: bool, cx: &App) -> Div {
        let remote = self.terminal.read(cx);
        let cover = self.cover(cx);
        let skeleton = (remote.replaying() && cover.is_none()).then(|| {
            div()
                .absolute()
                .inset_0()
                .p_2()
                .bg(cx.theme().colors().terminal_background)
                .child(SkeletonList::new(6, 28.))
        });
        let jump = (remote.scrolled_up(cx) && !ended).then(|| self.jump());
        let find = self
            .find
            .clone()
            .map(|find| div().absolute().top_1().right_3().child(find));
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .pt_2()
            .pb_2()
            .pl(px(10.))
            .pr(px(2.))
            .bg(cx.theme().colors().terminal_background)
            // The terminal takes the wheel but lets it go on to the grid, which then scrolled
            // too; xterm.js stops it in the web app (T-0162).
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .child(terminal_screen(&self.terminal, ended, cx))
            .children(skeleton)
            .children(cover)
            .children(jump)
            .children(find)
    }
}

impl RenderOnce for TerminalPane {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let status = self.terminal.read(cx).status().clone();
        let ended = match status {
            TerminalStatus::Ended { code } => Some(code),
            _ => None,
        };
        let strip = self.strip(&status, window, cx);
        let body = self.body(ended.is_some(), cx);
        let ended_bar = ended.map(|code| self.ended_bar(code, cx));
        let colors = cx.theme().colors();
        let border = if self.focused {
            colors.border_focused
        } else {
            colors.border_variant
        };
        let on_find = self.on_find.clone();
        let on_maximize = self.on_maximize.clone();
        let pane = v_flex()
            .id(self.key.clone())
            .size_full()
            .min_h_0()
            .overflow_hidden()
            .rounded(px(8.))
            .border_1()
            .border_color(border)
            .when_some(on_find, |pane, run| {
                pane.on_action(move |_: &FindInTerminal, window, cx| run(window, cx))
            })
            .when_some(on_maximize, |pane, run| {
                pane.on_action(move |_: &MaximizePane, window, cx| run(window, cx))
            })
            .child(self.header.focused(self.focused))
            .children(strip)
            .child(body)
            .children(ended_bar);
        with_drop(pane, &self.terminal, cx)
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

    #[test]
    fn another_task_gets_its_own_socket() {
        let one = task("T-1", TaskState::Running);
        let two = task("T-2", TaskState::Running);
        assert!(needs_new_terminal(Some(&one), &two, true, true));
        assert!(needs_new_terminal(None, &two, false, true));
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
    fn the_strip_waits_200_ms_unless_the_terminal_failed() {
        assert!(!strip_shows(false, Duration::from_millis(150)));
        assert!(strip_shows(false, Duration::from_millis(200)));
        assert!(strip_shows(true, Duration::ZERO));
    }

    #[test]
    fn says_the_exit_code_only_when_it_is_not_0_and_the_task_runs() {
        assert_eq!(
            ended_text(Some(2), false),
            "The terminal ended. Exit code 2."
        );
        assert_eq!(ended_text(Some(0), false), "The terminal ended.");
        assert_eq!(ended_text(Some(2), true), "The terminal ended.");
        assert_eq!(ended_text(None, false), "The terminal ended.");
    }
}
