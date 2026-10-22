//! The window and the root view: the task list on the left; on the right a terminal, or one of
//! the gate tools of phase 1 (bench, IME check).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use editor::Editor;
use gpui::{
    App, Bounds, Entity, FocusHandle, Focusable, StyleRefinement, TitlebarOptions, WindowBounds,
    WindowOptions, point, px, size,
};
use natamux_proto::tasks::task::Task;
use ui::prelude::*;

use super::options::Options;
use super::shortcuts::{
    CheckIme, CloseTool, SavePerfReport, StartBench, StopBench, TogglePerfOverlay,
};
use super::source::Source;
use crate::bench::bench_view::BenchView;
use crate::ime::ime_check::ImeCheck;
use crate::perf::{overlay, report};
use crate::tasks::task_list::{TaskList, TaskOpened, TasksChanged};
use crate::terminals::terminal_pane::TerminalPane;

const DEFAULT_SIZE: (u32, u32) = (1280, 800);
const LIST_WIDTH: f32 = 340.;
/// How often the perf overlay shows new numbers.
const OVERLAY_REFRESH: Duration = Duration::from_millis(500);

enum Tool {
    None,
    Bench(Entity<BenchView>),
    Ime(Entity<ImeCheck>),
}

pub struct Root {
    source: Option<Arc<Source>>,
    list: Entity<TaskList>,
    pane: Entity<TerminalPane>,
    tool: Tool,
    /// One line at the bottom, such as where a perf report went.
    notice: Option<String>,
}

impl Root {
    fn new(
        source: Result<Arc<Source>, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let events = source
            .as_ref()
            .map(|source| source.watch_tasks())
            .map_err(Clone::clone);
        let fake = matches!(source.as_deref(), Ok(Source::FakeDaemon));
        let list = cx.new(|cx| {
            let mut list = TaskList::new(events, window, cx);
            list.open_first = fake;
            list
        });
        let source = source.ok();
        let pane = cx.new(|cx| TerminalPane::new(source.clone(), cx));
        cx.subscribe_in(&list, window, |root, _, TaskOpened(task), window, cx| {
            root.close_tool(cx);
            root.pane
                .update(cx, |pane, cx| pane.show(task.clone(), window, cx));
        })
        .detach();
        cx.subscribe_in(&list, window, |root, _, TasksChanged(tasks), window, cx| {
            root.pane
                .update(cx, |pane, cx| pane.refresh(tasks, window, cx));
        })
        .detach();
        // Closing the window drops Root before an app quit runs its handlers, so with a bench
        // running the window quits the app instead: the quit handler below then still finds Root.
        let weak_root = cx.weak_entity();
        window.on_window_should_close(cx, move |_, cx| {
            let bench_runs = weak_root
                .read_with(cx, |root, _| matches!(root.tool, Tool::Bench(_)))
                .unwrap_or(false);
            if bench_runs {
                cx.quit();
            }
            !bench_runs
        });
        // A bench left running would flood the daemon after the app is gone.
        cx.on_app_quit(|root, cx| {
            let stop = root.take_bench_stop(cx);
            async move {
                if let Some(stop) = stop {
                    stop.await;
                }
            }
        })
        .detach();
        // Follow the system's light or dark look, as Zed's workspace does.
        cx.observe_window_appearance(window, |_, window, cx| {
            *theme::SystemAppearance::global_mut(cx) =
                theme::SystemAppearance(window.appearance().into());
            theme_settings::reload_theme(cx);
        })
        .detach();
        cx.spawn(async move |root, cx| {
            loop {
                cx.background_executor().timer(OVERLAY_REFRESH).await;
                let refreshed = root.update(cx, |_, cx| {
                    if overlay::enabled(cx) {
                        cx.notify();
                    }
                });
                if refreshed.is_err() {
                    break;
                }
            }
        })
        .detach();
        if overlay::enabled(cx) {
            overlay::watch_frames(window);
        }
        Root {
            source,
            list,
            pane,
            tool: Tool::None,
            notice: None,
        }
    }

    /// The text box of the task list (the smoke test types into it).
    pub fn filter(&self, cx: &App) -> Entity<Editor> {
        self.list.read(cx).filter().clone()
    }

