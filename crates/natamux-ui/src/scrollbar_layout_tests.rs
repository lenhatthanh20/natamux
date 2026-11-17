#![allow(clippy::unwrap_used, clippy::panic)]

use gpui::{Axis, Bounds, point, px, size};

use super::*;

fn area() -> Bounds<Pixels> {
    Bounds::new(point(px(0.), px(0.)), size(px(200.), px(100.)))
}

fn lanes(horizontal: Lane, vertical: Lane) -> Lanes {
    Lanes {
        horizontal,
        vertical,
    }
}

#[test]
fn the_thumb_covers_the_shown_part_and_is_at_least_25_px() {
    // 100 of 424 px shown is 23.6 px, so the least length: at the top, then at the end.
    assert_eq!(thumb_part(px(100.), px(324.), px(0.)), Some((0., 0.25)));
    assert_eq!(thumb_part(px(100.), px(324.), px(-324.)), Some((0.75, 1.)));
    // Half shown: half the viewport long, half way at half the scroll.
    assert_eq!(thumb_part(px(100.), px(100.), px(-50.)), Some((0.25, 0.75)));
    // An offset past an end counts as that end.
    assert_eq!(thumb_part(px(100.), px(100.), px(40.)), Some((0., 0.5)));
    assert_eq!(thumb_part(px(100.), px(100.), px(-900.)), Some((0.5, 1.)));
}

#[test]
fn there_is_no_thumb_without_a_scroll_or_room_for_it() {
    assert_eq!(thumb_part(px(100.), px(0.), px(0.)), None);
    assert_eq!(thumb_part(px(0.), px(10.), px(0.)), None);
    assert_eq!(thumb_part(px(20.), px(500.), px(0.)), None);
}

#[test]
fn a_bar_lies_in_a_14_px_lane_on_the_far_edge() {
    let viewport = size(px(200.), px(100.));
    let layout = Layout::of(
        area(),
        lanes(Lane::Bare, Lane::Bare),
        point(px(0.), px(324.)),
        viewport,
        point(px(0.), px(0.)),
    );
    // Only the vertical axis scrolls, so one bar, the whole height.
    assert_eq!(layout.pieces.len(), 1);
    let piece = &layout.pieces[0];
    assert_eq!(piece.axis, Axis::Vertical);
    assert_eq!(
        piece.strip,
        Bounds::new(point(px(186.), px(0.)), size(px(14.), px(100.)))
    );
    assert_eq!(
        piece.way,
        Bounds::new(point(px(190.), px(4.)), size(px(6.), px(92.)))
    );
    assert_eq!(
        piece.thumb,
        Bounds::new(point(px(190.), px(4.)), size(px(6.), px(23.)))
    );
    assert_eq!(piece.target(), piece.thumb);
}

#[test]
fn two_bars_stop_a_thumb_short_of_the_corner() {
    let layout = Layout::of(
        area(),
        lanes(Lane::Bare, Lane::Bare),
        point(px(200.), px(100.)),
        size(px(200.), px(100.)),
        point(px(-200.), px(-100.)),
    );
    assert_eq!(layout.pieces.len(), 2);
    let sideways = layout.along(Axis::Horizontal).unwrap();
    // The way is 192 px, less 6 for the corner: a half thumb at the end.
    assert_eq!(sideways.thumb.origin.x, px(4. + 93.));
    assert_eq!(sideways.thumb.size.width, px(93.));
    let down = layout.along(Axis::Vertical).unwrap();
    assert_eq!(down.thumb.origin.y, px(4. + 43.));
    assert_eq!(down.thumb.size.height, px(43.));
}

#[test]
fn a_held_thumb_follows_the_pointer_within_its_way() {
    let piece = Piece {
        axis: Axis::Vertical,
        lane: Lane::Bare,
        strip: Bounds::new(point(px(6.), px(16.)), size(px(14.), px(100.))),
        way: Bounds::new(point(px(10.), px(20.)), size(px(6.), px(92.))),
        thumb: Bounds::new(point(px(10.), px(20.)), size(px(6.), px(23.))),
    };
    let at = |y: f32, grab: f32| piece.offset_at(point(px(12.), px(y)), px(grab), px(324.));
    assert_eq!(at(20., 0.), px(0.));
    // 69 px to move in: the thumb's start at the end of it scrolls to the end.
    assert_eq!(at(89., 0.), px(-324.));
    assert_eq!(at(300., 0.), px(-324.));
    assert_eq!(at(10., 0.), px(0.));
    assert_eq!(at(20. + 34.5 + 11.5, 11.5), px(-162.));
    // A thumb as long as its way cannot move.
    let full = Piece {
        thumb: piece.way,
        ..piece
    };
    assert_eq!(full.offset_at(point(px(12.), px(60.)), px(0.), px(9.)), px(0.));
}

#[test]
fn a_press_takes_the_thumb_or_on_a_track_jumps_to_the_pointer() {
    let max = point(px(0.), px(324.));
    let viewport = size(px(200.), px(100.));
    let bare = Layout::of(
        area(),
        lanes(Lane::Off, Lane::Bare),
        max,
        viewport,
        point(px(0.), px(0.)),
    );
    assert_eq!(
        bare.press(point(px(193.), px(10.)), max),
        Some(Hit::Grab {
            axis: Axis::Vertical,
            grab: px(6.)
        })
    );
    // Beside the thumb a bare lane takes nothing.
    assert_eq!(bare.press(point(px(193.), px(80.)), max), None);
    assert_eq!(bare.thumb_at(point(px(193.), px(10.))), Some(0));
    assert_eq!(bare.thumb_at(point(px(100.), px(10.))), None);

    let tracked = Layout::of(
        area(),
        lanes(Lane::Off, Lane::Tracked),
        max,
        viewport,
        point(px(0.), px(0.)),
    );
    assert_eq!(tracked.pieces[0].target(), tracked.pieces[0].way);
    // The thumb's middle at y 50.5: 35 of 69 px.
    let Some(Hit::Jump { axis, offset }) = tracked.press(point(px(193.), px(50.5)), max) else {
        panic!("a jump");
    };
    assert_eq!(axis, Axis::Vertical);
    assert_eq!(offset, -px(324.) * (35. / 69.));
}

#[test]
fn a_frame_sees_whether_a_thumb_moved() {
    let at = |area: Bounds<Pixels>, y: f32| {
        Layout::of(
            area,
            lanes(Lane::Off, Lane::Bare),
            point(px(0.), px(324.)),
            size(px(200.), px(100.)),
            point(px(0.), px(y)),
        )
    };
    let first = at(area(), 0.);
    // The area moved, the thumb did not move within it.
    let shifted = at(area() + point(px(30.), px(40.)), 0.);
    assert!(first.same_thumbs(&shifted));
    assert!(!first.same_thumbs(&at(area(), -100.)));
    assert!(!first.same_thumbs(&Layout::default()));
}
