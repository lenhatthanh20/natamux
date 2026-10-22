//! How a panel resize handle changes the width (`ui/ResizeHandle.tsx`, docs/ui-spec.md 4.22). No
//! gpui, so it is tested alone.

pub const KEY_STEP: f32 = 16.0;
pub const KEY_STEP_LARGE: f32 = 64.0;

/// Which edge of its panel the handle sits on. The panel grows when the pointer moves away from
/// the panel: right for a left panel (`Right`), left for a right panel (`Left`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Right,
    // The details panel of screen 3 resizes from its left edge.
    #[cfg_attr(not(test), allow(dead_code))]
    Left,
}

impl Edge {
    fn sign(self) -> f32 {
        match self {
            Edge::Right => 1.0,
            Edge::Left => -1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limits {
    pub min: f32,
    pub max: f32,
    /// Enter and a double click go back to it.
    pub default: f32,
    /// A drag narrower than this collapses the panel (the sidebar: 180).
    pub collapse_below: Option<f32>,
}

impl Limits {
    pub fn clamp(&self, width: f32) -> f32 {
        width.clamp(self.min, self.max).round()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Resize {
    Width(f32),
    /// Fold the panel away; the next opening starts at `Limits::default`.
    Collapse,
}

/// The width after a key on the focused handle: arrows 16 (64 with Shift), Home the minimum, End
/// the maximum, Enter the default. None for other keys.
pub fn by_key(limits: &Limits, edge: Edge, width: f32, key: &str, shift: bool) -> Option<f32> {
    let step = if shift { KEY_STEP_LARGE } else { KEY_STEP };
    let next = match key {
        "right" => width + edge.sign() * step,
        "left" => width - edge.sign() * step,
        "home" => limits.min,
        "end" => limits.max,
        "enter" => limits.default,
        _ => return None,
    };
    Some(limits.clamp(next))
}

/// The width while dragging: the pointer moved `delta_x` from the handle's middle.
pub fn by_drag(limits: &Limits, edge: Edge, width: f32, delta_x: f32) -> Resize {
    let raw = width + edge.sign() * delta_x;
    match limits.collapse_below {
        Some(below) if raw < below => Resize::Collapse,
        _ => Resize::Width(limits.clamp(raw)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIDEBAR: Limits = Limits {
        min: 220.0,
        max: 440.0,
        default: 320.0,
        collapse_below: Some(180.0),
    };

    #[test]
    fn moves_16_px_per_arrow_and_64_with_shift() {
        assert_eq!(
            by_key(&SIDEBAR, Edge::Right, 300.0, "right", false),
            Some(316.0)
        );
        assert_eq!(
            by_key(&SIDEBAR, Edge::Right, 300.0, "left", false),
            Some(284.0)
        );
        assert_eq!(
            by_key(&SIDEBAR, Edge::Right, 300.0, "right", true),
            Some(364.0)
        );
    }

    #[test]
    fn grows_a_right_panel_with_the_left_arrow() {
        assert_eq!(
            by_key(&SIDEBAR, Edge::Left, 300.0, "left", false),
            Some(316.0)
        );
        assert_eq!(
            by_key(&SIDEBAR, Edge::Left, 300.0, "right", true),
            Some(236.0)
        );
    }

    #[test]
    fn home_end_and_enter_go_to_the_minimum_maximum_and_default() {
        assert_eq!(
            by_key(&SIDEBAR, Edge::Right, 300.0, "home", false),
            Some(220.0)
        );
        assert_eq!(
            by_key(&SIDEBAR, Edge::Right, 300.0, "end", false),
            Some(440.0)
        );
        assert_eq!(
            by_key(&SIDEBAR, Edge::Right, 260.0, "enter", false),
            Some(320.0)
        );
        assert_eq!(by_key(&SIDEBAR, Edge::Right, 300.0, "a", false), None);
    }

    #[test]
    fn keys_stay_inside_the_limits() {
        assert_eq!(
            by_key(&SIDEBAR, Edge::Right, 430.0, "right", true),
            Some(440.0)
        );
        assert_eq!(
            by_key(&SIDEBAR, Edge::Right, 230.0, "left", true),
            Some(220.0)
        );
    }

    #[test]
    fn drags_follow_the_pointer_within_the_limits() {
        assert_eq!(
            by_drag(&SIDEBAR, Edge::Right, 300.0, 25.4),
            Resize::Width(325.0)
        );
        assert_eq!(
            by_drag(&SIDEBAR, Edge::Left, 300.0, 25.0),
            Resize::Width(275.0)
        );
        assert_eq!(
            by_drag(&SIDEBAR, Edge::Right, 300.0, 500.0),
            Resize::Width(440.0)
        );
        assert_eq!(
            by_drag(&SIDEBAR, Edge::Right, 300.0, -100.0),
            Resize::Width(220.0)
        );
    }

    #[test]
    fn dragging_below_180_collapses_the_sidebar() {
        assert_eq!(
            by_drag(&SIDEBAR, Edge::Right, 220.0, -41.0),
            Resize::Collapse
        );
        assert_eq!(
            by_drag(&SIDEBAR, Edge::Right, 220.0, -40.0),
            Resize::Width(220.0)
        );
        let fixed = Limits {
            collapse_below: None,
            ..SIDEBAR
        };
        assert_eq!(
            by_drag(&fixed, Edge::Right, 220.0, -200.0),
            Resize::Width(220.0)
        );
    }
}
