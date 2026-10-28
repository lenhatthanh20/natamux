//! The title bar and the status bar as views of their own, which Root shows cached: a blink of
//! the crumb glyph or a step of the jobs spinner draws that bar only, and a frame of a terminal
//! reuses both bars.

use std::time::Duration;

use gpui::{ClickEvent, Entity, StyleRefinement, Subscription, WeakEntity};
use natamux_proto::tasks::task::TaskState;
use ui::prelude::*;

use super::Root;
use crate::agents::limits_rules::{agents_in_use, current, limits_chip, without_limits};
use crate::app::dashboard::Dashboard;
use crate::app::dashboard_data::{Part, Seen};
use crate::app::shell_rules::{Crumb, crumb, live_item, running_count, window_title};
use crate::app::shortcuts::{ToggleAgents, ToggleDetails, ToggleSidebar};
use crate::app::status_bar::{STATUS_BAR_HEIGHT, StatusBar, now_ms};
use crate::app::status_limits::LimitsItem;
use crate::app::title_bar::{TitleBar, title_bar_height};
use crate::app::update::{self, UpdateChanged};
use crate::app::update_state::UpdateState;
use crate::components::motion;
use crate::tasks::task_attention::{needs_me_count, next_quiet_ms};

/// A running task turns quiet by the clock alone, so the counts are made again this often.
const COUNTS_FRESH_MS: i64 = 1000;

/// The state of the status bar's toggles. Root holds it: it follows the layout of the window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Toggles {
    pub sidebar: bool,
    /// Whether the details show, or why the toggle cannot show them (`details_toggle`).
    pub details: Result<bool, &'static str>,
    pub agents: bool,
}

/// The counts of the status bar and the window title, and when they were made.
#[derive(Clone, Copy, Debug, PartialEq)]
struct BarCounts {
    at_ms: i64,
    needs_you: usize,
    running: usize,
}

/// The counts made at `at_ms` still hold at `now`: no dashboard change came between, and the
/// clock did not move a running task to quiet yet.
fn counts_hold(made_at: Option<i64>, now: i64) -> bool {
    made_at.is_some_and(|at| now - at < COUNTS_FRESH_MS)
}

pub(super) struct TitleBarView {
    dashboard: Entity<Dashboard>,
    #[cfg(test)]
    draws: usize,
    _observe: Subscription,
}

impl TitleBarView {
    pub(super) fn new(dashboard: Entity<Dashboard>, cx: &mut Context<Self>) -> Self {
        // The crumb: the open task, its project, Settings or Agents.
        let parts = &[Part::Tasks, Part::Repos, Part::View];
        let observe = Dashboard::redraw_on(&dashboard, parts, cx);
        Self {
            dashboard,
            #[cfg(test)]
            draws: 0,
            _observe: observe,
        }
    }
}

impl Render for TitleBarView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(test)]
        {
            self.draws += 1;
        }
        let dashboard = self.dashboard.read(cx);
        let crumb = if dashboard.settings.is_some() {
            Crumb::View("Settings")
        } else {
            let open = dashboard
                .open_task
                .as_deref()
                .and_then(|id| dashboard.data.task(id));
            crumb(open, dashboard.agents_open, &dashboard.data.repos)
        };
        TitleBar::new(crumb, self.dashboard.clone())
    }
}

pub(super) struct StatusBarView {
    root: WeakEntity<Root>,
    dashboard: Entity<Dashboard>,
    toggles: Toggles,
    counts: Option<BarCounts>,
    /// The window title last set, "(N) Natamux".
    title: String,
    /// Draws the bar when the next running task turns quiet, while the window has focus.
    clock: Option<gpui::Task<()>>,
    #[cfg(test)]
    draws: usize,
    _observe: Subscription,
    _observe_update: Subscription,
}

impl StatusBarView {
    pub(super) fn new(
        root: WeakEntity<Root>,
        dashboard: Entity<Dashboard>,
        cx: &mut Context<Self>,
    ) -> Self {
        // The counts, the link, the jobs and the plan limits.
        let mut seen = Seen::default();
        let observe = cx.observe(&dashboard, move |bar, dashboard, cx| {
            let parts = [Part::Tasks, Part::Link, Part::Jobs, Part::Limits];
            let gens = dashboard.read(cx).data.gens;
            if seen.changed(&gens, &parts) {
                bar.counts = None;
                bar.clock = None;
                cx.notify();
            }
        });
        let observe_update = cx.observe_global::<UpdateChanged>(|_, cx| cx.notify());
        Self {
            root,
            dashboard,
            toggles: Toggles {
                sidebar: false,
                details: Ok(false),
                agents: false,
            },
            counts: None,
            title: window_title(0),
            clock: None,
            #[cfg(test)]
            draws: 0,
            _observe: observe,
            _observe_update: observe_update,
        }
    }

    /// Root's toggles. The bar draws again only when one of them changed.
    pub(super) fn set_toggles(&mut self, toggles: Toggles, cx: &mut Context<Self>) {
        if self.toggles != toggles {
            self.toggles = toggles;
            cx.notify();
        }
    }

