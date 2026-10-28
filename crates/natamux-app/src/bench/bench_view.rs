//! Tools > Bench: 10 Terminals (gate 2): ten shells of one task, each printing about 1 MB/s of
//! coloured text, side by side, while the perf overlay measures. `--gate2` can bench up to 60
//! shells printing an agent's TUI (`--terminals`, `--output`); the ones past the grid stay hidden.

use std::sync::Arc;

use gpui::{App, Entity, FocusHandle, Focusable};
use natamux_proto::tasks::task::Task;
use ui::prelude::*;

use crate::app::source::Source;
use crate::terminals::pane_rules::MAX_PANES;
use crate::terminals::remote_terminal::{RemoteTerminal, Target};
use crate::terminals::terminal_screen::terminal_screen;

/// Far from the terminals a task's stages use (1, 2, 5, 6 and up), so the bench never types into
/// an agent.
pub const FIRST_TERMINAL: u16 = 901;
/// What Tools > Bench opens, and what `--gate2` opens without `--terminals`.
pub const TERMINALS: u16 = 10;
/// The most `--gate2 --terminals` takes: 901 to 960.
pub const MAX_TERMINALS: u16 = 60;
/// A quiet shell for typing after the bench (`--gate2`).
pub const TYPING_TERMINAL: u16 = 990;
/// The IME check's shell (ime/ime_check.rs).
pub const IME_TERMINAL: u16 = 991;
// The daemon names terminals 1 to 999.
const _: () = assert!(
    FIRST_TERMINAL + MAX_TERMINALS - 1 < TYPING_TERMINAL
        && TYPING_TERMINAL < IME_TERMINAL
        && IME_TERMINAL <= 999
);
const COLUMNS: usize = 5;
/// A grid of up to this many panes is three wide, like the app's grid.
const NARROW_COLUMNS: usize = 3;
/// The size a hidden bench shell starts at (columns, rows).
const HIDDEN_SIZE: (u16, u16) = (80, 24);

/// 1 MiB of coloured lines, then a second's rest: about 1 MB/s. POSIX sh, so any shell runs it.
pub const FLOOD: &str = concat!(
    "clear; L=$(printf '\\033[1;32mnatamux\\033[0m \\033[33mbench\\033[0m \\033[36m0123456789\\033[0m ",
    "abcdefghijklmnopqrstuvwxyz \\033[35mABCDEFGHIJKLMNOPQRSTUVWXYZ\\033[0m'); ",
    "while :; do yes \"$L\" | head -c 1048576; sleep 1; done\r",
);

/// Looks like an agent's TUI: every 0.1 s one synchronized frame of 20 coloured rows drawn over
/// the last frame, and every 2 s 200 lines that scroll. POSIX sh and printf only.
pub const AGENT: &str = concat!(
    "clear; i=0; while :; do ",
    "printf '\\033[?2026h\\033[20A'; r=0; ",
    "while [ $r -lt 20 ]; do ",
    "printf '\\033[2K\\033[3%dmagent step %d row %d\\033[0m reading files and editing code\\n' ",
    "$((r % 6 + 1)) $i $r; r=$((r + 1)); done; ",
    "printf '\\033[?2026l'; i=$((i + 1)); ",
    "if [ $((i % 20)) -eq 0 ]; then n=0; ",
    "while [ $n -lt 200 ]; do printf 'log line %d of burst %d\\n' $n $i; n=$((n + 1)); done; fi; ",
    "sleep 0.1; done\r",
);

/// What the bench shells print.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Output {
    Flood,
    Agent,
}

impl Output {
    pub fn parse(text: &str) -> Option<Output> {
        match text {
            "flood" => Some(Output::Flood),
            "agent" => Some(Output::Agent),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Output::Flood => "flood",
            Output::Agent => "agent",
        }
    }

    fn command(self) -> &'static str {
        match self {
            Output::Flood => FLOOD,
            Output::Agent => AGENT,
        }
    }
}

/// How many of `count` shells show. Up to 10 all show, as the bench always did, so old reports
/// compare; more than that show like the app's grid and the rest stay hidden.
pub fn shown_count(count: u16) -> usize {
    if count <= TERMINALS {
        usize::from(count)
    } else {
        MAX_PANES
    }
}

pub struct BenchView {
    source: Arc<Source>,
    task: Task,
    terminals: Vec<(String, Entity<RemoteTerminal>)>,
    /// The first this many terminals show; the rest stay connected and hidden.
    shown: usize,
}

impl BenchView {
    pub fn new(
        source: Arc<Source>,
        task: Task,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::sized(source, task, TERMINALS, Output::Flood, window, cx)
    }

    /// `count` shells printing `output`, for `--gate2 --terminals --output`.
    pub fn sized(
        source: Arc<Source>,
        task: Task,
        count: u16,
        output: Output,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let count = count.clamp(1, MAX_TERMINALS);
        Self::with_terminals(
            source,
            task,
            FIRST_TERMINAL..FIRST_TERMINAL + count,
            Some(output.command()),
            shown_count(count),
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
            1,
            window,
            cx,
        )
    }

