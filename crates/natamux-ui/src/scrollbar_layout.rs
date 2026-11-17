//! Where a scroll area's bars lie, from the area's bounds and what its scroll handle reports:
//! each bar's lane along the far edge, the way its thumb runs in, and the thumb. Plain geometry,
//! so it needs no window.

use gpui::{Along, Axis, Bounds, Pixels, Point, ScrollHandle, Size, point, px, size};

/// The thumb across.
pub(crate) const THUMB: Pixels = px(6.);
/// The space between the thumb and the lane's sides.
pub(crate) const INSET: Pixels = px(4.);
/// The lane across: the thumb with its inset on both sides.
pub(crate) const LANE: Pixels = px(14.);
const LEAST: Pixels = px(25.);

/// What one axis of a scroll area has.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Lane {
    #[default]
    Off,
    /// A thumb only.
    Bare,
    /// A thumb on a painted track, which takes presses and keeps its room while the area scrolls.
    Tracked,
}

/// The lanes of both axes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Lanes {
    pub(crate) horizontal: Lane,
    pub(crate) vertical: Lane,
}

impl Lanes {
    pub(crate) fn of(&self, axis: Axis) -> Lane {
        match axis {
            Axis::Horizontal => self.horizontal,
            Axis::Vertical => self.vertical,
        }
    }

    pub(crate) fn set(&mut self, axis: Axis, lane: Lane) {
        match axis {
            Axis::Horizontal => self.horizontal = lane,
            Axis::Vertical => self.vertical = lane,
        }
    }
}

/// The part of the viewport (start and end, 0 to 1) the thumb covers, for a viewport `viewport`
/// long that scrolls up to `max` and is now at `offset` (0 or below). None when nothing scrolls
/// or a thumb of the least length does not fit.
pub(crate) fn thumb_part(viewport: Pixels, max: Pixels, offset: Pixels) -> Option<(f32, f32)> {
    if max <= Pixels::ZERO || viewport <= Pixels::ZERO {
        return None;
    }
    let shown = viewport / (viewport + max);
    let length = (viewport * shown).max(LEAST);
    if length > viewport {
        return None;
    }
    let scrolled = (-offset).max(Pixels::ZERO).min(max);
    let start = (scrolled / max) * (viewport - length);
    Some((start / viewport, (start + length) / viewport))
}

/// One axis's bar as drawn.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Piece {
    pub(crate) axis: Axis,
    pub(crate) lane: Lane,
    /// The whole lane.
    pub(crate) strip: Bounds<Pixels>,
    /// The strip less its inset: where the thumb runs.
    pub(crate) way: Bounds<Pixels>,
    pub(crate) thumb: Bounds<Pixels>,
}

/// What a press at a point does to a bar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Hit {
    /// The thumb is taken, `grab` px from its start.
    Grab { axis: Axis, grab: Pixels },
    /// The track was hit beside the thumb: scroll to `offset` along `axis`.
    Jump { axis: Axis, offset: Pixels },
}

impl Piece {
    /// Where the bar takes the pointer: the whole way on a track, else just the thumb.
    pub(crate) fn target(&self) -> Bounds<Pixels> {
        match self.lane {
            Lane::Tracked => self.way,
            _ => self.thumb,
        }
    }

    fn thumb_start(&self) -> Pixels {
        self.thumb.origin.along(self.axis) - self.way.origin.along(self.axis)
    }

    fn thumb_length(&self) -> Pixels {
        self.thumb.size.along(self.axis)
    }

    /// The offset that puts the thumb, held `grab` px from its start, under `at`, for an area
    /// that scrolls up to `max` on this axis.
    pub(crate) fn offset_at(&self, at: Point<Pixels>, grab: Pixels, max: Pixels) -> Pixels {
        let free = self.way.size.along(self.axis) - self.thumb_length();
        let wanted = at.along(self.axis) - self.way.origin.along(self.axis) - grab;
        let start = wanted.max(Pixels::ZERO).min(free);
        let part = if free > Pixels::ZERO {
            start / free
        } else {
            0.
        };
        -max * part
    }
}

