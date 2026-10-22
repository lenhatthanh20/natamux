//! A shell of the daemon in Zed's terminal view (gate 3 of phase 1). Zed's `Terminal` normally
//! owns a local PTY; here it is display-only: the socket's bytes go into its screen, and what it
//! would write to a PTY (keys, pastes, IME text, answers to queries) goes to the socket. The input
//! hook is the one patch on our Zed fork, `Terminal::set_display_only_input`.

use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use gpui::{App, Entity, Focusable, Task, WeakEntity};
use natamux_client::{SessionOptions, TerminalEvent, TerminalHandle, TerminalStatus};
use settings::Settings;
use terminal::terminal_settings::TerminalSettings;
use terminal::{DisplayOnlyWriter, Terminal, TerminalBuilder};
use terminal_view::TerminalView;
use ui::prelude::*;
use util::paths::PathStyle;

use crate::app::source::Source;

/// How often a size change of the pane is looked for. Output and input never wait for it.
const SIZE_CHECK: Duration = Duration::from_millis(100);

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
    size: (u16, u16),
    /// "T-1/1": the task and terminal, for the perf overlay.
    label: String,
    /// Text to type once the socket is open.
    pending: Vec<String>,
    /// On screen: only then it claims the shell's size.
    visible: bool,
    /// When the last key went to this terminal's view, for key to glyph.
    last_key: Option<Instant>,
    /// The parser thread's work, once connected.
    jobs: Option<UnboundedSender<Job>>,
    _tasks: Vec<Task<()>>,
}

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
        let settings = TerminalSettings::get_global(cx).clone();
        let terminal = cx.new(|cx| {
            TerminalBuilder::new_display_only(
                settings.cursor_shape,
                settings.alternate_scroll,
                settings.max_scroll_history_lines,
                cx.entity_id().as_u64(),
                cx.background_executor(),
                PathStyle::local(),
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
        cx.observe_keystrokes(|this, _, window, cx| {
            if this.view.focus_handle(cx).contains_focused(window, cx) {
                this.last_key = Some(Instant::now());
            }
        })
        .detach();
        let watch = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(SIZE_CHECK).await;
                let alive = this.update(cx, |this, cx| this.check_size(&source, &target, cx));
                if alive.is_err() {
                    break;
                }
            }
        });
        RemoteTerminal {
            view,
            terminal,
            handle: None,
            status: TerminalStatus::Connecting,
            size: (0, 0),
            label,
            pending: Vec::new(),
            visible: true,
            last_key: None,
            jobs: None,
            _tasks: vec![watch],
        }
    }

    pub fn status(&self) -> &TerminalStatus {
        &self.status
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

    /// Types into the shell, as keys would; waits until the socket is open.
    pub fn type_text(&mut self, text: &str) {
        match (&self.handle, &self.status) {
            (Some(handle), TerminalStatus::Open) => handle.input(text),
            _ => self.pending.push(text.to_string()),
        }
    }

    /// The grid size the view laid out, once it did.
    fn grid_size(&self, cx: &App) -> Option<(u16, u16)> {
        let content = self.terminal.read(cx).last_content();
        let cols = u16::try_from(content.columns).ok()?.min(1000);
        let rows = u16::try_from(content.screen_lines).ok()?.min(1000);
        (cols > 0 && rows > 0).then_some((cols, rows))
    }

    /// Connects once the view knows its size, so the shell starts at that size; later sends each
    /// new size.
    fn check_size(&mut self, source: &Source, target: &Target, cx: &mut Context<Self>) {
        let Some(size) = self.grid_size(cx) else {
            return;
        };
        if size == self.size {
            return;
        }
        self.size = size;
        match &self.handle {
            Some(handle) => handle.resize(size.0, size.1),
            None => self.connect(source, target, size, cx),
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
            cx.notify();
            return;
        }
        // Zed parses a PTY terminal's output on a background thread; this one does the same, so
        // a flood does not hold up frames (gate 2 of phase 1).
        let writer = match self.terminal.read(cx).display_only_writer() {
            Ok(writer) => writer,
            Err(error) => {
                self.status = TerminalStatus::Failed(error.to_string());
                cx.notify();
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
                            cx.notify();
                            return;
                        }
                    }
                }
                TerminalEvent::CaughtUp { offset } => Job::CaughtUp(offset),
                TerminalEvent::Status(status) => {
                    if status == TerminalStatus::Open {
                        for text in self.pending.drain(..) {
                            handle.input(text);
                        }
                    }
                    self.status = status;
                    cx.notify();
                    continue;
                }
            };
            // Fails only when the parser thread is gone, with the terminal.
            jobs.unbounded_send(job).ok();
        }
    }
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
                if !bytes.is_empty() {
                    writer.write(&bytes);
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
