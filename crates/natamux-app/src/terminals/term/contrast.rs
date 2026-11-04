//! The minimum contrast of terminal text (docs/plan-mit-license.md 6.2.6): text whose APCA
//! lightness contrast with its background is below the minimum gets a new lightness, then less
//! saturation, and black or white as the last resort. APCA is the public W3 0.0.98G-4g model
//! (github.com/Myndex/apca-w3). All math is f32, as the golden table T-C1 was made in f32.

use gpui::{Hsla, Rgba};

const EXPONENT: f32 = 2.4;
const RED: f32 = 0.212_672_9;
const GREEN: f32 = 0.715_152_2;
const BLUE: f32 = 0.072_175;
const BLACK_THRESHOLD: f32 = 0.022;
const BLACK_CLAMP: f32 = 1.414;
const DELTA_Y_MIN: f32 = 0.0005;
const SCALE: f32 = 1.14;
const OFFSET: f32 = 0.027;
const LOW_CLIP: f32 = 0.1;
/// Exponents of the background and the text, for dark text on light and for light text on dark.
const DARK_ON_LIGHT: (f32, f32) = (0.56, 0.57);
const LIGHT_ON_DARK: (f32, f32) = (0.65, 0.62);
/// Halvings of the lightness search.
const STEPS: usize = 20;
/// The search stops once a tried lightness is this close to the minimum, above or below it.
const CLOSE: f32 = 1.0;
/// The saturation factors tried in turn, each with its own lightness search.
const SATURATIONS: [f32; 6] = [1.0, 0.8, 0.6, 0.4, 0.2, 0.0];

/// The screen luminance Y of a colour; alpha plays no part.
fn luminance(colour: Hsla) -> f32 {
    let rgb = Rgba::from(colour);
    RED * rgb.r.powf(EXPONENT) + GREEN * rgb.g.powf(EXPONENT) + BLUE * rgb.b.powf(EXPONENT)
}

/// Near black, Y is raised softly, so very dark pairs do not count as far apart.
fn soft_black(y: f32) -> f32 {
    if y > BLACK_THRESHOLD {
        y
    } else {
        y + (BLACK_THRESHOLD - y).powf(BLACK_CLAMP)
    }
}

/// The APCA lightness contrast Lc of text on a background: positive for dark text on light,
/// negative for light text on dark, 0 when too close to tell.
pub fn lightness_contrast(text: Hsla, background: Hsla) -> f32 {
    let text = soft_black(luminance(text));
    let back = soft_black(luminance(background));
    if (back - text).abs() < DELTA_Y_MIN {
        return 0.;
    }
    let contrast = if back > text {
        let (b, t) = DARK_ON_LIGHT;
        let raw = (back.powf(b) - text.powf(t)) * SCALE;
        if raw < LOW_CLIP { 0. } else { raw - OFFSET }
    } else {
        let (b, t) = LIGHT_ON_DARK;
        let raw = (back.powf(b) - text.powf(t)) * SCALE;
        if raw > -LOW_CLIP { 0. } else { raw + OFFSET }
    };
    contrast * 100.
}

fn enough(colour: Hsla, background: Hsla, minimum: f32) -> bool {
    lightness_contrast(colour, background).abs() >= minimum
}

/// The foreground to draw on `background` so the contrast is at least `minimum` (an absolute Lc,
/// 45 in the terminal). Hue and alpha are kept.
pub fn with_minimum_contrast(foreground: Hsla, background: Hsla, minimum: f32) -> Hsla {
    if minimum <= 0. || enough(foreground, background, minimum) {
        return foreground;
    }
    for factor in SATURATIONS {
        let start = Hsla {
            s: foreground.s * factor,
            ..foreground
        };
        let found = search_lightness(start, background, minimum);
        if enough(found, background, minimum) {
            return found;
        }
    }
    let gray = Hsla {
        s: 0.,
        ..foreground
    };
    if enough(gray, background, minimum) {
        return gray;
    }
    let plain = |l: f32| Hsla {
        h: 0.,
        s: 0.,
        l,
        a: foreground.a,
    };
    let (black, white) = (plain(0.), plain(1.));
    let contrast = |colour| lightness_contrast(colour, background).abs();
    if contrast(white) > contrast(black) {
        white
    } else {
        black
    }
}

/// Halves the lightness range between the colour and black (on a light background) or white (on
/// a dark one), keeping the lightness nearest the colour that has enough contrast. A tried
/// lightness within `CLOSE` of the minimum ends the search and is the answer, even just below
/// the minimum: the caller then tries less saturation.
fn search_lightness(colour: Hsla, background: Hsla, minimum: f32) -> Hsla {
    let darker = luminance(background) > 0.5;
    let (mut low, mut high) = if darker {
        (0., colour.l)
    } else {
        (colour.l, 1.)
    };
    let mut best = colour.l;
    for _ in 0..STEPS {
        let mid = (low + high) / 2.;
        let contrast = lightness_contrast(Hsla { l: mid, ..colour }, background).abs();
        let passes = contrast >= minimum;
        if passes {
            best = mid;
        }
        // Toward the colour when it passes, away from it when not.
        if passes == darker {
            low = mid;
        } else {
            high = mid;
        }
        if (contrast - minimum).abs() < CLOSE {
            best = mid;
            break;
        }
    }
    Hsla { l: best, ..colour }
}
