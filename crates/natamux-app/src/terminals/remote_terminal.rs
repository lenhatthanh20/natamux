//! A shell of the daemon in Zed's terminal view (gate 3 of phase 1). Zed's `Terminal` normally
//! owns a local PTY; here it is display-only: the socket's bytes go into its screen, and what it
//! would write to a PTY (keys, pastes, IME text, answers to queries) goes to the socket. Our Zed
//! fork adds three hooks for it: `Terminal::set_display_only_input` (the input),
//! `display_only_writer` (parsing off the main thread) and `set_wakeup_pace` (how often it wakes).

use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::StreamExt;
use futures::channel::mpsc::{UnboundedSender, unbounded};
use gpui::{AnyWindowHandle, App, Entity, Focusable, Priority, Subscription, Task};
use natamux_client::{SessionOptions, TerminalHandle, TerminalStatus};
use terminal::{Modes, Terminal};
use terminal_view::TerminalView;
use ui::prelude::*;

use self::events::{Screen, build_screen};
use super::paste_rules::bracketed;
use super::terminal_parser::{Job, Parser, lane};
use crate::app::source::Source;
use crate::terminals::terminal_size::{
    FIRST_HISTORY_COLS, SizeStep, history_lines, size_step, waits,
};

/// A new pane size goes to the shell once it held this long (two looks, T-0173). Output and input
/// never wait for it.
const SIZE_SETTLE: Duration = Duration::from_millis(100);

pub struct RemoteTerminal {
    pub view: Entity<TerminalView>,
    terminal: Entity<Terminal>,
    /// The shown view's focus listeners: they go with their screen.
    _focus: [Subscription; 2],
    handle: Option<Arc<TerminalHandle>>,
    status: TerminalStatus,
    /// The size the shell was told.
    size: (u16, u16),
    /// The grid size at the last check, for the settle rule.
    seen: Option<(u16, u16)>,
    /// The grid size at the last layout that was looked at.
    laid: Option<(u16, u16)>,
    /// Counts the waits for a new size to hold; only the newest one looks again.
    settle: u64,
    source: Arc<Source>,
    target: Target,
    /// "T-1/1": the task and terminal, for the perf overlay.
    label: String,
    /// Which shell of which task this is, for a drop of a path.
    task_id: String,
    name: String,
    /// Text to type once the socket is open.
    pending: Vec<String>,
    /// On screen: only then it claims the shell's size.
    visible: bool,
    /// Its view has the focus: its screen wakes and parses first.
    focused: bool,
    /// When it came on screen, until the end of its catch-up, for the perf report. The daemon
    /// answers every show of a parked terminal with a catch-up, empty when nothing was missed,
    /// so a frame drawn before it never ends the show (T-0210 review 3).
    shown_at: Option<Instant>,
    /// It showed a screen once. A later snapshot is written to a screen of its own, shown when
    /// complete, so the pane never goes blank (T-0210).
    drawn: bool,
    /// The screen a snapshot is being written to, while the old one stays on screen.
    incoming: Option<Screen>,
    /// Where Zed's terminals send what they would write to a PTY.
    input_sink: Option<UnboundedSender<Vec<u8>>>,
    /// The window it was made in: a snapshot's screen needs it for its view.
    window: AnyWindowHandle,
    /// When the last key went to this terminal's view, for key to glyph.
    last_key: Option<Instant>,
    /// The parser's work, once connected.
    jobs: Option<UnboundedSender<Job>>,
    /// The parser; None while it hands over to another priority.
    parser: Option<Task<Option<Parser>>>,
    /// The priority the parser runs at, or will once the handover ends.
    priority: Priority,
    /// The priority the running parser was spawned at.
    #[cfg_attr(not(test), allow(dead_code))]
    spawned_at: Priority,
    handover: Option<Task<()>>,
    /// Waits for the parser to finish a catch-up, then shows its screen.
    swap: Option<Task<()>>,
    /// A snapshot is being written: the pane shows its skeleton, not the history scrolling by.
    replaying: bool,
    /// The history limit Zed's terminal was built with.
    history: Option<usize>,
    _tasks: Vec<Task<()>>,
}

