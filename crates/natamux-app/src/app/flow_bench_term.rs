//! Terminal flood: the Agents grid with 6 panes, one of them printing lines from the fake. Draws
//! per wake: the window's draws while it prints, less those of the quiet grid before, over the
//! output events. Then the frame times while it prints, from the perf overlay's frame clock.

use std::collections::BTreeMap;
use std::time::Duration;

use anyhow::{Result, anyhow};
use gpui::{App, AsyncApp, WindowHandle};

use super::super::sleep;
use super::{PANES, PANES_OPEN, QUIET_FOR, back, overlay_on};
use crate::app::fake_daemon;
use crate::app::window::Root;
use crate::perf::{draws, overlay, stats};
use crate::terminals::agent_panes::pane_candidates;
use crate::terminals::terminal_cache::{TerminalCache, terminal_key};

/// An agent that prints fast, as a build log does.
const LINES_PER_SECOND: u32 = 100;
const FLOOD_FOR: Duration = Duration::from_secs(4);

pub(super) async fn flood(
    handle: WindowHandle<Root>,
    extra: &mut BTreeMap<String, f64>,
    cx: &mut AsyncApp,
) -> Result<()> {
    back(handle, cx)?;
    let (ids, fresh) = handle.update(cx, |root, _, cx| {
        let app: &App = cx;
        let tasks = &root.dashboard().read(app).data.tasks;
        let ids: Vec<String> = pane_candidates(tasks, &[])
            .into_iter()
            .map(|task| task.id.clone())
            .collect();
        // A terminal open before keeps its ended feed: the flood needs one the grid opens now.
        let fresh = ids
            .iter()
            .find(|id| TerminalCache::cached(&terminal_key(id, "1"), app).is_none())
            .cloned();
        (ids, fresh)
    })?;
    let fresh = fresh.ok_or_else(|| anyhow!("every pane's terminal was open before"))?;
    let mut panes: Vec<String> = ids
        .into_iter()
        .filter(|id| *id != fresh)
        .take(PANES - 1)
        .collect();
    panes.push(fresh.clone());
    let flood = fake_daemon::shared().flood(&fresh, "1", LINES_PER_SECOND);
    handle.update(cx, |root, _, cx| {
        root.dashboard().update(cx, |dashboard, cx| {
            dashboard.set_agent_panes(Some(panes), cx);
            dashboard.open_agents(cx);
        });
    })?;
    sleep(PANES_OPEN, cx).await;
    draws::take();
    sleep(QUIET_FOR, cx).await;
    let quiet = draws::take() as f64 / QUIET_FOR.as_secs_f64();
    flood.set_printing(true);
    sleep(FLOOD_FOR, cx).await;
    let drawn = draws::take() as f64 - quiet * FLOOD_FOR.as_secs_f64();
    let wakes = flood.sent();
    if wakes == 0 {
        return Err(anyhow!("the pane of {fresh} never opened its terminal"));
    }
    extra.insert("term_draws_per_wake".into(), drawn.max(0.) / wakes as f64);
    overlay_on(true, handle, cx)?;
    sleep(FLOOD_FOR, cx).await;
    let mut times: Vec<Duration> = cx.update(|cx| {
        let stats = &cx.global::<overlay::Perf>().stats;
        stats.frame_samples().map(|(took, _)| took).collect()
    });
    overlay_on(false, handle, cx)?;
    drop(flood);
    times.sort();
    if times.is_empty() {
        return Err(anyhow!("no frame was drawn while the pane printed"));
    }
    let p95 = stats::percentile(&times, 0.95);
    extra.insert("term_flood_frame_p95_ms".into(), p95.as_secs_f64() * 1000.0);
    back(handle, cx)
}
