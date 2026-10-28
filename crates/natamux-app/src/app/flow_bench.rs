//! `--bench-flows`: times the user flows of T-0221's "Targets" on the fake daemon, 5 runs each,
//! saves one perf report (its `flows` part) and prints one `flow_bench: {...}` line of numbers
//! for `scripts/perf-check.py`. Then it quits. A flow that cannot run says why and the next one
//! runs. How each number is measured: docs/perf.md.
//!
//! Most flows time from the action to the end of the first frame drawn once the change is in
//! the views ("drawn"): the frame callback takes the frame's start, and a task spawned in it
//! runs on the main thread right after that frame's draw.

use std::collections::BTreeMap;
use std::pin::pin;
use std::rc::Rc;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use futures::channel::oneshot;
use futures::future::{Either, select};
use gpui::{App, AppContext as _, AsyncApp, Window, WindowHandle};
use serde_json::{Value, json};

use super::window::Root;
use crate::perf::overlay::Perf;
use crate::perf::report::process_usage;
use crate::perf::stats::{PerfStats, percentile};

/// Runs of each flow.
const RUNS: usize = 5;
/// The fake's feed sends the seed at once; this is only a guard.
const WAIT_FOR_TASKS: Duration = Duration::from_secs(15);
const POLL: Duration = Duration::from_millis(50);
/// A flow whose frame does not come within this is reported and skipped.
const FRAME_WAIT: Duration = Duration::from_secs(5);
/// Frames to look at for the change, at most.
const MAX_FRAMES: u32 = 600;
/// Between two runs, so one run's reads do not overlap the next one.
const SETTLE: Duration = Duration::from_millis(300);

/// The `stats.flows` name of the UI-thread time of one dashboard event (`Dashboard::watch`).
pub const EVENT_UI: &str = "event_ui";

/// Starts the flows once the window has the fake's tasks; `tasks`: the task count of the event
/// flows (`--bench-tasks`).
pub fn run(handle: WindowHandle<Root>, tasks: usize, cx: &mut App) {
    cx.spawn(async move |cx| {
        let mut extra = BTreeMap::new();
        if let Err(error) = steps::all(handle, tasks, &mut extra, cx).await {
            eprintln!("flow_bench: {error:#}");
        }
        finish(handle, &mut extra, cx);
        cx.update(|cx| cx.quit());
    })
    .detach();
}

/// Called by `Dashboard::watch` after each event while `stats::timing_events()` is on.
pub fn event_applied(took: Duration, cx: &mut App) {
    if cx.has_global::<Perf>() {
        cx.global_mut::<Perf>().stats.flow(EVENT_UI, took);
    }
}

/// Saves the report and prints the numbers.
fn finish(handle: WindowHandle<Root>, extra: &mut BTreeMap<String, f64>, cx: &mut AsyncApp) {
    if let Some(usage) = process_usage() {
        let mib = usage.rss_bytes as f64 / 1024.0 / 1024.0;
        extra.insert("app_rss_mb".into(), mib);
    }
    match handle.update(cx, |root, _, cx| root.save_report(cx)) {
        Ok(Ok(path)) => println!("flow_bench: report {}", path.display()),
        Ok(Err(error)) => eprintln!("flow_bench: could not save the perf report: {error}"),
        Err(error) => eprintln!("flow_bench: the window is gone: {error}"),
    }
    let numbers = cx.update(|cx| summary(&cx.global::<Perf>().stats, extra));
    println!("flow_bench: {numbers}");
}

/// One flat object: `<flow>_p50_ms`, `<flow>_p95_ms` and `<flow>_runs` of each flow, then the
/// numbers that are no flow times.
fn summary(stats: &PerfStats, extra: &BTreeMap<String, f64>) -> Value {
    // µs steps: an event takes well under a ms.
    let to_ms = |duration: Duration| (duration.as_secs_f64() * 1e6).round() / 1000.0;
    let mut numbers = serde_json::Map::new();
    for (name, runs) in &stats.flows {
        let mut sorted = runs.clone();
        sorted.sort();
        let (p50, p95) = (percentile(&sorted, 0.5), percentile(&sorted, 0.95));
        numbers.insert(format!("{name}_p50_ms"), json!(to_ms(p50)));
        numbers.insert(format!("{name}_p95_ms"), json!(to_ms(p95)));
        numbers.insert(format!("{name}_runs"), json!(runs.len()));
    }
    for (name, value) in extra {
        numbers.insert(name.clone(), json!((value * 100.0).round() / 100.0));
    }
    Value::Object(numbers)
}