#[derive(Clone)]
pub struct Target {
    pub task_id: String,
    pub terminal: String,
}

impl RemoteTerminal {
    pub fn new(
        source: Arc<Source>,
        target: Target,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let label = format!("{}/{}", target.task_id, target.terminal);
        let (task_id, name) = (target.task_id.clone(), target.terminal.clone());
        let history = history_lines(FIRST_HISTORY_COLS);
        let screen = build_screen(history, None, window, cx);
        cx.observe_keystrokes(|this, _, window, cx| {
            if this.view.focus_handle(cx).contains_focused(window, cx) {
                this.last_key = Some(Instant::now());
            }
        })
        .detach();
        let window = window.window_handle();
        RemoteTerminal {
            view: screen.view,
            terminal: screen.terminal,
            _focus: screen._focus,
            handle: None,
            status: TerminalStatus::Connecting,
            size: (0, 0),
            seen: None,
            laid: None,
            settle: 0,
            source,
            target,
            label,
            task_id,
            name,
            pending: Vec::new(),
            visible: true,
            focused: false,
            shown_at: None,
            drawn: false,
            incoming: None,
            input_sink: None,
            window,
            last_key: None,
            jobs: None,
            parser: None,
            priority: Priority::Medium,
            spawned_at: Priority::Medium,
            handover: None,
            swap: None,
            replaying: false,
            history,
            _tasks: Vec::new(),
        }
    }

    /// The bench's own terminals: their perf numbers stay apart from the panes' terminals of
    /// the same shells, so a pane's first attach is no flash.
    pub fn set_bench_label(&mut self) {
        self.label = format!("bench:{}", self.label);
    }

    pub fn task_id(&self) -> &str {
        &self.task_id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn status(&self) -> &TerminalStatus {
        &self.status
    }

    /// Zed's terminal, for the find bar.
    pub fn terminal(&self) -> &Entity<Terminal> {
        &self.terminal
    }

    /// For the cache's tests: the fake daemon's terminals never end.
    #[cfg(test)]
    pub fn set_status(&mut self, status: TerminalStatus) {
        self.status = status;
    }

    pub fn replaying(&self) -> bool {
        self.replaying
    }

    /// The user scrolled up into the history: "Jump to latest" shows.
    pub fn scrolled_up(&self, cx: &App) -> bool {
        self.terminal.read(cx).last_content().display_offset > 0
    }

    pub fn jump_to_latest(&self, cx: &mut App) {
        self.terminal.update(cx, |terminal, cx| {
            terminal.scroll_to_bottom();
            cx.notify();
        });
    }

    /// The pane came on screen or left it, such as when a tool covers it. Off screen the daemon
    /// parks it: no output until it is shown, then one catch-up.
    pub fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        if visible == self.visible {
            return;
        }
        self.visible = visible;
        self.shown_at = visible.then(Instant::now);
        if let Some(handle) = &self.handle {
            handle.set_visible(visible);
        }
        self.follow_lane(cx);
    }

    /// The view got or lost the focus.
    fn set_focused(&mut self, focused: bool, cx: &mut Context<Self>) {
        if focused != self.focused {
            self.focused = focused;
            self.follow_lane(cx);
        }
    }

    /// Wakes the screen and runs the parser as its state asks: focused, shown or hidden.
    fn follow_lane(&mut self, cx: &mut Context<Self>) {
        let lane = lane(self.focused, self.visible);
        self.terminal
            .update(cx, |terminal, _| terminal.set_wakeup_pace(lane.pace));
        if lane.priority == self.priority {
            return;
        }
        self.priority = lane.priority;
        // A handover on its way spawns at the newest priority when it ends.
        let (Some(parser), Some(jobs), Some(handle)) =
            (self.parser.take(), self.jobs.clone(), self.handle.clone())
        else {
            return;
        };
        if jobs.unbounded_send(Job::Handoff).is_err() {
            return;
        }
        self.handover = Some(cx.spawn(async move |this, cx| {
            let Some(parser) = parser.await else {
                return;
            };
            this.update(cx, |this, cx| {
                let executor = cx.background_executor().clone();
                this.spawned_at = this.priority;
                this.parser = Some(parser.spawn(handle, this.priority, &executor));
            })
            .ok();
        }));
    }

