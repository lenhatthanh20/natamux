//! The toast stack (docs/ui-spec.md 4.15, `ui/Toaster.tsx`): bottom right, above the status bar
//! and above a dialog's scrim. The queue and its timing are `toast_rules`; the timer pauses while
//! the pointer is on a toast. Push from anywhere with `toast` or `push_toast`.

use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{Anchor, Entity, Global, anchored, deferred, point};
use ui::prelude::*;

use super::timing_rules::ToastClock;
use super::toast_rules::{Toast, ToastTone, Toasts, toast_duration};

type Run = Rc<dyn Fn(&mut Window, &mut App)>;

/// A link-style button under the toast text ("Open", "Undo"). It runs, then the toast goes.
pub struct ToastAction {
    pub label: SharedString,
    pub run: Run,
}

impl ToastAction {
    // No screen-1 toast has a button yet; Undo and Open come with later screens.
    #[allow(dead_code)]
    pub fn new(
        label: impl Into<SharedString>,
        run: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            label: label.into(),
            run: Rc::new(run),
        }
    }
}

pub struct Toaster {
    queue: Toasts,
    clocks: HashMap<u64, ToastClock>,
    runs: HashMap<u64, Vec<Run>>,
    epoch: Instant,
}

struct GlobalToaster(Entity<Toaster>);

impl Global for GlobalToaster {}

/// Shows a short message with no actions.
pub fn toast(text: &str, tone: ToastTone, cx: &mut App) {
    push_toast(text, tone, None, Vec::new(), cx);
}

/// Shows a message with an optional detail line and actions. Nothing shows before
/// `Toaster::init`.
pub fn push_toast(
    text: &str,
    tone: ToastTone,
    detail: Option<String>,
    actions: Vec<ToastAction>,
    cx: &mut App,
) {
    let Some(toaster) = cx.try_global::<GlobalToaster>().map(|g| g.0.clone()) else {
        return;
    };
    toaster.update(cx, |toaster, cx| {
        toaster.push(text, tone, detail, actions, cx)
    });
}

impl Toaster {
    /// Makes the window's toast stack. The root view renders it as a child.
    pub fn init(cx: &mut App) -> Entity<Self> {
        let toaster = cx.new(|_| Toaster {
            queue: Toasts::default(),
            clocks: HashMap::new(),
            runs: HashMap::new(),
            epoch: Instant::now(),
        });
        cx.set_global(GlobalToaster(toaster.clone()));
        toaster
    }

    pub fn push(
        &mut self,
        text: &str,
        tone: ToastTone,
        detail: Option<String>,
        actions: Vec<ToastAction>,
        cx: &mut Context<Self>,
    ) {
        let (labels, runs): (Vec<String>, Vec<Run>) = actions
            .into_iter()
            .map(|action| (action.label.to_string(), action.run))
            .unzip();
        let has_actions = !labels.is_empty();
        self.queue.push(text, tone, detail, labels);
        // The queue drops old and repeated toasts; forget their timers and actions too.
        let ids: Vec<u64> = self.queue.toasts.iter().map(|t| t.id).collect();
        self.clocks.retain(|id, _| ids.contains(id));
        self.runs.retain(|id, _| ids.contains(id));
        let Some(id) = ids.last().copied() else {
            return;
        };
        let clock = ToastClock::start(toast_duration(tone, has_actions), self.now());
        self.clocks.insert(id, clock);
        self.runs.insert(id, runs);
        self.arm(id, cx);
        cx.notify();
    }

    pub fn dismiss(&mut self, id: u64, cx: &mut Context<Self>) {
        self.queue.dismiss(id);
        self.clocks.remove(&id);
        self.runs.remove(&id);
        cx.notify();
    }

