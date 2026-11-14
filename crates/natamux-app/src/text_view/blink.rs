//! The caret's blink (plan 6.3.3, 6.3.4): 500 ms on, 500 ms off while the box has the focus,
//! solid for 500 ms after each change. Each box runs its own timer and redraws only itself. A
//! read-only box and every `--shot` keep the caret solid (D14).

use std::time::Duration;

use gpui::{App, Context};

use super::text_input::TextInput;
use crate::app::appearance::Inputs;

const PHASE: Duration = Duration::from_millis(500);

#[derive(Debug)]
pub struct Blink {
    /// The caret shows in this phase.
    pub visible: bool,
    /// The box has the focus, so the caret blinks.
    on: bool,
    /// Goes up at each restart, so an older timer stops.
    epoch: usize,
}

impl Default for Blink {
    fn default() -> Self {
        Self {
            visible: true,
            on: false,
            epoch: 0,
        }
    }
}

impl TextInput {
    fn blinks(&self, cx: &App) -> bool {
        !self.read_only && cx.try_global::<Inputs>().and_then(|inputs| inputs.shot).is_none()
    }

    pub(super) fn start_blink(&mut self, cx: &mut Context<Self>) {
        self.blink.on = true;
        self.pause_blink(cx);
    }

    pub(super) fn stop_blink(&mut self) {
        self.blink.on = false;
        self.blink.visible = true;
        self.blink.epoch += 1;
    }

    /// Shows the caret now; it blinks again one phase later.
    pub(super) fn pause_blink(&mut self, cx: &mut Context<Self>) {
        self.blink.visible = true;
        self.blink.epoch += 1;
        if self.blink.on && self.blinks(cx) {
            self.next_blink(cx);
        }
    }

    fn next_blink(&mut self, cx: &mut Context<Self>) {
        let epoch = self.blink.epoch;
        cx.spawn(async move |input, cx| {
            cx.background_executor().timer(PHASE).await;
            input
                .update(cx, |input, cx| {
                    if input.blink.epoch == epoch && input.blink.on {
                        input.blink.visible = !input.blink.visible;
                        cx.notify();
                        input.next_blink(cx);
                    }
                })
                .ok();
        })
        .detach();
    }

    /// The caret shows while the box has the focus, in the shown phase or when it does not blink.
    pub(super) fn caret_shows(&self, focused: bool, cx: &App) -> bool {
        focused && (!self.blinks(cx) || self.blink.visible)
    }
}