    /// The pane shows this terminal's state. The views that show it are cached and draw again
    /// only when a view in them changed, so the terminal's own view is told too.
    fn changed(&mut self, cx: &mut Context<Self>) {
        cx.notify();
        self.view.update(cx, |_, cx| cx.notify());
    }

    /// Types into the shell, as keys would; waits until the socket is open. A paused one wakes.
    pub fn type_text(&mut self, text: &str) {
        match (&self.handle, &self.status) {
            (Some(h), TerminalStatus::Open | TerminalStatus::Paused { .. }) => h.input(text),
            _ => self.pending.push(text.to_string()),
        }
    }

    /// Pastes as xterm does: wrapped in the bracketed paste marks when the program asked for them
    /// (the display terminal's mode), with no Enter.
    pub fn paste_text(&mut self, text: &str, cx: &App) {
        let on = self
            .terminal
            .read(cx)
            .last_content()
            .mode
            .contains(Modes::BRACKETED_PASTE);
        self.type_text(&bracketed(text, on));
    }

    /// The grid size the view laid out; 0 before it did.
    fn grid_size(&self, cx: &App) -> Option<(u16, u16)> {
        let content = self.terminal.read(cx).last_content();
        let cols = u16::try_from(content.columns).ok()?.min(1000);
        let rows = u16::try_from(content.screen_lines).ok()?.min(1000);
        Some((cols, rows))
    }

    /// The view laid out at another grid size than the last one looked at. Read only, so a
    /// paint can ask it.
    pub fn layout_changed(&self, cx: &App) -> bool {
        self.grid_size(cx) != self.laid
    }

    /// The view laid out at a new grid size: connect at it, or send it once it held.
    pub fn laid_out(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.laid = self.grid_size(cx);
        self.check_size(Some(window), cx);
    }

    /// Zed's terminal keeps the history limit it was built with, and the pane's width is known
    /// only once the view laid out. So the terminal is built again at its first size, before the
    /// shell connects and any output comes, with the daemon's rule at that width. A later resize
    /// keeps the limit.
    fn build_at(&mut self, cols: u16, window: &mut Window, cx: &mut Context<Self>) {
        let history = history_lines(cols);
        if history == self.history {
            return;
        }
        let bounds = self.terminal.read(cx).last_content().terminal_bounds;
        let focused = self.view.focus_handle(cx).contains_focused(window, cx);
        let screen = build_screen(history, Some(bounds), window, cx);
        if focused {
            window.focus(&screen.view.focus_handle(cx), cx);
        }
        self.show(screen, cx);
        self.history = history;
        self.follow_lane(cx);
        self.changed(cx);
    }

    /// Puts `screen` on the pane; the old screen goes, with its focus listeners. The old view is
    /// told first: only it is in the drawn tree, so only it makes its cached parents draw again
    /// and take the new view.
    fn show(&mut self, screen: Screen, cx: &mut Context<Self>) {
        self.view.update(cx, |_, cx| cx.notify());
        (self.terminal, self.view, self._focus) = (screen.terminal, screen.view, screen._focus);
    }

    /// Connects a terminal that never lays out, such as a hidden bench shell, at `size`.
    pub fn connect_unshown(
        &mut self,
        size: (u16, u16),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.handle.is_none() {
            self.connect_at(size, Some(window), cx);
        }
    }

    /// `window`: None from the settle timer; the terminal is then not built again for the width.
    fn connect_at(
        &mut self,
        size: (u16, u16),
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        self.size = size;
        if let Some(window) = window {
            self.build_at(size.0, window, cx);
        }
        let (source, target) = (self.source.clone(), self.target.clone());
        self.connect(&source, &target, size, cx);
    }

