//! A shell of the daemon in Zed's terminal view (gate 3 of phase 1). Zed's `Terminal` normally
//! owns a local PTY; here it is display-only: the socket's bytes go into its screen, and what it
//! would write to a PTY (keys, pastes, IME text, answers to queries) goes to the socket. The input
//! hook is the one patch on our Zed fork, `Terminal::set_display_only_input`.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context as TaskContext, Poll};
use std::time::{Duration, Instant};

use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use gpui::{App, Entity, Focusable, Task, WeakEntity};
use natamux_client::{SessionOptions, TerminalEvent, TerminalHandle, TerminalStatus};
use settings::Settings;
use terminal::terminal_settings::TerminalSettings;
use terminal::{DisplayOnlyWriter, Modes, Terminal, TerminalBounds, TerminalBuilder};
use terminal_view::TerminalView;
use ui::prelude::*;
use util::paths::PathStyle;

use super::paste_rules::bracketed;
use crate::app::source::Source;
use crate::terminals::terminal_size::{
    FIRST_HISTORY_COLS, SizeStep, history_lines, size_step, waits,
};

/// A new pane size goes to the shell once it held this long (two looks, T-0173). Output and input
/// never wait for it.
const SIZE_SETTLE: Duration = Duration::from_millis(100);
/// The most output parsed before the parser lets other work of the thread pool run. The paint
/// waits for the terminal's lock while a piece is parsed.
const PARSE_PIECE: usize = 4 * 1024;
/// Work for the parser thread of one terminal, in the order the socket gave it.
enum Job {
    /// Parse these bytes, then ack `ack` when it is set.
    Write { bytes: Vec<u8>, ack: Option<u64> },
    /// The catch-up's bytes are all parsed once the jobs before this one are done.
    CaughtUp(u64),
    /// A snapshot replaced the screen: write to the new one from here on. Boxed: a writer is
    /// large and the other jobs are small.
    Writer(Box<DisplayOnlyWriter>),
}

pub struct RemoteTerminal {
    pub view: Entity<TerminalView>,
    terminal: Entity<Terminal>,
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
    /// When the last key went to this terminal's view, for key to glyph.
    last_key: Option<Instant>,
    /// The parser thread's work, once connected.
    jobs: Option<UnboundedSender<Job>>,
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
        let (terminal, view) = build_screen(history, None, window, cx);
        cx.observe_keystrokes(|this, _, window, cx| {
            if this.view.focus_handle(cx).contains_focused(window, cx) {
                this.last_key = Some(Instant::now());
            }
        })
        .detach();
        RemoteTerminal {
            view,
            terminal,
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
            last_key: None,
            jobs: None,
            replaying: false,
            history,
            _tasks: Vec::new(),
        }
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

    /// The pane came on screen or left it, such as when a tool covers it.
    pub fn set_visible(&mut self, visible: bool) {
        if visible == self.visible {
            return;
        }
        self.visible = visible;
        if let Some(handle) = &self.handle {
            handle.set_visible(visible);
        }
    }

    /// The pane shows this terminal's state. The views that show it are cached and draw again
    /// only when a view in them changed, so the terminal's own view is told too.
    fn changed(&mut self, cx: &mut Context<Self>) {
        cx.notify();
        self.view.update(cx, |_, cx| cx.notify());
    }

