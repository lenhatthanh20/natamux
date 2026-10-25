//! Tools > Bench: 10 Terminals (gate 2): ten shells of one task, each printing about 1 MB/s of
//! coloured text, side by side, while the perf overlay measures.

use std::sync::Arc;

use gpui::{App, Entity, FocusHandle, Focusable};
use natamux_proto::tasks::task::Task;
use ui::prelude::*;

use crate::app::source::Source;
use crate::terminals::remote_terminal::{RemoteTerminal, Target};

/// Far from the terminals a task's stages use (1, 2, 5, 6 and up), so the bench never types into
/// an agent.
pub const FIRST_TERMINAL: u16 = 901;
pub const TERMINALS: u16 = 10;
/// A quiet shell for typing after the bench (`--gate2`); 911 is the IME check's.
pub const TYPING_TERMINAL: u16 = 912;
// The daemon names terminals 1 to 999.
const _: () = assert!(FIRST_TERMINAL + TERMINALS - 1 < TYPING_TERMINAL && TYPING_TERMINAL <= 999);
const COLUMNS: usize = 5;

/// 1 MiB of coloured lines, then a second's rest: about 1 MB/s. POSIX sh, so any shell runs it.
pub const FLOOD: &str = concat!(
    "clear; L=$(printf '\\033[1;32mnatamux\\033[0m \\033[33mbench\\033[0m \\033[36m0123456789\\033[0m ",
    "abcdefghijklmnopqrstuvwxyz \\033[35mABCDEFGHIJKLMNOPQRSTUVWXYZ\\033[0m'); ",
    "while :; do yes \"$L\" | head -c 1048576; sleep 1; done\r",
);

pub struct BenchView {
    source: Arc<Source>,
    task: Task,
    terminals: Vec<(String, Entity<RemoteTerminal>)>,
}

impl BenchView {
    pub fn new(
        source: Arc<Source>,
        task: Task,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::with_terminals(
            source,
            task,
            FIRST_TERMINAL..FIRST_TERMINAL + TERMINALS,
            Some(FLOOD),
            window,
            cx,
        )
    }

    /// One quiet shell of the task, to type into after the bench (`--gate2`).
    pub fn typing(
        source: Arc<Source>,
        task: Task,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::with_terminals(
            source,
            task,
            TYPING_TERMINAL..TYPING_TERMINAL + 1,
            None,
            window,
            cx,
        )
    }

    fn with_terminals(
        source: Arc<Source>,
        task: Task,
        numbers: std::ops::Range<u16>,
        typed: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let terminals = numbers
            .map(|number| {
                let name = number.to_string();
                let target = Target {
                    task_id: task.id.clone(),
                    terminal: name.clone(),
                };
                let source = source.clone();
                let terminal = cx.new(|cx| {
                    let mut terminal = RemoteTerminal::new(source, target, window, cx);
                    if let Some(text) = typed {
                        terminal.type_text(text);
                    }
                    terminal
                });
                (name, terminal)
            })
            .collect();
        BenchView {
            source,
            task,
            terminals,
        }
    }

    /// The focus of the first terminal, to give it the keys.
    pub fn first_focus(&self, cx: &App) -> Option<FocusHandle> {
        let (_, terminal) = self.terminals.first()?;
        Some(terminal.read(cx).view.focus_handle(cx))
    }

    /// Stops every flood with Ctrl+C and ends the bench's shells. The future gives one line for
    /// each shell the daemon could not end.
    pub fn stop(&mut self, cx: &mut Context<Self>) -> impl Future<Output = Vec<String>> + use<> {
        let terminals = std::mem::take(&mut self.terminals);
        let closes: Vec<_> = terminals
            .into_iter()
            .map(|(name, terminal)| {
                terminal.update(cx, |terminal, _| terminal.type_text("\x03"));
                let close = self.source.close_terminal(&self.task.id, &name);
                async move {
                    let result = close.await;
                    // Kept until the shell is gone, so its Ctrl+C goes out first.
                    drop(terminal);
                    result
                        .err()
                        .map(|error| format!("Terminal {name}: {error}"))
                }
            })
            .collect();
        cx.notify();
        async move {
            futures::future::join_all(closes)
                .await
                .into_iter()
                .flatten()
                .collect()
        }
    }
}

impl Render for BenchView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let rows = self.terminals.chunks(COLUMNS).map(|row| {
            h_flex()
                .flex_1()
                .min_h_0()
                .gap_px()
                .children(row.iter().map(|(name, terminal)| {
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .bg(colors.editor_background)
                        .child(
                            div().px_2().py_0p5().child(
                                Label::new(format!("Terminal {name}"))
                                    .size(LabelSize::XSmall)
                                    .color(Color::Muted),
                            ),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_h_0()
                                .child(terminal.read(cx).view.clone()),
                        )
                }))
        });
        v_flex()
            .size_full()
            .gap_px()
            .bg(colors.border)
            .child(
                h_flex()
                    .px_3()
                    .py_1p5()
                    .bg(colors.panel_background)
                    .gap_2()
                    .child(Label::new(format!("Bench on {}", self.task.id)))
                    .child(
                        Label::new("Tools > Stop Bench ends it.")
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    ),
            )
            .children(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_flood_is_one_line_of_plain_ascii_ending_in_enter() {
        assert!(FLOOD.is_ascii());
        assert!(FLOOD.ends_with('\r'));
        assert_eq!(FLOOD.matches('\r').count(), 1);
        assert!(!FLOOD.contains('\n'));
    }
}