    fn close_tool(&mut self, cx: &mut Context<Self>) {
        if let Some(stop) = self.take_bench_stop(cx) {
            cx.spawn(async move |root, cx| {
                let failed = stop.await;
                if failed.is_empty() {
                    return;
                }
                root.update(cx, |root, cx| {
                    root.notice = Some(format!(
                        "Could not end every bench shell: {}",
                        failed.join("; ")
                    ));
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
        self.set_tool(Tool::None, cx);
    }

    /// Ends a running bench: the future ends its shells and gives the ones that failed.
    pub(super) fn take_bench_stop(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<impl Future<Output = Vec<String>> + use<>> {
        let Tool::Bench(bench) = &self.tool else {
            return None;
        };
        let bench = bench.clone();
        self.set_tool(Tool::None, cx);
        Some(bench.update(cx, |bench, cx| bench.stop(cx)))
    }

    /// A tool covers the terminal pane; with none, the pane is back on screen.
    fn set_tool(&mut self, tool: Tool, cx: &mut Context<Self>) {
        let visible = matches!(tool, Tool::None);
        self.tool = tool;
        self.pane
            .update(cx, |pane, cx| pane.set_visible(visible, cx));
        cx.notify();
    }

    fn toggle_perf(&mut self, _: &TogglePerfOverlay, window: &mut Window, cx: &mut Context<Self>) {
        overlay::toggle(window, cx);
        cx.notify();
    }

    fn save_perf(&mut self, _: &SavePerfReport, _: &mut Window, cx: &mut Context<Self>) {
        self.save_report(cx).ok();
    }

    /// Saves the perf report and says where it went.
    pub(super) fn save_report(&mut self, cx: &mut Context<Self>) -> Result<PathBuf, String> {
        let stats = &cx.global::<overlay::Perf>().stats;
        let saved = report::save(stats).map_err(|error| error.to_string());
        self.notice = Some(match &saved {
            Ok(path) => format!("Saved the perf report to {}", path.display()),
            Err(error) => format!("Could not save the perf report: {error}"),
        });
        cx.notify();
        saved
    }

    fn start_bench(&mut self, _: &StartBench, window: &mut Window, cx: &mut Context<Self>) {
        let task = self.pane.read(cx).task().cloned();
        self.open_bench(task, BenchView::new, window, cx);
    }

    /// The bench, or another view of the task's shells, without its Terminal 1 (`--gate2`).
    pub(super) fn open_bench(
        &mut self,
        task: Option<Task>,
        view: impl FnOnce(Arc<Source>, Task, &mut Window, &mut Context<BenchView>) -> BenchView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<BenchView>> {
        let (Some(source), Some(task)) = (self.source.clone(), task) else {
            self.notice =
                Some("Pick a running task first: the bench opens 10 of its shells.".into());
            cx.notify();
            return None;
        };
        if !task.has_live_terminal() {
            self.notice = Some(format!("{} has no running shells.", task.id));
            cx.notify();
            return None;
        }
        self.close_tool(cx);
        if !overlay::enabled(cx) {
            overlay::toggle(window, cx);
        }
        let bench = cx.new(|cx| view(source, task, window, cx));
        self.set_tool(Tool::Bench(bench.clone()), cx);
        Some(bench)
    }

    /// A task of the daemon's list, once the list came.
    pub(super) fn task(&self, id: &str, cx: &App) -> Option<Task> {
        self.list.read(cx).task(id).cloned()
    }

    fn stop_bench(&mut self, _: &StopBench, _: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.tool, Tool::Bench(_)) {
            self.close_tool(cx);
        }
    }

    fn check_ime(&mut self, _: &CheckIme, window: &mut Window, cx: &mut Context<Self>) {
        self.close_tool(cx);
        let task = self.pane.read(cx).task().cloned();
        let source = self.source.clone();
        let check = cx.new(|cx| ImeCheck::new(source, task, window, cx));
        let focus = check.focus_handle(cx);
        window.focus(&focus, cx);
        self.set_tool(Tool::Ime(check), cx);
    }

    fn close(&mut self, _: &CloseTool, _: &mut Window, cx: &mut Context<Self>) {
        self.close_tool(cx);
    }
}

impl Focusable for Root {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.list.focus_handle(cx)
    }
}

impl Render for Root {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let font = theme_settings::setup_ui_font(window, cx);
        let colors = cx.theme().colors();
        let right = match &self.tool {
            Tool::None => self.pane.clone().into_any_element(),
            Tool::Bench(bench) => bench.clone().into_any_element(),
            Tool::Ime(check) => check.clone().into_any_element(),
        };
        let notice = self.notice.clone().map(|notice| {
            div()
                .absolute()
                .bottom_2()
                .left_2()
                .px_2()
                .py_1()
                .rounded_md()
                .bg(colors.elevated_surface_background)
                .border_1()
                .border_color(colors.border)
                .child(Label::new(notice).size(LabelSize::Small))
        });
        h_flex()
            .relative()
            .size_full()
            .font(font)
            .bg(colors.background)
            .text_color(colors.text)
            .on_action(cx.listener(Self::toggle_perf))
            .on_action(cx.listener(Self::save_perf))
            .on_action(cx.listener(Self::start_bench))
            .on_action(cx.listener(Self::stop_bench))
            .on_action(cx.listener(Self::check_ime))
            .on_action(cx.listener(Self::close))
            .child(
                div()
                    .w(px(LIST_WIDTH))
                    .h_full()
                    .flex_none()
                    .border_r_1()
                    .border_color(colors.border)
                    // Drawn again only when the list changes: it was 40% of each frame's draw
                    // while terminals printed (gate 2 review, 2026-10-22).
                    .child(
                        self.list
                            .clone()
                            .cached(StyleRefinement::default().size_full()),
                    ),
            )
            .child(div().flex_grow_1().min_w_0().h_full().child(right))
            .children(overlay::render(cx))
            .children(notice)
    }
}

pub fn open(
    options: &Options,
    source: Result<Arc<Source>, String>,
    cx: &mut App,
) -> anyhow::Result<gpui::WindowHandle<Root>> {
    let (width, height) = options.size.unwrap_or(DEFAULT_SIZE);
    let window_size = size(px(width as f32), px(height as f32));
    // A fixed place at the top left when a size is given, so CI can capture that part of the screen.
    let bounds = if options.size.is_some() {
        Bounds::new(point(px(0.), px(0.)), window_size)
    } else {
        Bounds::centered(None, window_size, cx)
    };
    let window = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("Natamux".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        |window, cx| {
            let root = cx.new(|cx| Root::new(source, window, cx));
            let focus = root.read(cx).focus_handle(cx);
            window.focus(&focus, cx);
            root
        },
    )?;
    Ok(window)
}
