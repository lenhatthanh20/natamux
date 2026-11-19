//! The terminal flows, each on the Agents grid with 6 panes.
//!
//! Flood: one pane prints lines from the fake. Draws per wake: the window's draws while it
//! prints, less those of the quiet grid before, over the output events. Then the frame times
//! while it prints, from the perf overlay's frame clock.
//!
//! Key to glyph: letters typed into a pane whose fake terminal echoes them, timed as gate 2 does
//! (`perf/stats.rs`), once in a quiet grid and once while another pane prints.
//!
//! Memory: the app's RSS before 6 panes with a full history open, once they show it, and once
//! they are hidden, per terminal.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use gpui::{App, AsyncApp, Entity, Focusable as _, Keystroke, WindowHandle};
use natamux_proto::terminals::history::scrollback_lines;

use super::super::{SETTLE, in_window, sleep};
use super::{PANES, PANES_OPEN, QUIET_FOR, back, overlay_on};
use crate::app::fake_daemon;
use crate::app::window::Root;
use crate::perf::report::process_usage;
use crate::perf::{draws, overlay, stats};
use crate::terminals::agent_panes::pane_candidates;
use crate::terminals::remote_terminal::RemoteTerminal;
use crate::terminals::terminal_cache::{TerminalCache, terminal_key};

/// An agent that prints fast, as a build log does.
const LINES_PER_SECOND: u32 = 100;
const FLOOD_FOR: Duration = Duration::from_secs(4);
/// Letters typed into the echoing pane, as gate 2 types: a fast typist.
const KEYS: usize = 60;
const KEY_GAP: Duration = Duration::from_millis(150);
const LETTERS: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
/// The memory flow's history lines are this wide, and there are as many as a terminal of this
/// width keeps.
const HISTORY_COLS: u16 = 120;
/// The panes parse a full history within this.
const HISTORY_PARSED: Duration = Duration::from_secs(4);

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

/// Key to glyph and key to echo p95 of `KEYS` letters typed into a pane whose fake terminal
/// echoes them, as `<name>_to_glyph_p95_ms` and `<name>_to_echo_p95_ms`; with `flooding`,
/// another pane prints as in the flood flow.
pub(super) async fn key(
    handle: WindowHandle<Root>,
    name: &str,
    flooding: bool,
    extra: &mut BTreeMap<String, f64>,
    cx: &mut AsyncApp,
) -> Result<()> {
    back(handle, cx)?;
    let (ids, fresh) = candidates(handle, 1 + usize::from(flooding), cx)?;
    let fake = fake_daemon::shared();
    let typed = fresh[0].clone();
    let echo = fake.echo(&typed, "1");
    let mut flood = None;
    if let Some(id) = fresh.get(1) {
        flood = Some(fake.flood(id, "1", LINES_PER_SECOND));
    }
    open_grid(handle, &ids, &fresh, cx)?;
    sleep(PANES_OPEN, cx).await;
    let terminal = opened(&typed, cx)?;
    in_window(handle, cx, |window, cx| {
        let focus = terminal.read(cx).view.focus_handle(cx);
        window.focus(&focus, cx);
    })?;
    if let Some(flood) = &flood {
        flood.set_printing(true);
    }
    overlay_on(true, handle, cx)?;
    sleep(SETTLE, cx).await;
    for index in 0..KEYS {
        let letter = char::from(LETTERS[index % LETTERS.len()]).to_string();
        let keystroke = Keystroke::parse(&letter).map_err(|error| anyhow!("{error}"))?;
        in_window(handle, cx, |window, cx| {
            window.dispatch_keystroke(keystroke, cx);
        })?;
        sleep(KEY_GAP, cx).await;
    }
    let summary = cx.update(|cx| {
        let stats = &cx.global::<overlay::Perf>().stats;
        stats.summary(Instant::now(), true)
    });
    overlay_on(false, handle, cx)?;
    drop((echo, flood));
    let (Some(glyph), Some(echoed)) = (summary.key_to_glyph_p95, summary.key_to_echo_p95) else {
        return Err(anyhow!("no key came back from the pane of {typed}"));
    };
    extra.insert(format!("{name}_to_glyph_p95_ms"), ms(glyph));
    extra.insert(format!("{name}_to_echo_p95_ms"), ms(echoed));
    back(handle, cx)
}

