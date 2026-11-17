#![allow(clippy::unwrap_used)]

use gpui::{Axis, Bounds, point, px, size};

use super::*;
use crate::scrollbar::ScrollAxes;

#[test]
fn the_thumb_is_the_shown_part_and_at_least_25_px() {
    // 100 of 424 px shown: 23.6 px, so the least length; at the top, then at the end.
    let top = thumb_span(px(324.), px(100.), px(0.)).unwrap();
    assert_eq!(top, 0.0..0.25);
    let end = thumb_span(px(324.), px(100.), px(-324.)).unwrap();
    assert_eq!(end, 0.75..1.0);
    // Half of it shown: the thumb is half the viewport, half way down at half the scroll.
    let half = thumb_span(px(100.), px(100.), px(-50.)).unwrap();
    assert_eq!(half, 0.25..0.75);
    // An offset past either end counts as that end.
    assert_eq!(thumb_span(px(100.), px(100.), px(40.)).unwrap(), 0.0..0.5);
    assert_eq!(thumb_span(px(100.), px(100.), px(-900.)).unwrap(), 0.5..1.0);
}

#[test]
fn no_thumb_without_a_scroll_or_room_for_it() {
    assert_eq!(thumb_span(px(0.), px(100.), px(0.)), None);
    assert_eq!(thumb_span(px(10.), px(0.), px(0.)), None);
    // 25 px do not fit in 20.
    assert_eq!(thumb_span(px(500.), px(20.), px(0.)), None);
}

#[test]
fn a_held_thumb_follows_the_pointer_within_its_way() {
    let way = Bounds::new(point(px(10.), px(20.)), size(px(6.), px(92.)));
    let at = |y: f32, grab: f32| offset_for(way, px(23.), point(px(12.), px(y)), px(grab), px(324.), Axis::Vertical);
    assert_eq!(at(20., 0.), px(0.));
    // 69 px to move in: the thumb's start at the end of it scrolls to the end.
    assert_eq!(at(89., 0.), px(-324.));
    assert_eq!(at(300., 0.), px(-324.));
    assert_eq!(at(10., 0.), px(0.));
    // A press on the track puts the thumb's middle at the pointer.
    assert_eq!(at(20. + 34.5 + 11.5, 11.5), px(-162.));
    // A thumb as long as its way cannot move.
    assert_eq!(offset_for(way, px(92.), point(px(12.), px(60.)), px(0.), px(9.), Axis::Vertical), px(0.));
}

#[test]
fn a_bar_shows_at_once_or_fades_in_by_its_setting() {
    assert_eq!(Seen::first(ScrollbarShow::Always), Seen::Shown);
    assert_eq!(Seen::first(ScrollbarShow::Never), Seen::Off);
    assert_eq!(
        Seen::first(ScrollbarShow::Auto),
        Seen::Fading {
            showing: true,
            done: 0.
        }
    );
    assert!(Seen::Shown.drawn());
    assert!(Seen::first(ScrollbarShow::Auto).drawn());
    assert!(!Seen::Hidden.drawn());
    assert!(!Seen::Off.drawn());
    assert_eq!(Seen::length(true), Duration::from_millis(50));
    assert_eq!(Seen::length(false), Duration::from_millis(400));
}

#[test]
fn a_fade_ends_shown_or_hidden() {
    let fading_in = Seen::Fading {
        showing: true,
        done: 0.,
    };
    assert_eq!(
        fading_in.at(0.4),
        Seen::Fading {
            showing: true,
            done: 0.4
        }
    );
    assert_eq!(fading_in.at(1.), Seen::Shown);
    let fading_out = Seen::Fading {
        showing: false,
        done: 0.,
    };
    assert_eq!(fading_out.at(1.), Seen::Hidden);
    assert_eq!(Seen::Shown.at(1.), Seen::Shown);
    assert_eq!(Seen::Off.at(1.), Seen::Off);
}

#[test]
fn shown_again_a_bar_turns_back_from_where_it_is() {
    let auto = ScrollbarShow::Auto;
    assert_eq!(
        Seen::Hidden.again(auto),
        Seen::Fading {
            showing: true,
            done: 0.
        }
    );
    assert_eq!(Seen::Hidden.again(ScrollbarShow::Always), Seen::Shown);
    let out = Seen::Fading {
        showing: false,
        done: 0.25,
    };
    assert_eq!(
        out.again(auto),
        Seen::Fading {
            showing: true,
            done: 0.75
        }
    );
    let coming = Seen::Fading {
        showing: true,
        done: 0.5,
    };
    assert_eq!(coming.again(auto), coming);
    assert_eq!(Seen::Shown.again(auto), Seen::Shown);
    assert_eq!(Seen::Off.again(auto), Seen::Off);
}

#[test]
fn the_axes_give_rooms_to_their_bars() {
    let rooms = |axes: ScrollAxes| Scrollbars::new(axes).rooms;
    assert_eq!(rooms(ScrollAxes::Horizontal), [Room::Thumb, Room::None]);
    assert_eq!(rooms(ScrollAxes::Vertical), [Room::None, Room::Thumb]);
    assert_eq!(rooms(ScrollAxes::Both), [Room::Thumb, Room::Thumb]);
    let track = Scrollbars::new(ScrollAxes::Horizontal)
        .with_track_along(ScrollAxes::Horizontal, gpui::black());
    assert_eq!(track.rooms, [Room::Track, Room::None]);
    assert_eq!(slot(Axis::Horizontal), 0);
    assert_eq!(slot(Axis::Vertical), 1);
}
