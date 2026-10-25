//! `--gate2 TASK`: gate 2 without a person at the keyboard (owner, 2026-10-22).
//! The bench runs for 60 s while the sidebar tree and a bench terminal scroll, and its perf report
//! is saved. Then main shows the task table while the bench's 10 shells keep printing, and the
//! table scrolls for 20 s: a second report. Then main shows the task's workspace with 4 of the
//! bench's shells as panes, printing, and the first pane scrolls for 20 s: a third report. Last,
//! the bench closes, 100 keys are typed into a quiet shell of the task, and a fourth report is
//! saved. Every path is printed.
//! Keys and scrolls go through the window's own event dispatch, not the keyboard and mouse, so
//! key to glyph starts where it does for real keys: at the app's key event.
//! It never opens the task's Terminal 1, where its agent may work.

use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use gpui::{
    App, AppContext as _, AsyncApp, Keystroke, ListOffset, Modifiers, Pixels, PlatformInput,
    Point, ScrollDelta, ScrollWheelEvent, TouchPhase, Window, WindowHandle, point, px,
};

use super::window::Root;
use crate::bench::bench_view::{BenchView, FIRST_TERMINAL};
use crate::perf::overlay;
use crate::perf::stats::PerfStats;

const WAIT_FOR_TASK: Duration = Duration::from_secs(15);
const BENCH: Duration = Duration::from_secs(60);
const TABLE: Duration = Duration::from_secs(20);
const PANES: Duration = Duration::from_secs(20);
/// The screen-2 target: frames hold with 4 panes of printing shells on screen.
const PANE_COUNT: u16 = 4;
/// The panes connect and draw their history within this.
const PANES_OPEN: Duration = Duration::from_secs(3);
const SCROLL_EVERY: Duration = Duration::from_millis(50);
/// Scroll this many steps one way, then as many back.
const SCROLL_TURN: u32 = 40;
/// Time for the typing shell to show its prompt.
const SHELL_START: Duration = Duration::from_secs(3);
const KEYS: usize = 100;
/// About a fast typist; each echo comes back well within it.
const KEY_GAP: Duration = Duration::from_millis(150);
/// Clears the typed line this often, so the shell never runs it.
const CLEAR_EVERY: usize = 20;
const LETTERS: &[u8] = b"abcdefghijklmnopqrstuvwxyz";

pub fn run(handle: WindowHandle<Root>, task_id: String, cx: &mut App) {
    cx.spawn(async move |cx| match steps(handle, &task_id, cx).await {
        Ok(()) => cx.update(|cx| cx.quit()),
        Err(error) => {
            eprintln!("gate2: {error:#}");
            std::process::exit(1);
        }
    })
    .detach();
}

