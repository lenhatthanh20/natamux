use super::*;

#[test]
fn the_axes_give_their_bars_a_lane() {
    let lanes = |axes: ScrollAxes| Scrollbars::new(axes).lanes;
    let both = |horizontal, vertical| Lanes {
        horizontal,
        vertical,
    };
    assert_eq!(lanes(ScrollAxes::Horizontal), both(Lane::Bare, Lane::Off));
    assert_eq!(lanes(ScrollAxes::Vertical), both(Lane::Off, Lane::Bare));
    assert_eq!(lanes(ScrollAxes::Both), both(Lane::Bare, Lane::Bare));
    let track = Scrollbars::new(ScrollAxes::Horizontal)
        .with_track_along(ScrollAxes::Horizontal, gpui::black());
    assert_eq!(track.lanes, both(Lane::Tracked, Lane::Off));
    assert_eq!(track.track, Some(gpui::black()));
}

#[test]
fn a_bar_shows_by_its_builder() {
    let axes = ScrollAxes::Both;
    assert_eq!(Scrollbars::new(axes).show, ScrollbarShow::Auto);
    assert_eq!(Scrollbars::always_visible(axes).show, ScrollbarShow::Always);
    let never = Scrollbars::shown(axes, ScrollbarShow::Never);
    assert_eq!(never.show, ScrollbarShow::Never);
    assert!(never.key.is_none() && never.handle.is_none() && !never.redraw_view);
    let keyed = never.id("bar").notify_content();
    assert!(keyed.key.is_some() && keyed.redraw_view);
}
