//! Where the grid of a terminal goes in its element (docs/plan-mit-license.md 6.2.6): the number
//! of rows from whole device pixels per line, the columns after a one cell gutter, and the spare
//! pixels above or below the rows. The golden table T-G1 is the reference.

use gpui::{Bounds, Pixels, Point, px};

/// The grid in the element's bounds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grid {
    /// The top left of the first cell, snapped down to device pixels.
    pub origin: Point<Pixels>,
    pub columns: usize,
    pub lines: usize,
    /// The height the rows take.
    pub height: Pixels,
}

/// The grid for cells of `cell_width` x `line_height` in `bounds`, at `scale` device pixels per
/// pixel. `spare_on_top`: the alternate screen, or the main one scrolled to the bottom with its
/// last row used; then the rows sit at the bottom.
pub fn grid(
    bounds: Bounds<Pixels>,
    cell_width: Pixels,
    line_height: Pixels,
    scale: f32,
    spare_on_top: bool,
) -> Grid {
    let line_dev = (f32::from(line_height) * scale).round().max(1.);
    let free_dev = (f32::from(bounds.size.height) * scale).floor();
    let rows = (free_dev / line_dev).floor().max(1.);
    let height = px(rows * line_dev / scale);
    // Counted from the height again, as a fraction: a very tall pane can get one row more than
    // `rows` (about 170 rows at 13 px). Kept as it is.
    let lines = whole(f32::from(height) / f32::from(line_height));
    let cell = f32::from(cell_width);
    let width = (f32::from(bounds.size.width) - cell).max(cell * 2.);
    let columns = whole(width / cell);
    let spare = if spare_on_top {
        bounds.size.height - height
    } else {
        px(0.)
    };
    let snap = |value: Pixels| px((f32::from(value) * scale).floor() / scale);
    Grid {
        origin: Point {
            x: snap(bounds.origin.x + cell_width),
            y: snap(bounds.origin.y + spare),
        },
        columns,
        lines,
        height,
    }
}

/// The whole cells in a fraction, one float step up, so 3.9999999 counts as 4.
fn whole(fraction: f32) -> usize {
    fraction.next_up().floor().max(1.) as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, size};

    #[test]
    fn puts_the_spare_pixels_on_top_or_below_the_rows() {
        let bounds = Bounds::new(point(px(10.3), px(20.7)), size(px(300.), px(200.)));
        // 2 device pixels per pixel: 34 per line, 400 free, so 11 rows of 187 px and 13 spare.
        let at = |y: f32| Grid {
            origin: point(px(18.), px(y)),
            columns: 36,
            lines: 11,
            height: px(187.),
        };
        assert_eq!(grid(bounds, px(8.), px(17.), 2., false), at(20.5));
        assert_eq!(grid(bounds, px(8.), px(17.), 2., true), at(33.5));
    }
}
