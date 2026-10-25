//! The app's one motion clock (docs/ui-spec.md 2.8, the web's `lib/motion-ticker.ts`). A moving
//! item asks for its frame while it renders, which also marks the view that drew it. One timer
//! sleeps until the next tick where a shown kind changes, then notifies only those views. With no
//! moving item, a window without focus, or `--shot`, no timer runs and items draw still forms.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use gpui::{App, AsyncApp, EntityId, Global, Task, Window};

use super::motion_rules::{Moving, TICK_MS, changes, tick_at, ticks_to_next_change};

pub struct Motion {
    /// Tick 0 of the whole app, so every running glyph blinks on the same tick.
    epoch: Instant,
    /// `--shot`: shots stay the same run to run.
    still: bool,
    active: bool,
    /// The views that drew each kind since they were last notified for it.
    watchers: HashMap<Moving, HashSet<EntityId>>,
    _timer: Option<Task<()>>,
    ticking: bool,
}

impl Global for Motion {}

/// The window starts without focus; its first activation starts the motion.
pub fn init(still: bool, cx: &mut App) {
    cx.set_global(Motion {
        epoch: Instant::now(),
        still,
        active: false,
        watchers: HashMap::new(),
        _timer: None,
        ticking: false,
    });
}

fn elapsed_ms(motion: &Motion) -> u64 {
    u64::try_from(motion.epoch.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// The tick to draw `kind` at, or `None` for its still form. Call it while the item renders: the
/// view being drawn is notified on the next tick where `kind` changes.
pub fn frame(kind: Moving, window: &Window, cx: &mut App) -> Option<u8> {
    if !cx.has_global::<Motion>() {
        return None;
    }
    let view = window.current_view();
    let motion = cx.global_mut::<Motion>();
    if motion.still || !motion.active {
        return None;
    }
    motion.watchers.entry(kind).or_default().insert(view);
    let tick = tick_at(elapsed_ms(motion));
    if !motion.ticking {
        motion.ticking = true;
        let timer = cx.spawn(async move |cx| run(cx).await);
        cx.global_mut::<Motion>()._timer = Some(timer);
    }
    Some(tick)
}

/// Follows the focus of the main window. Every view draws again once: still forms without focus,
/// and with focus each moving item marks its view again.
pub fn set_active(active: bool, cx: &mut App) {
    if !cx.has_global::<Motion>() {
        return;
    }
    let motion = cx.global_mut::<Motion>();
    if motion.active == active {
        return;
    }
    motion.active = active;
    motion._timer = None;
    motion.ticking = false;
    motion.watchers.clear();
    cx.refresh_windows();
}

/// The main window has focus. Background work that only keeps the screen fresh waits for it:
/// `set_active` draws every window again when focus comes back.
pub fn window_active(cx: &App) -> bool {
    cx.try_global::<Motion>()
        .is_some_and(|motion| motion.active)
}

/// How long to sleep until the next tick where a shown kind changes; `None` with nothing shown.
fn next_wait(motion: &Motion) -> Option<Duration> {
    let shown: Vec<Moving> = Moving::ALL
        .into_iter()
        .filter(|kind| {
            motion
                .watchers
                .get(kind)
                .is_some_and(|views| !views.is_empty())
        })
        .collect();
    let elapsed = elapsed_ms(motion);
    let ahead = ticks_to_next_change(tick_at(elapsed), &shown)?;
    // 1 ms past the edge, so the wake lands inside the tick it waits for.
    Some(Duration::from_millis(
        u64::from(ahead) * TICK_MS - elapsed % TICK_MS + 1,
    ))
}

/// Notifies the views of every kind that changes on this tick. They render again and mark
/// themselves again; a view that no longer shows the item drops out.
fn notify_due(cx: &mut App) {
    let motion = cx.global_mut::<Motion>();
    let tick = tick_at(elapsed_ms(motion));
    let due: HashSet<EntityId> = Moving::ALL
        .into_iter()
        .filter(|kind| changes(*kind, tick))
        .filter_map(|kind| motion.watchers.remove(&kind))
        .flatten()
        .collect();
    for view in due {
        cx.notify(view);
    }
}

/// Ends when nothing moving is shown; the next `frame` starts it again.
async fn run(cx: &mut AsyncApp) {
    loop {
        let wait = cx.update(|cx| {
            let motion = cx.global_mut::<Motion>();
            let wait = next_wait(motion);
            if wait.is_none() {
                motion.ticking = false;
            }
            wait
        });
        let Some(wait) = wait else { return };
        cx.background_executor().timer(wait).await;
        cx.update(notify_due);
    }
}
