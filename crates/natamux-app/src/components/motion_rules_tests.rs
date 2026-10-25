use super::*;

fn changing(kind: Moving) -> Vec<u8> {
    (0..TICKS).filter(|tick| changes(kind, *tick)).collect()
}

#[test]
fn the_clock_wraps_every_2_seconds() {
    assert_eq!(tick_at(0), 0);
    assert_eq!(tick_at(124), 0);
    assert_eq!(tick_at(125), 1);
    assert_eq!(tick_at(1999), 15);
    assert_eq!(tick_at(2000), 0);
    assert_eq!(tick_at(2000 * 1000 + 1125), 9);
}

#[test]
fn the_beat_blinks_on_ticks_8_and_9_only() {
    let still = Beat {
        opacity: 1.,
        scale: 1.,
    };
    for tick in (0..8).chain(10..16) {
        assert_eq!(beat(Some(tick)), still, "tick {tick}");
    }
    assert_eq!(
        beat(Some(8)),
        Beat {
            opacity: 0.35,
            scale: 0.78
        }
    );
    assert_eq!(
        beat(Some(9)),
        Beat {
            opacity: 0.7,
            scale: 0.9
        }
    );
    assert_eq!(beat(None), still);
}

#[test]
fn the_spinner_steps_one_dash_a_tick() {
    assert_eq!(spin_step(Some(0)), Some(0));
    assert_eq!(spin_step(Some(7)), Some(7));
    assert_eq!(spin_step(Some(8)), Some(0));
    assert_eq!(spin_step(Some(15)), Some(7));
    assert_eq!(spin_step(None), None);
}

#[test]
fn only_the_glyph_spinner_and_skeleton_move() {
    // The line into a running stage is still; only its glyph blinks (owner, T-0164).
    assert_eq!(Moving::ALL, [Moving::Beat, Moving::Spin, Moving::Pulse]);
}

#[test]
fn the_skeleton_pulses_down_to_045_and_back() {
    assert_eq!(pulse(Some(0)), 1.);
    assert!((pulse(Some(8)) - 0.45).abs() < 1e-6);
    assert!((pulse(Some(4)) - pulse(Some(12))).abs() < 1e-6);
    assert!(pulse(Some(4)) < pulse(Some(2)));
    assert!(pulse(Some(4)) > pulse(Some(6)));
    assert_eq!(pulse(None), 1.);
}

#[test]
fn each_kind_redraws_only_on_ticks_that_change() {
    assert_eq!(changing(Moving::Beat), vec![8, 9, 10]);
    assert_eq!(changing(Moving::Spin), (0..16).collect::<Vec<_>>());
    assert_eq!(changing(Moving::Pulse), (0..16).collect::<Vec<_>>());
}

#[test]
fn the_timer_sleeps_until_the_next_change() {
    assert_eq!(ticks_to_next_change(0, &[Moving::Beat]), Some(8));
    assert_eq!(ticks_to_next_change(8, &[Moving::Beat]), Some(1));
    assert_eq!(ticks_to_next_change(10, &[Moving::Beat]), Some(14));
    assert_eq!(
        ticks_to_next_change(3, &[Moving::Beat, Moving::Spin]),
        Some(1)
    );
    assert_eq!(ticks_to_next_change(3, &[]), None);
}
