//! Loading skeletons (docs/ui-spec.md 4.20, `ui/Skeleton.tsx`): bars with fixed widths, in rows
//! as tall as the real ones (table 8 rows of 40, sidebar 5 rows of 44). `Loading` shows them only
//! after 200 ms of loading and then keeps them at least 300 ms (`timing_rules::DelayedFlag`).

use std::time::{Duration, Instant};

use gpui::Task;
use ui::prelude::*;

use super::motion;
use super::motion_rules::{Moving, pulse};
use super::timing_rules::DelayedFlag;

/// Fixed widths, so a skeleton looks the same on every render.
const WIDTHS: [f32; 8] = [0.6, 0.4, 0.75, 0.5, 0.65, 0.45, 0.7, 0.55];

/// Rows of placeholder bars: a title bar, and a meta bar in rows of 40 px or more. The list pulses
/// its opacity once every 2 s (`motion_rules::pulse`).
#[derive(IntoElement)]
pub struct SkeletonList {
    rows: usize,
    row_height: f32,
}

impl SkeletonList {
    pub fn new(rows: usize, row_height: f32) -> Self {
        Self { rows, row_height }
    }
}

fn bar(height: f32, width: f32, cx: &App) -> Div {
    div()
        .h(px(height))
        .w(relative(width))
        .rounded(rems_from_px(4.0_f32))
        .bg(cx.theme().colors().element_background)
}

impl RenderOnce for SkeletonList {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let two_bars = self.row_height >= 40.;
        let opacity = pulse(motion::frame(Moving::Pulse, window, cx));
        v_flex()
            .w_full()
            .opacity(opacity)
            .children((0..self.rows).map(|index| {
                v_flex()
                    .h(px(self.row_height))
                    .justify_center()
                    .gap(rems_from_px(6.0_f32))
                    .px_2()
                    .child(bar(12., WIDTHS[index % WIDTHS.len()], cx))
                    .when(two_bars, |this| {
                        this.child(bar(10., WIDTHS[(index + 3) % WIDTHS.len()], cx))
                    })
            }))
    }
}

struct Gate {
    flag: DelayedFlag,
    epoch: Instant,
    _wake: Option<Task<()>>,
}

/// While `loading`, shows nothing for 200 ms, then the skeleton for at least 300 ms; otherwise
/// the content. Give each place its own `id`, so its timing survives renders.
#[derive(IntoElement)]
pub struct Loading {
    id: ElementId,
    loading: bool,
    skeleton: SkeletonList,
    content: Option<AnyElement>,
}

impl Loading {
    pub fn new(id: impl Into<ElementId>, loading: bool, skeleton: SkeletonList) -> Self {
        Self {
            id: id.into(),
            loading,
            skeleton,
            content: None,
        }
    }

    pub fn content(mut self, content: impl IntoElement) -> Self {
        self.content = Some(content.into_any_element());
        self
    }
}

impl RenderOnce for Loading {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let gate = window.use_keyed_state(self.id, cx, |_, _| Gate {
            flag: DelayedFlag::default(),
            epoch: Instant::now(),
            _wake: None,
        });
        let loading = self.loading;
        let shown = gate.update(cx, |gate, cx| {
            let now = u64::try_from(gate.epoch.elapsed().as_millis()).unwrap_or(u64::MAX);
            let (shown, wait) = gate.flag.poll(loading, now);
            // Notifying the gate redraws the view that renders it, when the answer changes.
            gate._wake = wait.map(|ms| {
                cx.spawn(async move |gate, cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(ms))
                        .await;
                    gate.update(cx, |_, cx| cx.notify()).ok();
                })
            });
            shown
        });
        if shown {
            self.skeleton.into_any_element()
        } else if loading {
            div().into_any_element()
        } else {
            self.content.unwrap_or_else(|| div().into_any_element())
        }
    }
}