    /// Connects once the view knows its size, so the shell starts at that size; later sends a
    /// new size once it settled. `window`: None from the settle timer.
    fn check_size(&mut self, window: Option<&mut Window>, cx: &mut Context<Self>) {
        let Some(now) = self.grid_size(cx) else {
            return;
        };
        let seen = self.seen.replace(now);
        match size_step(self.handle.is_some(), seen, now, self.size) {
            Some(SizeStep::Connect(size)) => self.connect_at(size, window, cx),
            Some(SizeStep::Resize(size)) => {
                self.size = size;
                if let Some(handle) = &self.handle {
                    handle.resize(size.0, size.1);
                }
            }
            None => {}
        }
        // A later layout or look starts a new wait; the older one then does nothing.
        self.settle += 1;
        if waits(now, self.size) {
            let settle = self.settle;
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(SIZE_SETTLE).await;
                this.update(cx, |this, cx| {
                    if this.settle == settle {
                        this.check_size(None, cx);
                    }
                })
                .ok();
            })
            .detach();
        }
    }

    fn connect(
        &mut self,
        source: &Source,
        target: &Target,
        (cols, rows): (u16, u16),
        cx: &mut Context<Self>,
    ) {
        let mut handle = source.open_terminal(SessionOptions::new(
            &target.task_id,
            &target.terminal,
            cols,
            rows,
            self.visible,
        ));
        let mut events = std::mem::replace(&mut handle.events, unbounded().1);
        let handle = Arc::new(handle);

        let (sink, mut input) = unbounded::<Vec<u8>>();
        let attached = self.terminal.update(cx, |terminal, _| {
            terminal.set_display_only_input(sink.clone())
        });
        if let Err(error) = attached {
            self.status = TerminalStatus::Failed(error.to_string());
            self.changed(cx);
            return;
        }
        self.input_sink = Some(sink);
        // Zed parses a PTY terminal's output on a background thread; this one does the same, so
        // a flood does not hold up frames (gate 2 of phase 1).
        let writer = match self.terminal.read(cx).display_only_writer() {
            Ok(writer) => writer,
            Err(error) => {
                self.status = TerminalStatus::Failed(error.to_string());
                self.changed(cx);
                return;
            }
        };
        let (jobs, queued) = unbounded();
        let lane = lane(self.focused, self.visible);
        (self.priority, self.spawned_at) = (lane.priority, lane.priority);
        self.terminal
            .update(cx, |terminal, _| terminal.set_wakeup_pace(lane.pace));
        self.parser = Some(Parser::new(writer, queued).spawn(
            handle.clone(),
            lane.priority,
            cx.background_executor(),
        ));
        self.jobs = Some(jobs);

        let typed = handle.clone();
        self._tasks.push(cx.spawn(async move |this, cx| {
            while let Some(bytes) = input.next().await {
                typed.input(String::from_utf8_lossy(&bytes).into_owned());
                // Only a key that sent input here counts for key to glyph.
                let counted = this.update(cx, |this, cx| {
                    if let Some(at) = this.last_key.take() {
                        crate::perf::overlay::key(&this.label, at, cx);
                    }
                });
                if counted.is_err() {
                    break;
                }
            }
        }));
        let window = self.window;
        self._tasks.push(cx.spawn(async move |this, cx| {
            while let Some(first) = events.next().await {
                let mut batch = vec![first];
                while let Ok(event) = events.try_recv() {
                    batch.push(event);
                }
                // A snapshot's own screen needs a window for its view.
                let applied = cx.update_window(window, |_, window, cx| {
                    this.update(cx, |this, cx| this.apply(batch, window, cx))
                });
                if !matches!(applied, Ok(Ok(()))) {
                    break;
                }
            }
        }));
        self.handle = Some(handle);
    }
}

#[path = "remote_terminal_events.rs"]
mod events;
#[cfg(test)]
#[path = "remote_terminal_tests.rs"]
mod tests;
