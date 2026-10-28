//! The flows of `--bench-flows`, in the order they run. The event flows run with `--bench-tasks`
//! tasks (200), the board with 300 Done tasks more, and the palette with 1,000 tasks.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use gpui::{App, AsyncApp, Keystroke, Window, WindowHandle, px, size};

use super::super::fake_daemon;
use super::super::view_state::Layout;
use super::super::window::Root;
use super::{
    POLL, RUNS, SETTLE, WAIT_FOR_TASKS, any_frame, drawn_when, in_window, note, record, sleep,
    time_runs,
};
use crate::palette::palette_layer::PaletteLayer;
use crate::palette::palette_rows::PaletteMode;
use crate::perf::{draws, overlay, stats};
use crate::workspaces::wrap_up_rules::is_finished_state;

/// The report's "30 agents working", and their events a second in all.
const AGENTS: usize = 30;
const AGENT_EVENTS_PER_SECOND: u32 = 60;
const AGENTS_FOR: Duration = Duration::from_secs(10);
/// The draws of a quiet window are counted this long first, and taken off the agents' draws.
const QUIET_FOR: Duration = Duration::from_secs(2);
const PALETTE_TASKS: usize = 1_000;
const BOARD_DONE: usize = 300;
/// What the palette's query grows to, a key at a time.
const QUERY: &str = "bench";
const PANES: usize = 6;
/// The panes connect and draw their first screen within this.
const PANES_OPEN: Duration = Duration::from_millis(1500);
/// The window steps between two sizes this many times, this often.
const RESIZES: usize = 40;
const RESIZE_EVERY: Duration = Duration::from_millis(50);
const SIZES: [(f32, f32); 2] = [(1280., 800.), (1100., 700.)];

pub(super) async fn all(
    handle: WindowHandle<Root>,
    tasks: usize,
    extra: &mut BTreeMap<String, f64>,
    cx: &mut AsyncApp,
) -> Result<()> {
    wait_for_tasks(handle, cx).await?;
    fake_daemon::shared().grow_tasks(tasks, 0);
    // An inactive window draws at most 30 frames a second (gate 2).
    in_window(handle, cx, |window, _| window.activate_window())?;
    sleep(Duration::from_secs(1), cx).await;
    extra.insert("tasks".into(), tasks as f64);
    note("task switch", task_switch(handle, cx).await);
    note("event to row", event_to_row(handle, cx).await);
    note("event UI time", event_ui(handle, extra, cx).await);
    note("diff click", diff_click(handle, cx).await);
    note("resize", resize(handle, extra, cx).await);
    // The board first: its open columns keep the tasks of the event flows, as a busy day has.
    fake_daemon::shared().grow_tasks(tasks + BOARD_DONE, BOARD_DONE);
    sleep(Duration::from_secs(1), cx).await;
    note("board", board(handle, cx).await);
    fake_daemon::shared().grow_tasks(PALETTE_TASKS, 0);
    sleep(Duration::from_secs(1), cx).await;
    note("palette", palette(handle, cx).await);
    Ok(())
}

async fn wait_for_tasks(handle: WindowHandle<Root>, cx: &mut AsyncApp) -> Result<()> {
    let started = Instant::now();
    while handle.update(cx, |root, _, cx| {
        root.dashboard().read(cx).data.tasks.is_empty()
    })? {
        if started.elapsed() > WAIT_FOR_TASKS {
            return Err(anyhow!("the fake daemon's tasks never came"));
        }
        sleep(POLL, cx).await;
    }
    Ok(())
}

/// Task switch: `dashboard.open` to the end of the first frame drawn once the workspace has the
/// task's change list. "Seen": two tasks opened in turn, both open before; "unseen": a task never
/// open before. The git banner and the details have no state a bench can read, so only the
/// change list counts.
async fn task_switch(handle: WindowHandle<Root>, cx: &mut AsyncApp) -> Result<()> {
    let ids = handle.update(cx, |root, _, cx| {
        let tasks = &root.dashboard().read(cx).data.tasks;
        let open = tasks
            .iter()
            .filter(|task| task.workspace.is_some() && !is_finished_state(task.state));
        open.map(|task| task.id.clone()).collect::<Vec<_>>()
    })?;
    if ids.len() < RUNS + 2 {
        return Err(anyhow!("only {} tasks have a workspace", ids.len()));
    }
    for id in &ids[..2] {
        open(handle, id, cx)?;
        sleep(SETTLE, cx).await;
    }
    let workspace = handle.update(cx, |root, _, _| root.workspace_view().clone())?;
    let flows = [("task_switch_seen", None), ("task_switch_unseen", Some(2))];
    for (flow, first) in flows {
        let ids = ids.clone();
        let workspace = workspace.clone();
        time_runs(
            flow,
            handle,
            cx,
            |index, root, _, cx| {
                // Each run switches from the task open before, so the view reads again.
                let id = first.map_or(index % 2, |first| first + index);
                root.dashboard()
                    .update(cx, |dashboard, cx| dashboard.open(&ids[id], cx));
            },
            |_| {
                let workspace = workspace.clone();
                Box::new(move |_: &mut Window, cx: &mut App| {
                    workspace.read(cx).current_changes().is_some()
                })
            },
        )
        .await?;
    }
    back(handle, cx)
}

