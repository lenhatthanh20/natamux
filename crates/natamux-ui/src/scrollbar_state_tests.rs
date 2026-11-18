#![allow(clippy::unwrap_used, clippy::panic)]

use std::time::{Duration, Instant};

use gpui::{Axis, px};

use super::*;

fn ms(count: u64) -> Duration {
    Duration::from_millis(count)
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

fn rising(model: &Model) -> bool {
    match model.look {
        Look::Auto(fade) => fade.rising,
        other => panic!("not Auto: {other:?}"),
    }
}

const HERE: Input = Input::Pointer {
    inside: true,
    button: false,
    over: None,
};
const OVER: Input = Input::Pointer {
    inside: true,
    button: false,
    over: Some(Axis::Vertical),
};
const AWAY: Input = Input::Pointer {
    inside: false,
    button: false,
    over: None,
};

/// An `Auto` bar after its first frame and its 50 ms fade in.
fn shown_auto(t0: Instant) -> Model {
    let mut model = Model::new(ScrollbarShow::Auto);
    model.apply(Input::Drawn { moved: true }, t0);
    model.frame(t0, false);
    assert_eq!(model.frame(t0 + ms(60), false).0.alpha, None);
    model
}

#[test]
fn always_and_never_do_not_fade_or_wait() {
    let t0 = Instant::now();
    let mut always = Model::new(ScrollbarShow::Always);
    always.apply(Input::Drawn { moved: true }, t0);
    always.apply(AWAY, t0);
    assert!(!always.waiting);
    let sight = Sight {
        drawn: true,
        alpha: None,
    };
    assert_eq!(always.frame(t0, false), (sight, false));

    let mut never = Model::new(ScrollbarShow::Never);
    assert!(never.off());
    never.apply(Input::Drawn { moved: true }, t0);
    assert!(!never.waiting);
    assert!(!never.frame(t0, false).0.drawn);
}

#[test]
fn an_auto_bar_fades_in_over_50_ms_from_its_first_frame() {
    let t0 = Instant::now();
    let mut model = Model::new(ScrollbarShow::Auto);
    assert!(model.drawn(t0));
    // The first frame starts the fade, wherever the clock was when the bar was made.
    let (first, animate) = model.frame(t0 + ms(500), false);
    assert_eq!(first.alpha, Some(0.));
    assert!(first.drawn && animate);
    let (half, _) = model.frame(t0 + ms(525), false);
    assert!(near(half.alpha.unwrap(), 0.5));
    let (end, animate) = model.frame(t0 + ms(550), false);
    assert_eq!(
        end,
        Sight {
            drawn: true,
            alpha: None
        }
    );
    assert!(!animate);
}

#[test]
fn a_frame_that_moved_a_thumb_starts_the_wait_again() {
    let t0 = Instant::now();
    let mut model = shown_auto(t0);
    assert!(model.waiting);
    let first = model.round;
    model.apply(Input::Drawn { moved: false }, t0);
    assert_eq!(model.round, first);
    model.apply(Input::Drawn { moved: true }, t0);
    assert!(model.waiting);
    assert_ne!(model.round, first);
    // The old wait ending does nothing.
    model.apply(Input::Waited { round: first }, t0 + ms(1000));
    assert!(rising(&model));
}

#[test]
fn the_wait_ends_in_a_400_ms_fade_out() {
    let t0 = Instant::now();
    let mut model = shown_auto(t0);
    let t1 = t0 + ms(1000);
    model.apply(Input::Waited { round: model.round }, t1);
    assert!(!rising(&model));
    assert!(!model.waiting);
    // The first frame of the fade still shows the bar whole.
    let (first, animate) = model.frame(t1 + ms(16), false);
    assert_eq!(first.alpha, Some(1.));
    assert!(animate);
    let (half, _) = model.frame(t1 + ms(216), false);
    assert!(near(half.alpha.unwrap(), 0.5));
    let (gone, animate) = model.frame(t1 + ms(416), false);
    assert!(!gone.drawn && !animate);
    // A hidden bar starts no wait.
    let outside = Input::Release {
        inside: false,
        over: None,
    };
    model.apply(outside, t1 + ms(500));
    assert!(!model.waiting);
}

#[test]
fn a_hovered_or_held_thumb_keeps_the_bar_shown() {
    let t0 = Instant::now();
    let mut model = shown_auto(t0);
    let round = model.round;
    assert!(model.apply(OVER, t0));
    assert_eq!(model.grip, Grip::Over(Axis::Vertical));
    assert!(!model.waiting);
    model.apply(Input::Waited { round }, t0 + ms(1000));
    assert!(rising(&model));
    // Pressed, and let go outside the area: the wait starts, but the held thumb stays shown.
    assert!(model.apply(Input::Grab(Axis::Vertical, px(3.)), t0));
    model.apply(
        Input::Release {
            inside: false,
            over: None,
        },
        t0 + ms(100),
    );
    assert!(model.waiting);
    assert_eq!(model.grip, Grip::Held(Axis::Vertical, px(3.)));
    model.apply(Input::Waited { round: model.round }, t0 + ms(1100));
    assert!(rising(&model));
    // Leaving the area lets go of the thumb and starts the wait.
    model.apply(AWAY, t0 + ms(1200));
    assert_eq!(model.grip, Grip::Free);
    assert!(model.waiting);
    model.apply(Input::Waited { round: model.round }, t0 + ms(2200));
    assert!(!rising(&model));
}

#[test]
fn a_release_in_the_area_hovers_the_thumb_under_it() {
    let t0 = Instant::now();
    let mut model = shown_auto(t0);
    model.apply(Input::Grab(Axis::Vertical, px(3.)), t0);
    let over = Input::Release {
        inside: true,
        over: Some(Axis::Vertical),
    };
    model.apply(over, t0);
    assert_eq!(model.grip, Grip::Over(Axis::Vertical));
    model.apply(Input::Wheel { over: None }, t0);
    assert_eq!(model.grip, Grip::Free);
    assert!(model.waiting);
}

#[test]
fn coming_into_the_area_shows_a_fading_bar_from_where_it_is() {
    let t0 = Instant::now();
    let mut model = shown_auto(t0);
    let t1 = t0 + ms(1000);
    model.apply(AWAY, t0);
    model.apply(Input::Waited { round: model.round }, t1);
    model.frame(t1, false);
    // A quarter faded out: it comes back from three quarters, over the rest of 50 ms.
    let t2 = t1 + ms(100);
    let first_round = model.round;
    model.apply(HERE, t2);
    assert!(rising(&model));
    assert!(model.waiting);
    assert_ne!(model.round, first_round);
    let (back, _) = model.frame(t2, false);
    assert!(near(back.alpha.unwrap(), gpui::ease_in_out(0.75)));
    let (shown, _) = model.frame(t2 + ms(13), false);
    assert_eq!(shown.alpha, None);
    assert!(shown.drawn);
}

#[test]
fn a_move_with_a_button_down_hovers_nothing() {
    let t0 = Instant::now();
    let mut model = shown_auto(t0);
    let held = Input::Pointer {
        inside: true,
        button: true,
        over: Some(Axis::Vertical),
    };
    assert!(!model.apply(held, t0));
    assert_eq!(model.grip, Grip::Free);
}

#[test]
fn reduced_motion_ends_a_fade_at_once() {
    let t0 = Instant::now();
    let mut model = Model::new(ScrollbarShow::Auto);
    let (sight, animate) = model.frame(t0, true);
    assert_eq!(
        sight,
        Sight {
            drawn: true,
            alpha: None
        }
    );
    assert!(!animate);
    model.apply(Input::Drawn { moved: true }, t0);
    model.apply(Input::Waited { round: model.round }, t0 + ms(1000));
    let (sight, animate) = model.frame(t0 + ms(1000), true);
    assert!(!sight.drawn && !animate);
}