/// Keeps one run of `flow`.
fn record(flow: &str, took: Duration, cx: &mut AsyncApp) {
    cx.update(|cx| cx.global_mut::<Perf>().stats.flow(flow, took));
}

/// A step that failed is told, and the bench goes on.
fn note(flow: &str, result: Result<()>) {
    if let Err(error) = result {
        eprintln!("flow_bench: {flow} skipped: {error:#}");
    }
}

/// Runs `f` with the window, as gate 2 does: not inside an update of Root.
fn in_window<R>(
    handle: WindowHandle<Root>,
    cx: &mut AsyncApp,
    f: impl FnOnce(&mut Window, &mut App) -> R,
) -> Result<R> {
    cx.update_window(handle.into(), |_, window, cx| f(window, cx))
}

async fn sleep(duration: Duration, cx: &mut AsyncApp) {
    cx.background_executor().timer(duration).await;
}

/// A test of one frame, kept across frames.
type Check = Rc<dyn Fn(&mut Window, &mut App) -> bool>;

/// The start of the first frame that begins with `ready` true, and the end of its draw.
async fn drawn_when(
    handle: WindowHandle<Root>,
    cx: &mut AsyncApp,
    ready: impl Fn(&mut Window, &mut App) -> bool + 'static,
) -> Result<(Instant, Instant)> {
    let (sender, receiver) = oneshot::channel();
    let ready: Check = Rc::new(ready);
    in_window(handle, cx, |window, _| {
        window.refresh();
        watch(window, ready, sender, MAX_FRAMES);
    })?;
    let timer = cx.background_executor().timer(FRAME_WAIT);
    match select(pin!(receiver), pin!(timer)).await {
        Either::Left((Ok(times), _)) => Ok(times),
        _ => Err(anyhow!("no frame showed the change")),
    }
}

/// Looks at each frame's start until `ready`; the window draws again meanwhile, so frames keep
/// coming while the change is on its way.
fn watch(
    window: &mut Window,
    ready: Check,
    done: oneshot::Sender<(Instant, Instant)>,
    left: u32,
) {
    window.on_next_frame(move |window, cx| {
        if ready(window, cx) {
            let start = Instant::now();
            // Frame callbacks run before the frame's draw; this task runs after it.
            cx.spawn(async move |_| {
                done.send((start, Instant::now())).ok();
            })
            .detach();
        } else if left > 0 {
            window.refresh();
            watch(window, ready, done, left - 1);
        }
    });
}

/// What a frame must show before it counts.
type Ready = Box<dyn Fn(&mut Window, &mut App) -> bool>;

/// The next frame counts, whatever it shows.
fn any_frame(_: usize) -> Ready {
    Box::new(|_: &mut Window, _: &mut App| true)
}

/// Times `action` to the end of the first frame drawn once `ready(run)` holds, `RUNS` times.
async fn time_runs(
    flow: &str,
    handle: WindowHandle<Root>,
    cx: &mut AsyncApp,
    mut action: impl FnMut(usize, &mut Root, &mut Window, &mut gpui::Context<Root>),
    ready: impl Fn(usize) -> Ready,
) -> Result<()> {
    for index in 0..RUNS {
        let started = Instant::now();
        handle.update(cx, |root, window, cx| action(index, root, window, cx))?;
        let (_, drawn) = drawn_when(handle, cx, ready(index)).await?;
        record(flow, drawn.saturating_duration_since(started), cx);
        sleep(SETTLE, cx).await;
    }
    Ok(())
}

#[path = "flow_bench_steps.rs"]
mod steps;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_summary_is_one_flat_object_of_numbers() {
        let mut stats = PerfStats::default();
        for ms in [10, 50, 20, 40, 30] {
            stats.flow("diff_click", Duration::from_millis(ms));
        }
        let extra = BTreeMap::from([("app_rss_mb".to_string(), 151.456)]);
        let numbers = summary(&stats, &extra);
        assert_eq!(numbers["diff_click_p50_ms"], 30.0);
        assert_eq!(numbers["diff_click_p95_ms"], 50.0);
        assert_eq!(numbers["diff_click_runs"], 5);
        assert_eq!(numbers["app_rss_mb"], 151.46);
        assert_eq!(numbers.as_object().map(serde_json::Map::len), Some(4));
    }
}
