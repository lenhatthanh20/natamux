//! The frames of the app's endless motion (docs/ui-spec.md 2.8, the web's `styles.css`): one clock
//! of 125 ms ticks, 16 to a loop of 2 s. A `None` tick is the still form (window without focus,
//! `--shot`). No gpui, so it is tested alone.

pub const TICK_MS: u64 = 125;
pub const TICKS: u8 = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Moving {
    /// A running state glyph (`.hm-beat`).
    Beat,
    /// The busy spinner (`.hm-spin8`).
    Spin,
    /// The skeleton list (`.hm-pulse`).
    Pulse,
}

impl Moving {
    pub const ALL: [Moving; 3] = [Moving::Beat, Moving::Spin, Moving::Pulse];
}

pub fn tick_at(elapsed_ms: u64) -> u8 {
    u8::try_from(elapsed_ms / TICK_MS % u64::from(TICKS)).unwrap_or(0)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Beat {
    pub opacity: f32,
    pub scale: f32,
}

/// Still on 14 of 16 ticks; every running glyph blinks on the same ones.
pub fn beat(tick: Option<u8>) -> Beat {
    let (opacity, scale) = match tick.map(|tick| tick % TICKS) {
        Some(8) => (0.35, 0.78),
        Some(9) => (0.7, 0.9),
        _ => (1., 1.),
    };
    Beat { opacity, scale }
}

/// Which of the 8 dashes leads, so 1000 ms a turn. `None`: the three dots.
pub fn spin_step(tick: Option<u8>) -> Option<u8> {
    tick.map(|tick| tick % 8)
}

/// 1 at tick 0, 0.45 at tick 8, eased both ways (smoothstep, close to CSS `ease-in-out`).
pub fn pulse(tick: Option<u8>) -> f32 {
    let Some(tick) = tick else { return 1. };
    let near = 1. - (f32::from(tick % TICKS) - 8.).abs() / 8.;
    1. - 0.55 * near * near * (3. - 2. * near)
}

/// The frame of `kind` at `tick`, as numbers to compare.
fn frame(kind: Moving, tick: u8) -> (f32, f32) {
    match kind {
        Moving::Beat => {
            let beat = beat(Some(tick));
            (beat.opacity, beat.scale)
        }
        Moving::Spin => (spin_step(Some(tick)).map_or(0., f32::from), 0.),
        Moving::Pulse => (pulse(Some(tick)), 0.),
    }
}

/// Whether `kind` looks different at `tick` than one tick before: only then is a redraw needed.
pub fn changes(kind: Moving, tick: u8) -> bool {
    let before = (tick % TICKS + TICKS - 1) % TICKS;
    frame(kind, tick) != frame(kind, before)
}

/// Ticks from `tick` to the next one where one of `kinds` changes; `None` with no kinds.
pub fn ticks_to_next_change(tick: u8, kinds: &[Moving]) -> Option<u8> {
    (1..=TICKS).find(|ahead| {
        let at = (tick % TICKS + ahead) % TICKS;
        kinds.iter().any(|kind| changes(*kind, at))
    })
}

#[cfg(test)]
#[path = "motion_rules_tests.rs"]
mod tests;