/// The app's RSS growth per terminal, in MB, for 6 panes whose screens come with a full
/// `history`, or empty: once the panes show them (`app_rss_per_terminal_mb`,
/// `app_rss_per_empty_terminal_mb`) and once the terminals are hidden
/// (`app_rss_per_hidden_terminal_mb`, `app_rss_per_hidden_empty_terminal_mb`). The screens are
/// in the fake before the first read, so only the terminals count.
pub(super) async fn memory(
    handle: WindowHandle<Root>,
    history: bool,
    extra: &mut BTreeMap<String, f64>,
    cx: &mut AsyncApp,
) -> Result<()> {
    back(handle, cx)?;
    let (ids, fresh) = candidates(handle, PANES, cx)?;
    let width = usize::from(HISTORY_COLS);
    let line = format!("{:<width$}\r\n", "history line of the memory flow");
    let rows = if history {
        scrollback_lines(HISTORY_COLS)
    } else {
        0
    };
    let screen = line.repeat(rows);
    let fake = fake_daemon::shared();
    for id in &fresh {
        fake.set_screen(id, "1", screen.clone());
    }
    drop(screen);
    let before = rss_mb()?;
    open_grid(handle, &ids, &fresh, cx)?;
    sleep(HISTORY_PARSED, cx).await;
    let open = rss_mb()?;
    let mut terminals = Vec::new();
    for id in &fresh {
        terminals.push(opened(id, cx)?);
    }
    back(handle, cx)?;
    cx.update(|cx| {
        for terminal in &terminals {
            terminal.update(cx, |terminal, cx| terminal.set_visible(false, cx));
        }
    });
    sleep(Duration::from_secs(1), cx).await;
    let hidden = rss_mb()?;
    let per_terminal = |mb: f64| (mb - before) / terminals.len() as f64;
    let kind = if history { "" } else { "empty_" };
    let (shown, parked) = (per_terminal(open), per_terminal(hidden));
    extra.insert(format!("app_rss_per_{kind}terminal_mb"), shown);
    extra.insert(format!("app_rss_per_hidden_{kind}terminal_mb"), parked);
    Ok(())
}

/// The ids of the pane candidates, and the first `count` of them whose Terminal 1 was never
/// opened: a terminal opened before keeps its ended feed, and a flow needs one that opens now.
fn candidates(
    handle: WindowHandle<Root>,
    count: usize,
    cx: &mut AsyncApp,
) -> Result<(Vec<String>, Vec<String>)> {
    let (ids, fresh) = handle.update(cx, |root, _, cx| {
        let app: &App = cx;
        let tasks = &root.dashboard().read(app).data.tasks;
        let ids: Vec<String> = pane_candidates(tasks, &[])
            .into_iter()
            .map(|task| task.id.clone())
            .collect();
        let fresh: Vec<String> = ids
            .iter()
            .filter(|id| TerminalCache::cached(&terminal_key(id, "1"), app).is_none())
            .take(count)
            .cloned()
            .collect();
        (ids, fresh)
    })?;
    if fresh.len() < count {
        return Err(anyhow!("only {} pane terminals were never open", fresh.len()));
    }
    Ok((ids, fresh))
}

/// The Agents grid with `fresh` first, filled up to `PANES` from `ids`.
fn open_grid(
    handle: WindowHandle<Root>,
    ids: &[String],
    fresh: &[String],
    cx: &mut AsyncApp,
) -> Result<()> {
    let mut panes = fresh.to_vec();
    let others = ids.iter().filter(|id| !fresh.contains(id));
    panes.extend(others.take(PANES.saturating_sub(fresh.len())).cloned());
    handle.update(cx, |root, _, cx| {
        root.dashboard().update(cx, |dashboard, cx| {
            dashboard.set_agent_panes(Some(panes), cx);
            dashboard.open_agents(cx);
        });
    })
}

/// The terminal a pane of the grid opened for `id`.
fn opened(id: &str, cx: &mut AsyncApp) -> Result<Entity<RemoteTerminal>> {
    let key = terminal_key(id, "1");
    cx.update(|cx| TerminalCache::cached(&key, cx))
        .ok_or_else(|| anyhow!("the pane of {id} never opened its terminal"))
}

fn rss_mb() -> Result<f64> {
    let usage = process_usage().ok_or_else(|| anyhow!("no memory number from ps"))?;
    Ok(usage.rss_bytes as f64 / 1024.0 / 1024.0)
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}