    /// A task turns quiet by the clock alone, so no dashboard change draws the bar then. Without
    /// focus no timer runs; the window draws again when focus comes back.
    fn follow_quiet(&mut self, now: i64, cx: &mut Context<Self>) {
        if !motion::window_active(cx) {
            self.clock = None;
            return;
        }
        if self.clock.is_some() {
            return;
        }
        let Some(wait) = next_quiet_ms(&self.dashboard.read(cx).data.tasks, now) else {
            return;
        };
        self.clock = Some(cx.spawn(async move |bar, cx| {
            let wait = Duration::from_millis(u64::try_from(wait).unwrap_or(0));
            cx.background_executor().timer(wait).await;
            bar.update(cx, |bar, cx| {
                bar.counts = None;
                bar.clock = None;
                cx.notify();
            })
            .ok();
        }));
    }

    fn counts(&mut self, cx: &App) -> BarCounts {
        let now = now_ms();
        match self.counts {
            Some(counts) if counts_hold(Some(counts.at_ms), now) => counts,
            _ => {
                let tasks = &self.dashboard.read(cx).data.tasks;
                let counts = BarCounts {
                    at_ms: now,
                    needs_you: needs_me_count(tasks, now),
                    running: running_count(tasks),
                };
                self.counts = Some(counts);
                counts
            }
        }
    }
}

/// A click of the bar that runs on Root.
fn on_root(
    root: &WeakEntity<Root>,
    run: impl Fn(&mut Root, &mut Window, &mut Context<Root>) + 'static,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    let root = root.clone();
    move |_, window, cx| {
        root.update(cx, |root, cx| run(root, window, cx)).ok();
    }
}

impl Render for StatusBarView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(test)]
        {
            self.draws += 1;
        }
        let counts = self.counts(cx);
        self.follow_quiet(counts.at_ms, cx);
        let title = window_title(counts.needs_you);
        if title != self.title {
            window.set_window_title(&title);
            self.title = title;
        }
        let data = &self.dashboard.read(cx).data;
        let (live, jobs) = (live_item(&data.link), data.jobs.clone());
        let shown = current(&data.limits, counts.at_ms);
        let limits = limits_chip(&shown).map(|chip| LimitsItem {
            chip,
            no_data: without_limits(&agents_in_use(&data.tasks, counts.at_ms), &shown),
            limits: shown.clone(),
        });
        let width = f32::from(window.viewport_size().width);
        let Toggles {
            sidebar,
            details,
            agents,
        } = self.toggles;
        let dashboard = self.dashboard.clone();
        let state = update::state();
        let words = state.status_item(update::bytes());
        let root = self.root.clone();
        StatusBar::new(live, counts.running, counts.needs_you, jobs, width)
            .toggles(sidebar, details, agents)
            .limits(limits)
            .update(words, move |_, window, cx| match &state {
                UpdateState::Ready { .. } => {
                    root.update(cx, |root, cx| root.restart_to_update(window, cx))
                        .ok();
                }
                UpdateState::Manual { page, .. } => cx.open_url(page),
                UpdateState::Failed { .. } => update::check(true, cx),
                _ => {}
            })
            .on_toggles(
                on_root(&self.root, |root, window, cx| {
                    root.toggle_sidebar(&ToggleSidebar, window, cx)
                }),
                on_root(&self.root, |root, window, cx| {
                    root.toggle_details(&ToggleDetails, window, cx)
                }),
                on_root(&self.root, |root, window, cx| {
                    root.toggle_agents(&ToggleAgents, window, cx)
                }),
            )
            .on_running(on_root(&self.root, |root, _, cx| {
                root.show_state(Some(TaskState::Running), cx)
            }))
            .on_needs_you(on_root(&self.root, |root, window, cx| {
                root.show_needs_you(window, cx)
            }))
            .on_open_task(move |id, _, cx| {
                dashboard.update(cx, |dashboard, cx| dashboard.open(id, cx))
            })
    }
}

impl Root {
    /// The two bars, cached: Root draws on every frame of a terminal, the bars only when they
    /// changed.
    pub(super) fn render_bars(
        &mut self,
        sidebar_shown: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (AnyElement, AnyElement) {
        let dashboard = self.dashboard.read(cx);
        let (settings, agents) = (dashboard.settings.is_some(), dashboard.agents_open);
        // Over Settings both toggles show off; a click closes Settings (`close_settings`).
        let details = if settings {
            Ok(false)
        } else {
            self.details_toggle(window, cx)
        };
        let toggles = Toggles {
            sidebar: sidebar_shown && !settings,
            details,
            agents,
        };
        self.status_bar
            .update(cx, |bar, cx| bar.set_toggles(toggles, cx));
        let title_style = StyleRefinement::default()
            .w_full()
            .flex_none()
            .h(title_bar_height(window));
        let status_style = StyleRefinement::default()
            .w_full()
            .flex_none()
            .h(rems_from_px(STATUS_BAR_HEIGHT));
        let title_bar = self.title_bar.clone().cached(title_style);
        let status_bar = self.status_bar.clone().cached(status_style);
        (title_bar.into_any_element(), status_bar.into_any_element())
    }
}

#[cfg(test)]
mod tests {
    use super::{COUNTS_FRESH_MS, counts_hold};

    #[test]
    fn counts_are_made_again_after_a_change_or_once_a_second() {
        assert!(!counts_hold(None, 5_000));
        assert!(counts_hold(Some(5_000), 5_000));
        assert!(counts_hold(Some(5_000), 5_000 + COUNTS_FRESH_MS - 1));
        assert!(!counts_hold(Some(5_000), 5_000 + COUNTS_FRESH_MS));
    }
}

#[cfg(test)]
#[path = "window_bars_tests.rs"]
mod draw_tests;