/// Event to redrawn row: a task change pushed into the fake's feed, to the end of the first
/// frame drawn once the dashboard has it. The table shows All tasks.
async fn event_to_row(handle: WindowHandle<Root>, cx: &mut AsyncApp) -> Result<()> {
    back(handle, cx)?;
    let (id, dashboard) = handle.update(cx, |root, _, cx| {
        let dashboard = root.dashboard().clone();
        let tasks = &dashboard.read(cx).data.tasks;
        let id = tasks.first().map(|task| task.id.clone());
        (id, dashboard)
    })?;
    let id = id.ok_or_else(|| anyhow!("the list is empty"))?;
    for index in 0..RUNS {
        let title = format!("Bench row change {index}");
        let started = Instant::now();
        if !fake_daemon::shared().touch_task(&id, &title) {
            return Err(anyhow!("the fake has no task {id}"));
        }
        let (dashboard, id) = (dashboard.clone(), id.clone());
        let (_, drawn) = drawn_when(handle, cx, move |_, cx| {
            let task = dashboard.read(cx).data.task(&id).cloned();
            task.is_some_and(|task| task.title == title)
        })
        .await?;
        record("event_to_row", drawn.saturating_duration_since(started), cx);
        sleep(SETTLE, cx).await;
    }
    Ok(())
}

/// UI-thread time per dashboard event: `Dashboard::watch` times each `apply` and the observers
/// it wakes while 30 fake agents send events. Draws per event: the window's draws in that time,
/// less those of the quiet window before, over the events.
async fn event_ui(
    handle: WindowHandle<Root>,
    extra: &mut BTreeMap<String, f64>,
    cx: &mut AsyncApp,
) -> Result<()> {
    back(handle, cx)?;
    sleep(SETTLE, cx).await;
    draws::take();
    sleep(QUIET_FOR, cx).await;
    let quiet = draws::take() as f64 / QUIET_FOR.as_secs_f64();
    let before = event_count(cx);
    stats::time_events(true);
    let fake = fake_daemon::shared();
    let agents = fake.start_agents(AGENTS, AGENT_EVENTS_PER_SECOND);
    sleep(AGENTS_FOR, cx).await;
    drop(agents);
    // The last events still in the feed are timed too.
    sleep(SETTLE, cx).await;
    stats::time_events(false);
    let drawn = draws::take() as f64 - quiet * (AGENTS_FOR + SETTLE).as_secs_f64();
    let events = event_count(cx).saturating_sub(before);
    if events == 0 {
        return Err(anyhow!("no dashboard event came from the fake agents"));
    }
    let per_event = drawn.max(0.) / events as f64;
    extra.insert("event_draws_per_event".into(), per_event);
    Ok(())
}

fn event_count(cx: &mut AsyncApp) -> usize {
    cx.update(|cx| {
        let flows = &cx.global::<overlay::Perf>().stats.flows;
        flows.get(super::EVENT_UI).map_or(0, Vec::len)
    })
}

/// Click to diff: a file picked in the Changes tab, to the end of the next frame drawn. The
/// fake answers the read at once, so that frame has the diff; the diff view has no state a
/// bench can read to check it.
async fn diff_click(handle: WindowHandle<Root>, cx: &mut AsyncApp) -> Result<()> {
    let fake = fake_daemon::shared();
    let ids = handle.update(cx, |root, _, cx| {
        let tasks = &root.dashboard().read(cx).data.tasks;
        tasks.iter().map(|task| task.id.clone()).collect::<Vec<_>>()
    })?;
    let (id, paths) = ids
        .into_iter()
        .find_map(|id| {
            let list = fake.list_changes(&id, false).ok()?;
            let paths: Vec<String> = list.into_iter().map(|entry| entry.path).collect();
            (paths.len() >= 2).then_some((id, paths))
        })
        .ok_or_else(|| anyhow!("no fake task has two changed files"))?;
    handle.update(cx, |root, _, cx| {
        root.dashboard()
            .update(cx, |dashboard, cx| dashboard.open_changes(&id, false, cx));
    })?;
    sleep(Duration::from_secs(1), cx).await;
    let changes = handle.update(cx, |root, _, cx| {
        root.workspace_view().read(cx).changes_view().clone()
    })?;
    time_runs(
        "diff_click",
        handle,
        cx,
        |index, _, window, cx| {
            // Another file each time: a pick of the shown file reads nothing.
            let path = &paths[(index + 1) % paths.len()];
            changes.update(cx, |changes, cx| changes.shot_change(path, 0, window, cx));
        },
        any_frame,
    )
    .await?;
    back(handle, cx)
}

