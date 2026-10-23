//! Where a scroll view goes to show one of its children in the middle (the web's
//! `scrollIntoView({ block: 'center' })`). Pure, so it is tested without a window.

/// The scroll offset (0 at the top, more negative further down) that puts a child of
/// `child_top`..`child_top + child_height` (in content pixels, from the top of the first child)
/// in the middle of a viewport `viewport_height` high. It stays within 0 and `-max_scroll`.
pub fn centred_offset(
    viewport_height: f32,
    child_top: f32,
    child_height: f32,
    max_scroll: f32,
) -> f32 {
    let wanted = child_top + child_height / 2. - viewport_height / 2.;
    -wanted.clamp(0., max_scroll.max(0.))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn puts_the_child_in_the_middle_and_stays_inside_the_content() {
        // A 100 px child at 500 in a 400 px view: its middle 550 goes to 200.
        assert_eq!(centred_offset(400., 500., 100., 1000.), -350.);
        // Near the top it cannot scroll above the content.
        assert_eq!(centred_offset(400., 20., 100., 1000.), 0.);
        // Near the end it stops at the last scroll.
        assert_eq!(centred_offset(400., 1300., 100., 1000.), -1000.);
        assert_eq!(centred_offset(400., 500., 100., -5.), 0.);
    }
}
