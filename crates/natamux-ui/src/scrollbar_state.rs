//! What a scroll area's bars are doing, as a plain value that moves on one input at a time: how
//! visible they are, what the pointer does with a thumb, and whether the wait to hide them runs.
//! The element feeds it inputs and acts on what it then reads; time comes in as an argument, so
//! tests can run it without a window.

use std::time::{Duration, Instant};

use gpui::{Axis, Pixels, ease_in_out};

use crate::scrollbar::ScrollbarShow;

const RISE: Duration = Duration::from_millis(50);
const FALL: Duration = Duration::from_millis(400);
/// How long a shown bar waits before it starts to hide.
pub(crate) const HIDE_AFTER: Duration = Duration::from_secs(1);

/// A fade of an `Auto` bar: from `from` (0 hidden, 1 shown) toward shown or hidden, counted from
/// `start`. A fade gets its start from the first frame that draws it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Fade {
    pub(crate) rising: bool,
    from: f32,
    start: Option<Instant>,
}

impl Fade {
    fn new(rising: bool, from: f32) -> Fade {
        Fade {
            rising,
            from,
            start: None,
        }
    }

    /// How visible the bar is at `now`, 0 to 1, before easing.
    pub(crate) fn level(&self, now: Instant) -> f32 {
        let Some(start) = self.start else {
            return self.from;
        };
        let length = if self.rising { RISE } else { FALL };
        let moved = now.saturating_duration_since(start).as_secs_f32() / length.as_secs_f32();
        if self.rising {
            (self.from + moved).min(1.)
        } else {
            (self.from - moved).max(0.)
        }
    }

    fn done(&self, now: Instant) -> bool {
        let level = self.level(now);
        if self.rising {
            level >= 1.
        } else {
            level <= 0.
        }
    }

    /// Heads the other way from where it is now; the same way changes nothing.
    fn head(&mut self, rising: bool, now: Instant) {
        if self.rising != rising {
            *self = Fade::new(rising, self.level(now));
        }
    }
}

/// How the bar shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Look {
    Always,
    /// No bar, no room, no pointer.
    Never,
    Auto(Fade),
}

/// What the pointer does with a thumb.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) enum Grip {
    #[default]
    Free,
    Over(Axis),
    /// Held `grab` px from the thumb's start.
    Held(Axis, Pixels),
}

/// Something that happened to the bar. `over` is the bar whose thumb is under the pointer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Input {
    /// A frame laid the bars out; `moved` when a thumb lies elsewhere than in the last frame.
    Drawn { moved: bool },
    /// The pointer moved with no thumb held.
    Pointer {
        inside: bool,
        button: bool,
        over: Option<Axis>,
    },
    /// The wheel turned with the pointer in the area.
    Wheel { over: Option<Axis> },
    /// A left press took a thumb.
    Grab(Axis, Pixels),
    /// A button came up.
    Release { inside: bool, over: Option<Axis> },
    /// The wait numbered `round` ended.
    Waited { round: u32 },
}

/// What a frame draws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Sight {
    pub(crate) drawn: bool,
    /// The bar's colours times this, while a fade runs.
    pub(crate) alpha: Option<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Model {
    pub(crate) look: Look,
    pub(crate) grip: Grip,
    /// The pointer was in the area at the last move. True at first, so a first move inside is
    /// not an entry.
    inside: bool,
    /// The wait to hide runs, as number `round`.
    pub(crate) waiting: bool,
    pub(crate) round: u32,
}

impl Model {
    pub(crate) fn new(show: ScrollbarShow) -> Model {
        let look = match show {
            ScrollbarShow::Always => Look::Always,
            ScrollbarShow::Never => Look::Never,
            ScrollbarShow::Auto => Look::Auto(Fade::new(true, 0.)),
        };
        Model {
            look,
            grip: Grip::Free,
            inside: true,
            waiting: false,
            round: 0,
        }
    }

    pub(crate) fn off(&self) -> bool {
        self.look == Look::Never
    }

    pub(crate) fn drawn(&self, now: Instant) -> bool {
        match self.look {
            Look::Always => true,
            Look::Never => false,
            Look::Auto(fade) => fade.rising || fade.level(now) > 0.,
        }
    }

    /// Takes one input; true when the event behind it should go no further.
    pub(crate) fn apply(&mut self, input: Input, now: Instant) -> bool {
        match input {
            Input::Drawn { moved } => {
                if moved {
                    self.show(now);
                    self.restart_wait(now);
                }
                false
            }
            Input::Pointer {
                inside,
                button,
                over,
            } => {
                let was_inside = std::mem::replace(&mut self.inside, inside);
                if inside && !button {
                    if !was_inside {
                        self.show(now);
                        self.restart_wait(now);
                    }
                    self.grip_to(over.map_or(Grip::Free, Grip::Over), now);
                    return self.grip != Grip::Free;
                }
                if was_inside && !inside {
                    self.grip_to(Grip::Free, now);
                }
                false
            }
            Input::Wheel { over } => {
                self.grip_to(over.map_or(Grip::Free, Grip::Over), now);
                false
            }
            Input::Grab(axis, grab) => {
                self.grip_to(Grip::Held(axis, grab), now);
                true
            }
            Input::Release { inside, over } => {
                if inside {
                    self.grip_to(over.map_or(Grip::Free, Grip::Over), now);
                } else {
                    self.wait(now);
                }
                false
            }
            Input::Waited { round } => {
                if self.waiting && round == self.round {
                    self.waiting = false;
                    if self.grip == Grip::Free
                        && let Look::Auto(fade) = &mut self.look
                    {
                        fade.head(false, now);
                    }
                }
                false
            }
        }
    }

    /// Where a frame at `now` has the bar. With `still` (reduced motion) a running fade ends at
    /// once. `animate` asks for the next frame.
    pub(crate) fn frame(&mut self, now: Instant, still: bool) -> (Sight, bool) {
        let sight = |drawn| Sight { drawn, alpha: None };
        let Look::Auto(fade) = &mut self.look else {
            return (sight(self.look == Look::Always), false);
        };
        if still && !fade.done(now) {
            *fade = Fade::new(fade.rising, if fade.rising { 1. } else { 0. });
        }
        if fade.done(now) {
            return (sight(fade.rising), false);
        }
        if fade.start.is_none() {
            fade.start = Some(now);
        }
        let alpha = Some(ease_in_out(fade.level(now)));
        (Sight { drawn: true, alpha }, true)
    }

    fn show(&mut self, now: Instant) {
        if let Look::Auto(fade) = &mut self.look {
            fade.head(true, now);
        }
    }

    /// Starts the wait to hide unless one runs: only for a drawn `Auto` bar.
    fn wait(&mut self, now: Instant) {
        if !self.waiting && matches!(self.look, Look::Auto(_)) && self.drawn(now) {
            self.waiting = true;
            self.round = self.round.wrapping_add(1);
        }
    }

    fn restart_wait(&mut self, now: Instant) {
        self.waiting = false;
        self.wait(now);
    }

    /// A thumb taken or hovered shows the bar and stops the wait; let go, the wait starts.
    fn grip_to(&mut self, grip: Grip, now: Instant) {
        if self.grip == grip {
            return;
        }
        if grip == Grip::Free {
            self.wait(now);
        } else {
            self.show(now);
            self.waiting = false;
        }
        self.grip = grip;
    }
}

#[cfg(test)]
#[path = "scrollbar_state_tests.rs"]
mod tests;