    /// Types into the shell, as keys would; waits until the socket is open.
    pub fn type_text(&mut self, text: &str) {
        match (&self.handle, &self.status) {
            (Some(handle), TerminalStatus::Open) => handle.input(text),
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
        let (terminal, view) = build_screen(history, Some(bounds), window, cx);
        if focused {
            window.focus(&view.focus_handle(cx), cx);
        }
        (self.terminal, self.view, self.history) = (terminal, view, history);
        self.changed(cx);
    }

    /// Connects once the view knows its size, so the shell starts at that size; later sends a
    /// new size once it settled. `window`: None from the settle timer.
    fn check_size(&mut self, window: Option<&mut Window>, cx: &mut Context<Self>) {
        let Some(now) = self.grid_size(cx) else {
            return;
        };
        let seen = self.seen.replace(now);
        match size_step(self.handle.is_some(), seen, now, self.size) {
            Some(SizeStep::Connect(size)) => {
                self.size = size;
                if let Some(window) = window {
                    self.build_at(size.0, window, cx);
                }
                let (source, target) = (self.source.clone(), self.target.clone());
                self.connect(&source, &target, size, cx);
            }
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
        let attached = self
            .terminal
            .update(cx, |terminal, _| terminal.set_display_only_input(sink));
        if let Err(error) = attached {
            self.status = TerminalStatus::Failed(error.to_string());
            self.changed(cx);
            return;
        }
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
        self._tasks.push(
            cx.background_executor()
                .spawn(parse(writer, handle.clone(), queued)),
        );
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
        self._tasks.push(cx.spawn(async move |this, cx| {
            while let Some(first) = events.next().await {
                let mut batch = vec![first];
                while let Ok(event) = events.try_recv() {
                    batch.push(event);
                }
                if this.update(cx, |this, cx| this.apply(batch, cx)).is_err() {
                    break;
                }
            }
        }));
        self.handle = Some(handle);
    }

    fn apply(&mut self, batch: Vec<TerminalEvent>, cx: &mut Context<Self>) {
        let (Some(handle), Some(jobs)) = (self.handle.clone(), self.jobs.clone()) else {
            return;
        };
        for event in batch {
            let job = match event {
                TerminalEvent::Output { bytes, end } => {
                    if !bytes.is_empty() {
                        crate::perf::overlay::output(&self.label, bytes.len(), cx);
                    }
                    Job::Write { bytes, ack: end }
                }
                TerminalEvent::Reset { .. } => {
                    self.replaying = true;
                    self.changed(cx);
                    // A snapshot draws the whole screen again on a clean one. It keeps the pane's
                    // size: the daemon replays at the shell's size, which this pane claimed.
                    let writer = self.terminal.update(cx, |terminal, cx| {
                        terminal.replace_display_output(&[], cx)?;
                        terminal.display_only_writer()
                    });
                    match writer {
                        Ok(writer) => Job::Writer(Box::new(writer)),
                        Err(error) => {
                            self.status = TerminalStatus::Failed(error.to_string());
                            self.changed(cx);
                            return;
                        }
                    }
                }
                TerminalEvent::CaughtUp { offset } => {
                    self.replaying = false;
                    self.changed(cx);
                    Job::CaughtUp(offset)
                }
                TerminalEvent::Status(status) => {
                    if status == TerminalStatus::Open {
                        for text in self.pending.drain(..) {
                            handle.input(text);
                        }
                    }
                    self.status = status;
                    self.changed(cx);
                    continue;
                }
            };
            // Fails only when the parser thread is gone, with the terminal.
            jobs.unbounded_send(job).ok();
        }
    }
}

/// Pending once, so other work of the thread pool runs between two pieces of a flood.
struct YieldNow(bool);

impl Future for YieldNow {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<()> {
        if self.0 {
            return Poll::Ready(());
        }
        self.0 = true;
        cx.waker().wake_by_ref();
        Poll::Pending
    }
}

/// Zed's terminal and its view. `bounds`: the grid the last one laid out, so the new one starts
/// at it.
fn build_screen(
    history: Option<usize>,
    bounds: Option<TerminalBounds>,
    window: &mut Window,
    cx: &mut Context<RemoteTerminal>,
) -> (Entity<Terminal>, Entity<TerminalView>) {
    let settings = TerminalSettings::get_global(cx).clone();
    let terminal = cx.new(|cx| {
        TerminalBuilder::new_display_only_with_bounds(
            settings.cursor_shape,
            settings.alternate_scroll,
            history,
            cx.entity_id().as_u64(),
            cx.background_executor(),
            PathStyle::local(),
            bounds.unwrap_or_default(),
        )
        .subscribe(cx)
    });
    let view = cx.new(|cx| {
        TerminalView::new(
            terminal.clone(),
            WeakEntity::new_invalid(),
            None,
            WeakEntity::new_invalid(),
            window,
            cx,
        )
    });
    // Focus claims the shell's size, as the web's textarea focus does: after a resume the shell
    // may still have another client's size.
    let focus = view.focus_handle(cx);
    cx.on_focus_in(&focus, window, |this, _, _| {
        if let Some(handle) = &this.handle {
            handle.claim();
        }
    })
    .detach();
    (terminal, view)
}

/// The parser thread of one terminal. It acks only what it parsed, so a flood slows down to the
/// speed it parses at.
async fn parse(
    mut writer: DisplayOnlyWriter,
    handle: Arc<TerminalHandle>,
    mut jobs: UnboundedReceiver<Job>,
) {
    while let Some(job) = jobs.next().await {
        match job {
            Job::Write { bytes, ack } => {
                let mut pieces = bytes.chunks(PARSE_PIECE).peekable();
                while let Some(piece) = pieces.next() {
                    writer.write(piece);
                    if pieces.peek().is_some() {
                        YieldNow(false).await;
                    }
                }
                if let Some(offset) = ack {
                    handle.parsed(offset);
                }
            }
            Job::CaughtUp(offset) => handle.caught_up(offset),
            Job::Writer(new) => writer = *new,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PARSE_PIECE;

    #[test]
    fn parses_in_pieces_of_4_kib() {
        let flood = vec![b'x'; 64 * 1024];
        assert_eq!(flood.chunks(PARSE_PIECE).count(), 16);
        assert!(flood.chunks(PARSE_PIECE).all(|piece| piece.len() <= 4096));
    }
}
