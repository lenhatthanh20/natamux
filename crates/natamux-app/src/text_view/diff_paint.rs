//! What a diff side paints past the code view (plan 6.5, S6.2 part 2), with the values measured
//! on the `changes-diff-*` shots of B2:
//! - each hunk's lines in a box: the hollow background of each line's kind, a 1 px line of the
//!   box's first and last kind at its top and bottom, from the strip's right to the right edge;
//! - the strip in the gutter, floor(0.275 x line height) wide, filled with the colour of the
//!   hunk's kind at 0.3 inside a 1 px line of it;
//! - a 1 px `border_variant` line half a row down a gap row;
//! - the changed words, and the marker of a hunk with no lines on the side.

use gpui::{App, Bounds, ContentMask, Hsla, Pixels, Point, Window, fill, point, px};
use theme::{ActiveTheme, ThemeColors};

use super::element::Layout;
use super::text_input::TextInput;
use crate::git::diff_model::Status;
use crate::git::diff_rows::{BlockKind, Kind, SideRows};

/// The width of the strip of a hunk in the gutter, per line height.
const STRIP: f32 = 0.275;
/// The size of the marker of a hunk with no rows on a side, per line height.
const MARKER: f32 = 0.35;
/// The strip and the marker are filled with their colour at this opacity, inside a line of it.
const STRIP_FILL: f32 = 0.3;

/// What a diff side paints past the code view, in window coordinates.
#[derive(Default)]
pub(super) struct DiffPaint {
    /// Over the line of the caret, before the guides: the rows of each hunk, the lines at the top
    /// and the bottom of its box, then the lines of the gaps.
    pub backs: Vec<(Bounds<Pixels>, Hsla)>,
    /// The changed words, after `backs`, kept out of the gutter as the text is.
    pub words: Vec<(Bounds<Pixels>, Hsla)>,
    /// The strips and markers in the gutter, painted hollow.
    pub strips: Vec<(Bounds<Pixels>, Hsla)>,
    /// The text keeps to the right of this, out of the gutter.
    pub text_left: Pixels,
}

impl DiffPaint {
    /// What the text layer of a side drawn over `bounds` is clipped to: the gutter has no
    /// background to cover text, guides, selection or carets scrolled under it.
    pub fn text_mask(&self, bounds: Bounds<Pixels>) -> ContentMask<Pixels> {
        ContentMask {
            bounds: Bounds::from_corners(
                point(self.text_left, bounds.top()),
                bounds.bottom_right(),
            ),
        }
    }
}

/// Where a diff side is drawn, for the parts of `DiffPaint`.
struct Frame<'a> {
    laid: &'a Layout,
    bounds: Bounds<Pixels>,
    origin: Point<Pixels>,
    /// The width of a hunk's strip.
    strip: Pixels,
    colors: &'a ThemeColors,
}

