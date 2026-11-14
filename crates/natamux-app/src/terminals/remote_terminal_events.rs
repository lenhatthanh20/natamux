//! The socket's events of a remote terminal: output to the parser, catch-ups, status, and the
//! screens they are written to. A snapshot of a terminal that already showed a screen goes to a
//! screen of its own (T-0210).

use std::time::Instant;

use futures::channel::oneshot;
use gpui::{Entity, FocusHandle, Focusable, Subscription};
use natamux_client::{TerminalEvent, TerminalStatus};
use ui::prelude::*;

use super::screen::{DisplayOnlyWriter, Terminal, TerminalBounds, TerminalView};
use super::{RemoteTerminal, send_input};
use crate::app::app_log;
use crate::terminals::terminal_parser::{Job, lane};

impl RemoteTerminal {
    /// The socket's events, in order. Output goes to the parser; a snapshot of a terminal that
    /// already showed a screen is written to a new screen, shown once it is complete.
    pub(crate) fn apply(
        &mut self,
        batch: Vec<TerminalEvent>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (Some(handle), Some(jobs)) = (self.handle.clone(), self.jobs.clone()) else {
            return;
        };
        for event in batch {
            let job = match event {
                TerminalEvent::Output { bytes, end } => {
                    if !bytes.is_empty() {
                        crate::perf::overlay::output(&self.label, bytes.len(), cx);
                    }
                    if !self.replaying && self.incoming.is_none() {
                        self.mark_drawn(cx);
                    }
                    Job::Write { bytes, ack: end }
                }
                TerminalEvent::Reset { .. } => match self.reset(window, cx) {
                    Some(job) => job,
                    None => return,
                },
                TerminalEvent::CaughtUp { offset } => Job::CaughtUp {
                    offset,
                    parsed: self.caught_up(cx),
                },
                TerminalEvent::Status(status) => {
                    if status == TerminalStatus::Open {
                        for text in self.pending.drain(..) {
                            send_input(&handle, text);
                        }
                    }
                    if status == TerminalStatus::Reconnecting
                        && self.status != TerminalStatus::Reconnecting
                    {
                        let reason = handle.last_loss().unwrap_or_else(|| "unknown".into());
                        app_log::write(&format!("terminal {}: reconnecting: {reason}", self.label));
                        crate::perf::overlay::reconnect(&self.label, cx);
                        // The catch-up after a reconnect is not a show.
                        self.shown_at = None;
                    }
                    self.status = status;
                    self.changed(cx);
                    continue;
                }
            };
            // Fails only when the parser is gone, with the terminal.
            jobs.unbounded_send(job).ok();
        }
    }

    /// The perf report counts a skeleton only on a terminal that drew before.
    fn mark_drawn(&mut self, cx: &mut Context<Self>) {
        if !self.drawn {
            self.drawn = true;
            crate::perf::overlay::drew(&self.label, cx);
        }
    }

    /// A snapshot starts. The first one draws on the shown screen behind the skeleton; a later
    /// one gets a screen of its own, so the old screen stays until the new one is complete.
    fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<Job> {
        if !self.drawn {
            self.replaying = true;
            crate::perf::overlay::skeleton(&self.label, cx);
            self.changed(cx);
            // It keeps the pane's size: the daemon replays at the shell's size, which this pane
            // claimed.
            let writer = self.terminal.update(cx, |terminal, cx| {
                terminal.replace_display_output(&[], cx)?;
                terminal.display_only_writer()
            });
            return self.writer_job(writer, cx);
        }
        let bounds = self.terminal.read(cx).last_content().terminal_bounds;
        let screen = build_screen(self.history, Some(bounds), window, cx);
        let pace = lane(self.focused, self.visible).pace;
        let writer = screen.terminal.update(cx, |terminal, _| {
            terminal.set_wakeup_pace(pace);
            if let Some(sink) = &self.input_sink {
                terminal.set_display_only_input(sink.clone())?;
            }
            terminal.display_only_writer()
        });
        self.incoming = Some(screen);
        self.writer_job(writer, cx)
    }

    fn writer_job(
        &mut self,
        writer: anyhow::Result<DisplayOnlyWriter>,
        cx: &mut Context<Self>,
    ) -> Option<Job> {
        match writer {
            Ok(writer) => Some(Job::Writer(Box::new(writer))),
            Err(error) => {
                self.status = TerminalStatus::Failed(error.to_string());
                self.changed(cx);
                None
            }
        }
    }

