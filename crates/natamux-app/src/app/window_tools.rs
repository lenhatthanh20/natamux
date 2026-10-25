//! The phase 1 tools of the Tools menu: the perf overlay and report, the bench and the IME
//! check. A tool covers main until it closes.

use std::path::PathBuf;
use std::sync::Arc;

use gpui::{Entity, Focusable};
use natamux_proto::tasks::task::Task;
use ui::prelude::*;

use super::{Root, Tool};
use crate::app::shortcuts::{
    CheckIme, CloseTool, SavePerfReport, StartBench, StopBench, TogglePerfOverlay,
};
use crate::app::source::Source;
use crate::bench::bench_view::BenchView;
use crate::components::toast_rules::ToastTone;
use crate::components::toasts::toast;
use crate::ime::ime_check::ImeCheck;
use crate::perf::{overlay, report};

impl Root {
    pub(super) fn close_tool(&mut self, cx: &mut Context<Self>) {
        if let Some(stop) = self.take_bench_stop(cx) {
            cx.spawn(async move |root, cx| {
                let failed = stop.await;
                if failed.is_empty() {
                    return;
                }
                root.update(cx, |_, cx| {
                    let text = format!("Could not end every bench shell: {}", failed.join("; "));
                    toast(&text, ToastTone::Error, cx);
                })
                .ok();
            })
            .detach();
        }
        self.set_tool(Tool::None, cx);
    }

    /// `--gate2`: main shows All tasks while the bench's shells keep printing.
    pub(in crate::app) fn park_bench(&mut self, cx: &mut Context<Self>) {
        if let Tool::Bench(bench) = std::mem::replace(&mut self.tool, Tool::None) {
            self.parked_bench = Some(bench);
        }
        self.sync_pane(cx);
    }

    /// Ends a running bench, a parked one too: the future ends its shells and gives the ones
    /// that failed.
    pub(in crate::app) fn take_bench_stop(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<impl Future<Output = Vec<String>> + use<>> {
        if let Some(bench) = self.parked_bench.take() {
            self.tool = Tool::Bench(bench);
        }
        let Tool::Bench(bench) = &self.tool else {
            return None;
        };
        let bench = bench.clone();
        self.set_tool(Tool::None, cx);
        Some(bench.update(cx, |bench, cx| bench.stop(cx)))
    }

    /// A tool covers main; with none, main shows the list or the open task again.
    fn set_tool(&mut self, tool: Tool, cx: &mut Context<Self>) {
        self.tool = tool;
        self.sync_pane(cx);
    }

    pub(super) fn toggle_perf(
        &mut self,
        _: &TogglePerfOverlay,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        overlay::toggle(window, cx);
        cx.notify();
    }

    pub(super) fn save_perf(&mut self, _: &SavePerfReport, _: &mut Window, cx: &mut Context<Self>) {
        self.save_report(cx).ok();
    }

    /// Saves the perf report and says where it went.
    pub(in crate::app) fn save_report(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<PathBuf, String> {
        let stats = &cx.global::<overlay::Perf>().stats;
        let saved = report::save(stats).map_err(|error| error.to_string());
        match &saved {
            Ok(path) => {
                let text = format!("Saved the perf report to {}", path.display());
                toast(&text, ToastTone::Success, cx);
            }
            Err(error) => {
                let text = format!("Could not save the perf report: {error}");
                toast(&text, ToastTone::Error, cx);
            }
        }
        saved
    }

    pub(super) fn start_bench(
        &mut self,
        _: &StartBench,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let task = self.current_task(cx);
        self.open_bench(task, BenchView::new, window, cx);
    }

    /// The bench, or another view of the task's shells, without its Terminal 1 (`--gate2`).
    pub(in crate::app) fn open_bench(
        &mut self,
        task: Option<Task>,
        view: impl FnOnce(Arc<Source>, Task, &mut Window, &mut Context<BenchView>) -> BenchView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<BenchView>> {
        let (Some(source), Some(task)) = (self.source.clone(), task) else {
            let text = "Pick a running task first: the bench opens 10 of its shells.";
            toast(text, ToastTone::Info, cx);
            return None;
        };
        if !task.has_live_terminal() {
            toast(
                &format!("{} has no running shells.", task.id),
                ToastTone::Info,
                cx,
            );
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

    pub(super) fn stop_bench(&mut self, _: &StopBench, _: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.tool, Tool::Bench(_)) {
            self.close_tool(cx);
        }
    }

    pub(super) fn check_ime(&mut self, _: &CheckIme, window: &mut Window, cx: &mut Context<Self>) {
        self.close_tool(cx);
        let task = self.current_task(cx);
        let source = self.source.clone();
        let check = cx.new(|cx| ImeCheck::new(source, task, window, cx));
        let focus = check.focus_handle(cx);
        window.focus(&focus, cx);
        self.set_tool(Tool::Ime(check), cx);
    }

    pub(super) fn close(&mut self, _: &CloseTool, _: &mut Window, cx: &mut Context<Self>) {
        self.close_tool(cx);
    }
}