async fn steps(handle: WindowHandle<Root>, task_id: &str, cx: &mut AsyncApp) -> Result<()> {
    let task = wait_for_task(handle, task_id, cx).await?;

    handle
        .update(cx, |root, window, cx| {
            root.open_bench(Some(task.clone()), BenchView::new, window, cx)
        })?
        .ok_or_else(|| anyhow!("{task_id} has no running shells"))?;
    // An inactive window draws at most 30 frames a second while frame callbacks wait.
    in_window(handle, cx, |window, _| window.activate_window())?;
    cx.background_executor()
        .timer(Duration::from_millis(500))
        .await;
    if !in_window(handle, cx, |window, _| window.is_window_active())? {
        eprintln!("gate2: the window is not the active one; click it to make frames count");
    }
    // Each scroll starts at the top, so runs compare.
    handle.update(cx, |root, _, cx| {
        let tree = root.sidebar().read(cx).tree().clone();
        tree.read(cx).list_state().scroll_to(ListOffset::default());
    })?;
    println!("gate2: bench on {task_id} for {} s", BENCH.as_secs());
    let started = Instant::now();
    let mut step = 0;
    while started.elapsed() < BENCH {
        cx.background_executor().timer(SCROLL_EVERY).await;
        step += 1;
        in_window(handle, cx, |window, cx| scroll(step, window, cx))?;
    }
    save(handle, "bench", cx)?;
    scroll_table(handle, cx).await?;
    scroll_panes(handle, task_id, cx).await?;
    stop_bench(handle, cx).await?;

    println!("gate2: typing {KEYS} keys into Terminal 912 of {task_id}");
    let typing = handle
        .update(cx, |root, window, cx| {
            root.open_bench(Some(task.clone()), BenchView::typing, window, cx)
        })?
        .ok_or_else(|| anyhow!("{task_id} has no running shells"))?;
    cx.background_executor().timer(SHELL_START).await;
    in_window(handle, cx, |window, cx| {
        if let Some(focus) = typing.read(cx).first_focus(cx) {
            window.focus(&focus, cx);
        }
    })?;
    // The shell answers its first key slowly while it sets up its line editor: not counted.
    press(handle, "a", cx)?;
    press(handle, "ctrl-u", cx)?;
    cx.background_executor().timer(SHELL_START).await;
    cx.update(|cx| {
        let perf = cx.global_mut::<overlay::Perf>();
        perf.stats = PerfStats::default();
    });
    for index in 0..KEYS {
        let letter = LETTERS[index % LETTERS.len()] as char;
        press(handle, &letter.to_string(), cx)?;
        cx.background_executor().timer(KEY_GAP).await;
        if (index + 1).is_multiple_of(CLEAR_EVERY) {
            press(handle, "ctrl-u", cx)?;
            cx.background_executor().timer(KEY_GAP).await;
        }
    }

    save(handle, "typing", cx)?;
    stop_bench(handle, cx).await
}

/// Main shows the table while the bench's shells keep printing out of sight: the table scrolls by
/// itself, then its report is saved.
async fn scroll_table(handle: WindowHandle<Root>, cx: &mut AsyncApp) -> Result<()> {
    cx.update(|cx| {
        let perf = cx.global_mut::<overlay::Perf>();
        perf.stats = PerfStats::default();
    });
    handle.update(cx, |root, _, cx| {
        root.park_bench(cx);
        root.task_view().read(cx).table().read(cx).scroll_to(0);
    })?;
    println!("gate2: scrolling the task table for {} s", TABLE.as_secs());
    let started = Instant::now();
    let mut step = 0;
    while started.elapsed() < TABLE {
        cx.background_executor().timer(SCROLL_EVERY).await;
        step += 1;
        in_window(handle, cx, |window, cx| {
            let size = window.viewport_size();
            let table = point(size.width * 0.6, size.height / 2.);
            wheel(table, step, window, cx);
        })?;
    }
    save(handle, "table", cx)
}

/// Main shows the task's workspace with only the first bench shells as panes: they keep printing
/// while the first pane scrolls, then the report is saved and All tasks shows again.
async fn scroll_panes(handle: WindowHandle<Root>, task_id: &str, cx: &mut AsyncApp) -> Result<()> {
    let names: Vec<String> = (FIRST_TERMINAL..FIRST_TERMINAL + PANE_COUNT)
        .map(|name| name.to_string())
        .collect();
    handle.update(cx, |root, _, cx| {
        let panes = root.workspace_view().read(cx).panes().clone();
        panes.update(cx, |panes, cx| panes.show_only(Some(names), cx));
        root.dashboard()
            .update(cx, |dashboard, cx| dashboard.open(task_id, cx));
    })?;
    cx.background_executor().timer(PANES_OPEN).await;
    cx.update(|cx| {
        let perf = cx.global_mut::<overlay::Perf>();
        perf.stats = PerfStats::default();
    });
    println!(
        "gate2: scrolling a grid of {PANE_COUNT} printing panes for {} s",
        PANES.as_secs()
    );
    let started = Instant::now();
    let mut step = 0;
    while started.elapsed() < PANES {
        cx.background_executor().timer(SCROLL_EVERY).await;
        step += 1;
        in_window(handle, cx, |window, cx| {
            // The first pane of the 2 by 2 grid, right of the docked sidebar.
            let size = window.viewport_size();
            let pane = point(size.width * 0.45, size.height * 0.3);
            wheel(pane, step, window, cx);
        })?;
    }
    save(handle, "panes", cx)?;
    handle.update(cx, |root, _, cx| {
        root.dashboard()
            .update(cx, |dashboard, cx| dashboard.back(cx));
        let panes = root.workspace_view().read(cx).panes().clone();
        panes.update(cx, |panes, cx| panes.show_only(None, cx));
    })?;
    Ok(())
}