/// The bars of one frame, horizontal first.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Layout {
    pub(crate) pieces: Vec<Piece>,
}

impl Layout {
    pub(crate) fn measure(area: Bounds<Pixels>, lanes: Lanes, handle: &ScrollHandle) -> Layout {
        Layout::of(
            area,
            lanes,
            handle.max_offset(),
            handle.bounds().size,
            handle.offset(),
        )
    }

    /// The bars over `area` for a viewport of `viewport` that scrolls up to `max` and is at
    /// `offset`.
    pub(crate) fn of(
        area: Bounds<Pixels>,
        lanes: Lanes,
        max: Point<Pixels>,
        viewport: Size<Pixels>,
        offset: Point<Pixels>,
    ) -> Layout {
        let parts: Vec<(Axis, Lane, (f32, f32))> = [Axis::Horizontal, Axis::Vertical]
            .into_iter()
            .filter(|&axis| lanes.of(axis) != Lane::Off)
            .filter_map(|axis| {
                let part = thumb_part(viewport.along(axis), max.along(axis), offset.along(axis))?;
                Some((axis, lanes.of(axis), part))
            })
            .collect();
        // Two bars would meet in the corner, so each thumb runs a thumb's width short of it.
        let corner = if parts.len() == 2 {
            THUMB
        } else {
            Pixels::ZERO
        };
        let pieces = parts
            .into_iter()
            .map(|(axis, lane, (from, to))| {
                let strip = strip(area, axis);
                let way = strip.dilate(-INSET);
                let room = way.size.along(axis) - corner;
                let (start, end) = (from * room, to * room);
                let thumb = Bounds::new(
                    way.origin.apply_along(axis, |origin| origin + start),
                    way.size.apply_along(axis, |_| end - start),
                );
                Piece {
                    axis,
                    lane,
                    strip,
                    way,
                    thumb,
                }
            })
            .collect();
        Layout { pieces }
    }

    /// The bar whose thumb is under `at`.
    pub(crate) fn thumb_at(&self, at: Point<Pixels>) -> Option<usize> {
        self.pieces.iter().position(|piece| piece.thumb.contains(&at))
    }

    pub(crate) fn along(&self, axis: Axis) -> Option<&Piece> {
        self.pieces.iter().find(|piece| piece.axis == axis)
    }

    /// What a press at `at` does, for an area that scrolls up to `max`.
    pub(crate) fn press(&self, at: Point<Pixels>, max: Point<Pixels>) -> Option<Hit> {
        let piece = self
            .pieces
            .iter()
            .find(|piece| piece.target().contains(&at))?;
        let axis = piece.axis;
        if piece.thumb.contains(&at) {
            let grab = at.along(axis) - piece.thumb.origin.along(axis);
            return Some(Hit::Grab { axis, grab });
        }
        let half = piece.thumb_length() / 2.;
        Some(Hit::Jump {
            axis,
            offset: piece.offset_at(at, half, max.along(axis)),
        })
    }

    /// The thumbs lie where they lay in `other`: same bars, starts and lengths.
    pub(crate) fn same_thumbs(&self, other: &Layout) -> bool {
        let key = |piece: &Piece| (piece.axis, piece.thumb_start(), piece.thumb_length());
        self.pieces.len() == other.pieces.len()
            && self
                .pieces
                .iter()
                .zip(&other.pieces)
                .all(|(a, b)| key(a) == key(b))
    }
}

/// The lane of a bar along `axis`: the area's whole length, on its bottom or right edge.
fn strip(area: Bounds<Pixels>, axis: Axis) -> Bounds<Pixels> {
    match axis {
        Axis::Horizontal => Bounds::new(
            point(area.left(), area.bottom() - LANE),
            size(area.size.width, LANE),
        ),
        Axis::Vertical => Bounds::new(
            point(area.right() - LANE, area.top()),
            size(LANE, area.size.height),
        ),
    }
}

#[cfg(test)]
#[path = "scrollbar_layout_tests.rs"]
mod tests;