    /// The catch-up's bytes all came. Its screen is shown once the parser wrote them all: the
    /// old screen stays until then. Gives the sender the parser tells.
    fn caught_up(&mut self, cx: &mut Context<Self>) -> oneshot::Sender<()> {
        self.replaying = false;
        self.mark_drawn(cx);
        self.changed(cx);
        let (parsed, ready) = oneshot::channel();
        // This catch-up's own screen: a later snapshot brings its own.
        let incoming = self.incoming.take();
        let shown_at = self.shown_at.take().filter(|_| self.visible);
        let window = self.window;
        // Swaps run in order. Dropping a waiting one would drop its screen, and the pane would
        // stay on the old screen while the output goes to the dropped one.
        let earlier = self.swap.take();
        self.swap = Some(cx.spawn(async move |this, cx| {
            if let Some(earlier) = earlier {
                earlier.await;
            }
            if ready.await.is_err() {
                return;
            }
            cx.update_window(window, |_, window, cx| {
                this.update(cx, |this, cx| {
                    this.show_parsed(incoming, shown_at, window, cx)
                })
            })
            .ok();
        }));
        parsed
    }

    fn show_parsed(
        &mut self,
        incoming: Option<Screen>,
        shown_at: Option<Instant>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(screen) = incoming {
            if self.has_focus(window, cx) {
                window.focus(&screen.view.focus_handle(cx), cx);
            }
            self.show(screen, cx);
            // The new screen got the pace of when its snapshot started.
            self.follow_lane(cx);
        }
        if let Some(at) = shown_at {
            crate::perf::overlay::shown(&self.label, at.elapsed(), cx);
        }
        self.changed(cx);
    }
}

/// The terminal, its view, and the view's focus listeners.
pub(super) struct Screen {
    pub(super) terminal: Entity<Terminal>,
    pub(super) view: Entity<TerminalView>,
    pub(super) _focus: Vec<Subscription>,
}

/// `bounds`: the grid the last screen laid out, so the new one starts at it.
#[cfg(not(feature = "own-terminal"))]
pub(super) fn build_screen(
    history: Option<usize>,
    bounds: Option<TerminalBounds>,
    window: &mut Window,
    cx: &mut Context<RemoteTerminal>,
) -> Screen {
    use gpui::WeakEntity;
    use settings::Settings;
    use terminal::TerminalBuilder;
    use terminal::terminal_settings::TerminalSettings;
    use util::paths::PathStyle;

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
        let mut view = TerminalView::new(
            terminal.clone(),
            WeakEntity::new_invalid(),
            None,
            WeakEntity::new_invalid(),
            window,
            cx,
        );
        // No Zed workspace here to handle New Terminal, New Center Terminal or Close Terminal Tab
        // from the right-click menu (T-0247).
        view.set_show_workspace_actions(false, cx);
        view
    });
    let focus = view.focus_handle(cx);
    Screen {
        terminal,
        view,
        _focus: watch_focus(&focus, window, cx).into(),
    }
}

/// The own terminal: its cursor starts as a block and alternate scroll is on (6.2.2); its menu
/// has only what works without a Zed workspace.
#[cfg(feature = "own-terminal")]
pub(super) fn build_screen(
    history: Option<usize>,
    bounds: Option<TerminalBounds>,
    window: &mut Window,
    cx: &mut Context<RemoteTerminal>,
) -> Screen {
    use crate::terminals::term::view::MenuFocus;

    let terminal = cx.new(|cx| Terminal::new(history, bounds, cx));
    let view = cx.new(|cx| TerminalView::new(terminal.clone(), window, cx));
    let focus = view.focus_handle(cx);
    let mut subscriptions = Vec::from(watch_focus(&focus, window, cx));
    let menu = cx.subscribe(&view, |this, _, event: &MenuFocus, cx| {
        if event.0 {
            this.focus_came(cx);
        } else {
            this.set_focused(false, cx);
        }
    });
    subscriptions.push(menu);
    Screen {
        terminal,
        view,
        _focus: subscriptions,
    }
}

impl RemoteTerminal {
    fn focus_came(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = &self.handle {
            #[cfg(test)]
            super::sent::claimed();
            handle.claim();
        }
        self.set_focused(true, cx);
    }
}

/// Follows the focus of a screen's view. Focus claims the shell's size, as the web's textarea
/// focus does: after a resume the shell may still have another client's size.
pub(super) fn watch_focus(
    focus: &FocusHandle,
    window: &mut Window,
    cx: &mut Context<RemoteTerminal>,
) -> [Subscription; 2] {
    let focus_in = cx.on_focus_in(focus, window, |this, _, cx| {
        // Back from the own view's menu: the focus never left as far as the shell knows.
        #[cfg(feature = "own-terminal")]
        if this.view.read(cx).back_from_menu {
            return;
        }
        this.focus_came(cx);
    });
    // Not blur: focus in a child, such as the context menu, still counts as focused.
    let focus_out = cx.on_focus_out(focus, window, |this, _, _window, cx| {
        // The own view's menu is drawn outside its focus, but still counts as the view's.
        #[cfg(feature = "own-terminal")]
        if this.view.read(cx).menu_holds_focus(_window, cx) {
            return;
        }
        this.set_focused(false, cx)
    });
    [focus_in, focus_out]
}