/// Saves the perf report of one part of the run and prints its path.
fn save(handle: WindowHandle<Root>, part: &str, cx: &mut AsyncApp) -> Result<()> {
    let path = handle.update(cx, |root, _, cx| root.save_report(cx))?;
    let path = path.map_err(|error| anyhow!("could not save the perf report: {error}"))?;
    println!("gate2: {part} report {}", path.display());
    Ok(())
}

/// The daemon's task list comes a moment after the window opens.
async fn wait_for_task(
    handle: WindowHandle<Root>,
    task_id: &str,
    cx: &mut AsyncApp,
) -> Result<natamux_proto::tasks::task::Task> {
    let started = Instant::now();
    loop {
        if let Some(task) = handle.update(cx, |root, _, cx| root.task(task_id, cx))? {
            return Ok(task);
        }
        if started.elapsed() > WAIT_FOR_TASK {
            return Err(anyhow!("the daemon's list has no task {task_id}"));
        }
        cx.background_executor()
            .timer(Duration::from_millis(200))
            .await;
    }
}

/// Ends the shells of the bench view and says which ones the daemon could not end.
async fn stop_bench(handle: WindowHandle<Root>, cx: &mut AsyncApp) -> Result<()> {
    let stop = handle.update(cx, |root, _, cx| root.take_bench_stop(cx))?;
    if let Some(stop) = stop {
        for failed in stop.await {
            eprintln!("gate2: {failed}");
        }
    }
    Ok(())
}

fn press(handle: WindowHandle<Root>, key: &str, cx: &mut AsyncApp) -> Result<()> {
    let keystroke = Keystroke::parse(key).map_err(|error| anyhow!("the key {key:?}: {error}"))?;
    in_window(handle, cx, |window, cx| {
        window.dispatch_keystroke(keystroke, cx);
    })
}

/// Runs `f` with the window but without updating Root: an event can reach Root's own handlers,
/// and Root cannot be updated twice at once (the first --gate2 run, 2026-10-22).
fn in_window<R>(
    handle: WindowHandle<Root>,
    cx: &mut AsyncApp,
    f: impl FnOnce(&mut Window, &mut App) -> R,
) -> Result<R> {
    cx.update_window(handle.into(), |_, window, cx| f(window, cx))
}

/// Scrolls the sidebar tree, then the first bench terminal, down for a while and up again. One
/// target at a time: a pointer that jumps between them on every event changes the cursor each
/// time, which costs AppKit about 5 ms and no person's scrolling does (gate 2 review, 2026-10-22).
fn scroll(step: u32, window: &mut Window, cx: &mut App) {
    let turn = step / SCROLL_TURN;
    let size = window.viewport_size();
    // The docked sidebar is 320 wide at the default 1280; the bench fills main to its right.
    let position = if (turn / 2).is_multiple_of(2) {
        point(px(170.), size.height / 2.)
    } else {
        point(px(460.), px(120.))
    };
    wheel(position, step, window, cx);
}

/// One wheel step at `position`: down for `SCROLL_TURN` steps, then as many up.
fn wheel(position: Point<Pixels>, step: u32, window: &mut Window, cx: &mut App) {
    let down = (step / SCROLL_TURN).is_multiple_of(2);
    let lines = if down { -3. } else { 3. };
    window.dispatch_event(
        PlatformInput::ScrollWheel(ScrollWheelEvent {
            position,
            delta: ScrollDelta::Lines(point(0., lines)),
            modifiers: Modifiers::default(),
            touch_phase: TouchPhase::Moved,
        }),
        cx,
    );
}