/// Window resize with 6 panes in the Agents grid: the frame times while the window steps
/// between two sizes, from the perf overlay's frame clock.
async fn resize(
    handle: WindowHandle<Root>,
    extra: &mut BTreeMap<String, f64>,
    cx: &mut AsyncApp,
) -> Result<()> {
    let ids: Vec<String> = fake_daemon::shared()
        .tasks_with_terminals()
        .into_iter()
        .take(PANES)
        .collect();
    extra.insert("resize_panes".into(), ids.len() as f64);
    handle.update(cx, |root, _, cx| {
        root.dashboard().update(cx, |dashboard, cx| {
            dashboard.set_agent_panes(Some(ids), cx);
            dashboard.open_agents(cx);
        });
    })?;
    sleep(PANES_OPEN, cx).await;
    overlay_on(true, handle, cx)?;
    for step in 0..RESIZES {
        let (width, height) = SIZES[step % SIZES.len()];
        in_window(handle, cx, |window, _| {
            window.resize(size(px(width), px(height)))
        })?;
        sleep(RESIZE_EVERY, cx).await;
    }
    let summary = cx.update(|cx| {
        let stats = &cx.global::<overlay::Perf>().stats;
        stats.summary(Instant::now(), true)
    });
    overlay_on(false, handle, cx)?;
    let (width, height) = SIZES[0];
    in_window(handle, cx, |window, _| {
        window.resize(size(px(width), px(height)))
    })?;
    if summary.frames == 0 {
        return Err(anyhow!("no frame was drawn while the window resized"));
    }
    let ms = |duration: Duration| duration.as_secs_f64() * 1000.0;
    extra.insert("resize_frame_p50_ms".into(), ms(summary.p50));
    extra.insert("resize_frame_p99_ms".into(), ms(summary.p99));
    let slow = summary.over_one_refresh as f64;
    extra.insert("resize_frames_over_1_refresh".into(), slow);
    back(handle, cx)
}

/// The perf overlay's frame clock, as Tools > Toggle Perf Overlay starts it, with new samples.
fn overlay_on(on: bool, handle: WindowHandle<Root>, cx: &mut AsyncApp) -> Result<()> {
    cx.update(|cx| {
        let perf = cx.global_mut::<overlay::Perf>();
        perf.enabled = on;
        perf.stats.next_part();
    });
    in_window(handle, cx, |window, _| overlay::watch_frames(window))?;
    handle.update(cx, |root, _, cx| root.follow_overlay(cx))
}

/// Palette with 1,000 tasks: the query set a key at a time, to the end of the next frame drawn.
/// The rows follow the query in the same update.
async fn palette(handle: WindowHandle<Root>, cx: &mut AsyncApp) -> Result<()> {
    handle.update(cx, |root, window, cx| {
        PaletteLayer::open(root.dashboard(), PaletteMode::Jump, None, window, cx);
    })?;
    sleep(SETTLE, cx).await;
    time_runs(
        "palette_key",
        handle,
        cx,
        |index, _, window, cx| {
            let typed = &QUERY[..(index + 1).min(QUERY.len())];
            PaletteLayer::set_query(typed, window, cx);
        },
        any_frame,
    )
    .await?;
    let escape = Keystroke::parse("escape").map_err(|error| anyhow!("{error}"))?;
    in_window(handle, cx, |window, cx| {
        window.dispatch_keystroke(escape, cx);
    })
}

/// Board with 300 Done tasks: the board told to draw again, and the main-thread time of the
/// frame that draws it (frame start to the end of its draw).
async fn board(handle: WindowHandle<Root>, cx: &mut AsyncApp) -> Result<()> {
    let board = handle.update(cx, |root, _, cx| {
        root.dashboard().update(cx, |dashboard, cx| {
            dashboard.set_layout(Layout::Board, cx);
            if !dashboard.view().show_finished {
                dashboard.toggle_show_finished(cx);
            }
        });
        root.task_view().read(cx).board().clone()
    })?;
    sleep(Duration::from_secs(1), cx).await;
    for _ in 0..RUNS {
        handle.update(cx, |_, _, cx| board.update(cx, |_, cx| cx.notify()))?;
        let (start, drawn) = drawn_when(handle, cx, |_, _| true).await?;
        record("board_frame", drawn.saturating_duration_since(start), cx);
        sleep(SETTLE, cx).await;
    }
    Ok(())
}

fn open(handle: WindowHandle<Root>, id: &str, cx: &mut AsyncApp) -> Result<()> {
    handle.update(cx, |root, _, cx| {
        root.dashboard()
            .update(cx, |dashboard, cx| dashboard.open(id, cx));
    })
}

fn back(handle: WindowHandle<Root>, cx: &mut AsyncApp) -> Result<()> {
    handle.update(cx, |root, _, cx| {
        root.dashboard()
            .update(cx, |dashboard, cx| dashboard.back(cx));
    })
}