    fn with_terminals(
        source: Arc<Source>,
        task: Task,
        numbers: std::ops::Range<u16>,
        typed: Option<&str>,
        shown: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let terminals = numbers
            .enumerate()
            .map(|(index, number)| {
                let name = number.to_string();
                let target = Target {
                    task_id: task.id.clone(),
                    terminal: name.clone(),
                };
                let source = source.clone();
                let terminal = cx.new(|cx| {
                    let mut terminal = RemoteTerminal::new(source, target, window, cx);
                    terminal.set_bench_label();
                    let visible = index < shown;
                    terminal.set_visible(visible, cx);
                    if !visible {
                        // No pane lays it out, and a terminal connects from its layout.
                        terminal.connect_unshown(HIDDEN_SIZE, window, cx);
                    }
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
            shown,
        }
    }

    /// `--gate2`: tells the shells that show whether they are on screen. The others are hidden.
    pub fn set_panes_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        for (_, terminal) in self.terminals.iter().take(self.shown) {
            terminal.update(cx, |terminal, cx| terminal.set_visible(visible, cx));
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
        let columns = if self.shown <= MAX_PANES {
            NARROW_COLUMNS
        } else {
            COLUMNS
        };
        let shown: Vec<_> = self.terminals.iter().take(self.shown).collect();
        let rows = shown.chunks(columns).map(|row| {
            h_flex()
                .flex_1()
                .min_h_0()
                .gap_px()
                .children(row.iter().copied().map(|(name, terminal)| {
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
                                .child(terminal_screen(terminal, false, cx)),
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
    use std::time::Instant;

    use natamux_client::{TerminalEvent, TerminalStatus};

    use super::*;
    use crate::perf::overlay::Perf;

    #[test]
    fn the_flood_is_one_line_of_plain_ascii_ending_in_enter() {
        assert!(FLOOD.is_ascii());
        assert!(FLOOD.ends_with('\r'));
        assert_eq!(FLOOD.matches('\r').count(), 1);
        assert!(!FLOOD.contains('\n'));
    }

    #[test]
    fn the_agent_output_is_one_line_of_synchronized_frames_ending_in_enter() {
        assert!(AGENT.is_ascii());
        assert!(AGENT.starts_with("clear;"));
        assert!(AGENT.ends_with('\r'));
        assert_eq!(AGENT.matches('\r').count(), 1);
        assert!(!AGENT.contains('\n'));
        assert!(AGENT.contains("?2026h") && AGENT.contains("?2026l"));
        assert!(AGENT.contains("[20A") && AGENT.contains("-lt 20"));
        assert!(AGENT.contains("-lt 200"));
        assert!(!AGENT.contains("echo") && AGENT.contains("printf"));
    }

    #[test]
    fn up_to_ten_shells_all_show_and_more_show_six() {
        assert_eq!(shown_count(1), 1);
        assert_eq!(shown_count(10), 10);
        assert_eq!(shown_count(11), MAX_PANES);
        assert_eq!(shown_count(60), MAX_PANES);
    }

    // Review of T-0210: past the grid, the hidden shells never started, so a bench of 30
    // measured 6. The fake source cannot count sessions; an open status means one was opened.
    #[gpui::test]
    fn every_terminal_of_a_bench_past_the_grid_connects(cx: &mut gpui::TestAppContext) {
        crate::app::test_views::init(cx);
        let source = Arc::new(Source::FakeDaemon);
        let task = Task {
            id: "T-0146".into(),
            ..Task::default()
        };
        let count = TERMINALS + 2;
        let (bench, cx) = cx.add_window_view(move |window, cx| {
            BenchView::sized(source, task, count, Output::Agent, window, cx)
        });
        cx.run_until_parked();
        let statuses: Vec<TerminalStatus> = bench.read_with(cx, |bench, cx| {
            bench
                .terminals
                .iter()
                .map(|(_, terminal)| terminal.read(cx).status().clone())
                .collect()
        });
        assert_eq!(statuses.len(), usize::from(count));
        assert!(usize::from(count) > shown_count(count));
        assert!(
            statuses
                .iter()
                .all(|status| *status == TerminalStatus::Open),
            "{statuses:?}"
        );
    }

    // Review 2 of T-0210: the bench's terminals and the panes' terminals of the same shells
    // shared perf names, so a pane's first attach counted as a flash in every `--gate2` run.
    #[gpui::test]
    fn a_pane_of_a_bench_shell_shows_no_flash_on_its_first_attach(cx: &mut gpui::TestAppContext) {
        crate::app::test_views::init(cx);
        cx.update(|cx| cx.global_mut::<Perf>().enabled = true);
        let source = Arc::new(Source::FakeDaemon);
        let pane_source = Arc::clone(&source);
        let task = Task {
            id: "T-0146".into(),
            ..Task::default()
        };
        let (bench, cx) = cx.add_window_view(move |window, cx| {
            BenchView::sized(source, task, 1, Output::Agent, window, cx)
        });
        cx.run_until_parked();
        // The cache's terminal of the same shell: its first event is a snapshot.
        bench.update_in(cx, |_, window, cx| {
            let target = Target {
                task_id: "T-0146".into(),
                terminal: FIRST_TERMINAL.to_string(),
            };
            let pane = cx.new(|cx| RemoteTerminal::new(pane_source, target, window, cx));
            pane.update(cx, |pane, cx| {
                pane.connect_unshown(HIDDEN_SIZE, window, cx);
                let snapshot = vec![TerminalEvent::Reset { cols: 80, rows: 24 }];
                pane.apply(snapshot, window, cx);
            });
        });
        cx.run_until_parked();
        let (skeletons, totals) = cx.update(|_, cx| {
            let stats = &cx.global::<Perf>().stats;
            let skeletons: u32 = stats.skeletons.values().sum();
            (skeletons, stats.output_totals(Instant::now()).1)
        });
        // The bench's terminal drew: its screen came as output.
        assert!(
            totals
                .iter()
                .any(|(name, bytes)| name == "bench:T-0146/901" && *bytes > 0),
            "{totals:?}"
        );
        assert_eq!(skeletons, 0);
    }

    #[test]
    fn output_names_parse() {
        assert_eq!(Output::parse("agent"), Some(Output::Agent));
        assert_eq!(Output::parse("flood"), Some(Output::Flood));
        assert_eq!(Output::parse("loud"), None);
        assert_eq!(Output::Agent.name(), "agent");
    }
}