impl Frame<'_> {
    fn row_top(&self, row: usize) -> Pixels {
        self.origin.y + self.laid.line_height * row as f32
    }

    fn shown(&self, top: Pixels, bottom: Pixels) -> bool {
        bottom >= self.bounds.top() && top <= self.bounds.bottom()
    }

    /// The background and the line colour of a box's rows of `status`.
    fn box_colours(&self, status: Status) -> (Hsla, Hsla) {
        match status {
            Status::Deleted => (
                self.colors.editor_diff_hunk_deleted_hollow_background,
                self.colors.editor_diff_hunk_deleted_hollow_border,
            ),
            _ => (
                self.colors.editor_diff_hunk_added_hollow_background,
                self.colors.editor_diff_hunk_added_hollow_border,
            ),
        }
    }

    fn strip_colour(&self, kind: Option<&Kind>) -> Hsla {
        match kind {
            Some(Kind::Added) => self.colors.version_control_added,
            Some(Kind::Deleted) => self.colors.version_control_deleted,
            _ => self.colors.version_control_modified,
        }
    }

    /// A row of the box from the strip's right to the right edge, `height` tall.
    fn across(&self, top: Pixels, height: Pixels) -> Bounds<Pixels> {
        Bounds::from_corners(
            point(self.bounds.left() + self.strip, top),
            point(self.bounds.right(), top + height),
        )
    }

    /// Each run of changed lines of one hunk: its lines in the background of their own kind
    /// (inline, a hunk's deleted lines come right above its added ones, in one box), a line at
    /// the box's top and bottom, and the strip.
    fn hunks(&self, rows: &SideRows, paint: &mut DiffPaint) {
        let (laid, lines) = (self.laid, &rows.lines);
        let changed = |at: usize, hunk: usize| {
            lines
                .get(at)
                .is_some_and(|line| line.hunk == Some(hunk) && line.status != Status::Unchanged)
        };
        let mut index = 0;
        while index < lines.len() {
            let Some(hunk) = lines[index].hunk.filter(|&hunk| changed(index, hunk)) else {
                index += 1;
                continue;
            };
            let first = index;
            while changed(index, hunk) {
                index += 1;
            }
            let last = index - 1;
            let (Some(top_line), Some(bottom_line)) = (laid.lines.get(first), laid.lines.get(last))
            else {
                continue;
            };
            let top = self.row_top(top_line.first_row);
            let bottom = self.row_top(bottom_line.first_row + bottom_line.row_starts.len());
            if !self.shown(top, bottom) {
                continue;
            }
            let laid_lines = laid.lines.get(first..=last).unwrap_or_default();
            for (line, side_line) in laid_lines.iter().zip(&lines[first..=last]) {
                let height = laid.line_height * line.row_starts.len() as f32;
                let rect = self.across(self.row_top(line.first_row), height);
                paint.backs.push((rect, self.box_colours(side_line.status).0));
            }
            let one = px(1.);
            let (top_colour, bottom_colour) = (
                self.box_colours(lines[first].status).1,
                self.box_colours(lines[last].status).1,
            );
            paint.backs.push((self.across(top, one), top_colour));
            paint.backs.push((self.across(bottom - one, one), bottom_colour));
            let left = self.bounds.left();
            paint.strips.push((
                Bounds::from_corners(point(left, top), point(left + self.strip, bottom)),
                self.strip_colour(rows.kinds.get(hunk)),
            ));
        }
    }

    /// A gap row has a line across it, half a row down.
    fn gaps(&self, rows: &SideRows, paint: &mut DiffPaint) {
        let mut before: Option<(usize, usize)> = None;
        for block in &rows.blocks {
            let offset = match before {
                Some((line, rows)) if line == block.line => rows,
                _ => 0,
            };
            before = Some((block.line, offset + block.rows));
            if block.kind != BlockKind::Gap {
                continue;
            }
            let top = self.row_top(self.laid.blocks_top(block.line) + offset);
            let y = top + (self.laid.line_height / 2.).floor();
            if self.shown(y, y + px(1.)) {
                paint.backs.push((
                    Bounds::from_corners(
                        point(self.bounds.left(), y),
                        point(self.bounds.right(), y + px(1.)),
                    ),
                    self.colors.border_variant,
                ));
            }
        }
    }

    // shortcut: the changed words as plain rects and the marker as a square centred on the row
    // boundary; no B2 shot shows either, so check them on the first shot that does.
    fn words(&self, rows: &SideRows, paint: &mut DiffPaint) {
        for (range, status) in &rows.words {
            let color = match status {
                Status::Deleted => self.colors.version_control_word_deleted,
                _ => self.colors.version_control_word_added,
            };
            for rect in self.laid.selection_rects(range.clone()) {
                let rect = Bounds::new(rect.origin + self.origin, rect.size);
                if self.shown(rect.top(), rect.bottom()) {
                    paint.words.push((rect, color));
                }
            }
        }
    }

    fn markers(&self, rows: &SideRows, paint: &mut DiffPaint) {
        let laid = self.laid;
        let size = (laid.line_height * MARKER).floor();
        for (line, hunk) in &rows.markers {
            let row = laid
                .lines
                .get(*line)
                .map_or_else(|| laid.blocks_top(laid.lines.len()), |line| line.first_row);
            let y = self.row_top(row) - (size / 2.).floor();
            if self.shown(y, y + size) {
                paint.strips.push((
                    Bounds::new(point(self.bounds.left(), y), gpui::size(size, size)),
                    self.strip_colour(rows.kinds.get(*hunk)),
                ));
            }
        }
    }
}

/// Paints a strip or a marker: filled with its colour at `STRIP_FILL`, inside a 1 px line of it.
pub(super) fn paint_hollow(bounds: Bounds<Pixels>, color: Hsla, window: &mut Window) {
    window.paint_quad(fill(bounds, color.opacity(STRIP_FILL)));
    let one = px(1.);
    let (left, top, right, bottom) = (bounds.left(), bounds.top(), bounds.right(), bounds.bottom());
    for edge in [
        Bounds::from_corners(point(left, top), point(right, top + one)),
        Bounds::from_corners(point(left, bottom - one), point(right, bottom)),
        Bounds::from_corners(point(left, top + one), point(left + one, bottom - one)),
        Bounds::from_corners(point(right - one, top + one), point(right, bottom - one)),
    ] {
        window.paint_quad(fill(edge, color));
    }
}

impl TextInput {
    /// The boxes, lines, words and strips of a diff side drawn over `bounds`, where the text
    /// starts at `origin`. None for every other view.
    pub(super) fn diff_paint(
        &self,
        laid: &Layout,
        bounds: Bounds<Pixels>,
        origin: Point<Pixels>,
        cx: &App,
    ) -> Option<DiffPaint> {
        let diff = self.code.as_ref()?.diff.as_ref()?;
        let frame = Frame {
            laid,
            bounds,
            origin,
            strip: (laid.line_height * STRIP).floor(),
            colors: cx.theme().colors(),
        };
        let mut paint = DiffPaint {
            text_left: bounds.left() + laid.key.gutter,
            ..DiffPaint::default()
        };
        frame.hunks(&diff.rows, &mut paint);
        frame.gaps(&diff.rows, &mut paint);
        frame.words(&diff.rows, &mut paint);
        frame.markers(&diff.rows, &mut paint);
        Some(paint)
    }

}

