//! The scrollbar of a terminal (docs/plan-mit-license.md 6.2.6): a track of 14 px on the right
//! that the grid always leaves free, a 6 px thumb from the lines of the history, the screen and
//! the scroll offset, and the thumb's colour per state. It shows on a scroll or a new size and
//! hides 1 s later, unless the settings show it always or never.

use std::time::Duration;

use gpui::{Bounds, Hsla, Pixels, point, px, size};
use theme::ThemeColors;

/// The width the track takes: the thumb and its padding on each side.
pub const TRACK_WIDTH: Pixels = px(14.);
const THUMB_WIDTH: Pixels = px(6.);
const PADDING: Pixels = px(4.);
const MIN_THUMB: Pixels = px(25.);
/// The border on the track's inner side, and how much of its colour shows.
pub const BORDER: Pixels = px(1.);
pub const BORDER_ALPHA: f32 = 0.6;
/// When the thumb is not hovered and the track is not kept free, its colour shows at most this
/// much.
const MAX_ALPHA: f32 = 0.7;
/// The time a scroll shows the scrollbar for, then how long it fades out.
pub const SHOWN_FOR: Duration = Duration::from_secs(1);
pub const FADE: Duration = Duration::from_millis(400);

/// When the scrollbar shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Show {
    Never,
    Auto,
    Always,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThumbState {
    Idle,
    Hovered,
    Dragged,
}

/// The thumb in `track` for a screen of `screen_lines` over `total_lines`, scrolled up
/// `display_offset` lines; None when it all fits.
pub fn thumb(
    track: Bounds<Pixels>,
    total_lines: usize,
    screen_lines: usize,
    display_offset: usize,
) -> Option<Bounds<Pixels>> {
    if total_lines <= screen_lines || screen_lines == 0 {
        return None;
    }
    let length = track.size.height - PADDING * 2.;
    let height = (length * (screen_lines as f32 / total_lines as f32)).max(MIN_THUMB);
    let travel = (length - height).max(px(0.));
    let history = (total_lines - screen_lines) as f32;
    let from_top = 1. - (display_offset as f32 / history).min(1.);
    Some(Bounds {
        origin: point(
            track.origin.x + PADDING,
            track.origin.y + PADDING + travel * from_top,
        ),
        size: size(THUMB_WIDTH, height),
    })
}

/// The display offset for the thumb's top at `top` (a drag), within the history.
pub fn offset_at(
    track: Bounds<Pixels>,
    thumb_height: Pixels,
    top: Pixels,
    total_lines: usize,
    screen_lines: usize,
) -> usize {
    let travel = track.size.height - PADDING * 2. - thumb_height;
    if travel <= px(0.) || total_lines <= screen_lines {
        return 0;
    }
    let from_top = ((top - track.origin.y - PADDING) / travel).clamp(0., 1.);
    let history = (total_lines - screen_lines) as f32;
    ((1. - from_top) * history).round() as usize
}

/// The thumb's colour of each state.
#[derive(Clone, Copy, Debug)]
pub struct ThumbColours {
    pub idle: Hsla,
    pub hovered: Hsla,
    pub dragged: Hsla,
}

impl ThumbColours {
    pub fn of(colours: &ThemeColors) -> Self {
        ThumbColours {
            idle: colours.scrollbar_thumb_background,
            hovered: colours.scrollbar_thumb_hover_background,
            dragged: colours.scrollbar_thumb_active_background,
        }
    }
}

/// The thumb's colour: its state's colour over the track's background. Unless the thumb is
/// hovered or the track is kept free, that colour shows at most `MAX_ALPHA`.
pub fn thumb_colour(
    state: ThumbState,
    colours: ThumbColours,
    track_background: Hsla,
    reserved: bool,
) -> Hsla {
    let mut colour = match state {
        ThumbState::Idle => colours.idle,
        ThumbState::Hovered => colours.hovered,
        ThumbState::Dragged => colours.dragged,
    };
    if state != ThumbState::Hovered && !reserved {
        colour.a = colour.a.min(MAX_ALPHA);
    }
    track_background.blend(colour)
}

/// How much of the scrollbar shows `since` the last scroll or new size: all of it for
/// `SHOWN_FOR`, then less over `FADE`.
pub fn auto_alpha(since: Duration) -> f32 {
    match since.checked_sub(SHOWN_FOR) {
        None => 1.,
        Some(fading) => (1. - fading.as_secs_f32() / FADE.as_secs_f32()).max(0.),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{black, size, white};

    fn track() -> Bounds<Pixels> {
        Bounds::new(point(px(100.), px(10.)), size(TRACK_WIDTH, px(208.)))
    }

    #[test]
    fn the_thumb_follows_the_offset_and_keeps_its_least_length() {
        assert_eq!(thumb(track(), 20, 20, 0), None);
        // 200 px to move in: a quarter of the lines shown.
        let bottom = thumb(track(), 80, 20, 0).unwrap();
        assert_eq!(bottom.size, size(px(6.), px(50.)));
        assert_eq!(bottom.origin, point(px(104.), px(164.)));
        let top = thumb(track(), 80, 20, 60).unwrap();
        assert_eq!(top.origin.y, px(14.));
        let long = thumb(track(), 2000, 20, 0).unwrap();
        assert_eq!(long.size.height, px(25.));
        assert_eq!(offset_at(track(), px(50.), px(14.), 80, 20), 60);
        assert_eq!(offset_at(track(), px(50.), px(164.), 80, 20), 0);
    }

    #[test]
    fn the_thumb_colour_follows_its_state() {
        let grey = black().blend(white().opacity(0.5));
        let colours = ThumbColours {
            idle: grey.opacity(0.9),
            hovered: black().opacity(0.9),
            dragged: white().opacity(0.9),
        };
        let track = white();
        let over = |colour: Hsla| track.blend(colour);
        let reserved = |state| thumb_colour(state, colours, track, true);
        assert_eq!(reserved(ThumbState::Idle), over(colours.idle));
        assert_eq!(reserved(ThumbState::Hovered), over(colours.hovered));
        assert_eq!(reserved(ThumbState::Dragged), over(colours.dragged));
        // A track that is not kept free caps the colour, but not under the pointer.
        let capped = thumb_colour(ThumbState::Idle, colours, track, false);
        assert_eq!(
            capped,
            over(Hsla {
                a: 0.7,
                ..colours.idle
            })
        );
        let hovered = thumb_colour(ThumbState::Hovered, colours, track, false);
        assert_eq!(hovered, over(colours.hovered));
        assert_ne!(capped, reserved(ThumbState::Idle));
    }

    #[test]
    fn the_scrollbar_fades_after_a_second() {
        assert_eq!(auto_alpha(Duration::from_millis(900)), 1.);
        assert_eq!(auto_alpha(Duration::from_millis(1200)), 0.5);
        assert_eq!(auto_alpha(Duration::from_secs(3)), 0.);
    }
}