    fn now(&self) -> u64 {
        u64::try_from(self.epoch.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    /// Wakes up when the toast is due. A timer that wakes after a pause finds it not due and
    /// waits again.
    fn arm(&self, id: u64, cx: &mut Context<Self>) {
        let Some(due) = self.clocks.get(&id).and_then(ToastClock::due_at) else {
            return;
        };
        let wait = Duration::from_millis(due.saturating_sub(self.now()));
        cx.spawn(async move |toaster, cx| {
            cx.background_executor().timer(wait).await;
            toaster
                .update(cx, |toaster, cx| {
                    let now = toaster.now();
                    match toaster.clocks.get(&id).map(|clock| clock.is_due(now)) {
                        Some(true) => toaster.dismiss(id, cx),
                        Some(false) => toaster.arm(id, cx),
                        None => {}
                    }
                })
                .ok();
        })
        .detach();
    }

    fn hover(&mut self, id: u64, hovered: bool, cx: &mut Context<Self>) {
        let now = self.now();
        let Some(clock) = self.clocks.get_mut(&id) else {
            return;
        };
        if hovered {
            clock.pause(now);
        } else {
            clock.resume(now);
            self.arm(id, cx);
        }
    }

    fn render_toast(&self, toast: &Toast, cx: &mut Context<Self>) -> impl IntoElement {
        let id = toast.id;
        let (icon, color) = match toast.tone {
            ToastTone::Info => ("icons/lucide/info.svg", Color::Accent),
            ToastTone::Success => ("icons/lucide/circle-check.svg", Color::Success),
            ToastTone::Error => ("icons/lucide/circle-alert.svg", Color::Error),
        };
        let runs = self.runs.get(&id).cloned().unwrap_or_default();
        let actions: Vec<Button> = toast
            .actions
            .iter()
            .zip(runs)
            .enumerate()
            .map(|(index, (label, run))| {
                Button::new(("toast-action", index), label.clone())
                    .style(ButtonStyle::Transparent)
                    .color(Color::Accent)
                    .on_click(cx.listener(move |toaster, _, window, cx| {
                        run(window, cx);
                        toaster.dismiss(id, cx);
                    }))
            })
            .collect();
        h_flex()
            .id(("toast", id))
            .w_full()
            .items_start()
            .gap(px(10.))
            .pl(px(14.))
            .pr(px(12.))
            .py(px(12.))
            .elevation_2(cx)
            .rounded(px(8.))
            .on_hover(cx.listener(move |toaster, hovered: &bool, _, cx| {
                toaster.hover(id, *hovered, cx);
            }))
            .child(
                div()
                    .flex_none()
                    .mt(px(2.))
                    .child(Icon::from_path(icon).size(IconSize::Medium).color(color)),
            )
            .child(
                v_flex()
                    .min_w_0()
                    .flex_1()
                    .child(Label::new(toast.text.clone()))
                    .when_some(toast.detail.clone(), |this, detail| {
                        this.child(
                            Label::new(detail)
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        )
                    })
                    .when(!toast.actions.is_empty(), |this| {
                        this.child(h_flex().mt(px(6.)).gap_3().children(actions))
                    }),
            )
            .child(
                IconButton::new(("toast-dismiss", id), IconName::Close)
                    .icon_size(IconSize::Small)
                    .tooltip(ui::Tooltip::text("Dismiss"))
                    .on_click(cx.listener(move |toaster, _, _, cx| toaster.dismiss(id, cx))),
            )
    }
}

impl Render for Toaster {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.queue.toasts.is_empty() {
            return div().into_any_element();
        }
        let viewport = window.viewport_size();
        let items: Vec<_> = self
            .queue
            .toasts
            .iter()
            .map(|toast| self.render_toast(toast, cx).into_any_element())
            .collect();
        // 16 px from the right edge, 12 px above the 28 px status bar.
        deferred(
            anchored()
                .anchor(Anchor::BottomRight)
                .position(point(viewport.width - px(16.), viewport.height - px(40.)))
                .child(
                    v_flex()
                        .w(px(360.))
                        .max_w(viewport.width - px(32.))
                        .gap_2()
                        .children(items),
                ),
        )
        .with_priority(4)
        .into_any_element()
    }
}
