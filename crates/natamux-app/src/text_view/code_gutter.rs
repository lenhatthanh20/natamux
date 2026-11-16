//! The gutter and the sticky rows of a code view (plan 6.4, S4.3c), for the editor tab:
//! - line numbers right-aligned in a gutter as wide as Zed's editor made it there: room for at
//!   least 4 digits, the git strip and the numbers 2 ch on the left, the fold markers 4 ch on the
//!   right (D12; folding itself is dropped);
//! - the rows of the outline items the top row is inside, pinned at the top, each pushed up as
//!   its item ends (Zed's sticky scroll, the items of `outline::containing`);
//! - the guides and the outline, kept until the text changes.

use std::ops::Range;
use std::rc::Rc;

use gpui::{App, Bounds, Font, Pixels, TextAlign, TextRun, Window, fill, point, px, size};
use theme::ActiveTheme;

use super::code_paint::CodePaint;
use super::code_rules::{Guide, active_guides, guides};
use super::code_runs::line_bytes;
use super::element::Layout;
use super::outline::{OutlineItem, containing, outline, outline_in};
use super::text_input::TextInput;
use crate::git::diff_model::Status;
use crate::text::motion;

/// The digits the gutter always has room for.
const MIN_DIGITS: usize = 4;
/// Left of the numbers, in ch: the git strip and the numbers take one each.
const LEFT_PAD: f32 = 2.;
/// Right of the numbers, in ch: the room of the fold markers.
const RIGHT_PAD: f32 = 4.;
/// Left and right of the numbers of a diff side, in ch: there the numbers sit nearer the text
/// (measured on the `changes-diff-*` shots; recheck at the S6.2 flip).
const DIFF_LEFT_PAD: f32 = 4.;
const DIFF_RIGHT_PAD: f32 = 3.;
/// Sticky scroll looks for items this many lines below the top one.
pub(super) const STICKY_LINES: usize = 10;

/// The guides and the outline of the text, kept until it changes.
#[derive(Default)]
pub(super) struct Caches {
    /// By the text version.
    guides: Option<(u64, Rc<Vec<Guide>>)>,
    /// By the text version and the caret's line.
    active: Option<(u64, usize, Rc<Vec<usize>>)>,
    /// By the colour count, which goes up with each new tree.
    outline: Option<(u64, Rc<Vec<OutlineItem>>)>,
    /// The items around the top rows, by the colour count and their bytes.
    near: Option<(u64, Range<usize>, Rc<Vec<OutlineItem>>)>,
}

/// The gutter as painted: its box, the x the numbers end at, and each shown number with its top
/// and whether it is on a line of the selection. A diff side has no background of its own (its
/// hunk boxes reach under the numbers) and shows the numbers of added lines in their colour.
pub(super) struct GutterPaint {
    pub bounds: Bounds<Pixels>,
    pub right: Pixels,
    pub numbers: Vec<(Pixels, usize, bool)>,
    pub added: Vec<bool>,
    pub background: bool,
}

/// A pinned row: the shown row, its line number and its top.
pub(super) struct StickyPaint {
    pub row: usize,
    pub number: usize,
    pub top: Pixels,
}

/// A pinned row as sticky scroll finds it: the shown row of the item's head, its line, and how
/// far from the top it is drawn, in rows.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Sticky {
    pub row: usize,
    pub line: usize,
    pub offset: f32,
}

/// The gutter's width and the room right of its numbers, for `lines` lines, from the width and
/// the advance of `0`.
pub(super) fn gutter_widths(
    lines: usize,
    ch_width: Pixels,
    ch_advance: Pixels,
) -> (Pixels, Pixels) {
    widths(lines, ch_width, ch_advance, (LEFT_PAD, RIGHT_PAD))
}

/// The gutter's width and the room right of its numbers, with `pads` ch left and right of them.
fn widths(
    lines: usize,
    ch_width: Pixels,
    ch_advance: Pixels,
    (left, right): (f32, f32),
) -> (Pixels, Pixels) {
    let digits = lines.max(1).to_string().len().max(MIN_DIGITS);
    let right = ch_width * right;
    (ch_advance * digits as f32 + ch_width * left + right, right)
}

/// The width and the advance of `0` in `font`.
fn ch_sizes(font: &Font, font_size: Pixels, window: &Window) -> (Pixels, Pixels) {
    let text_system = window.text_system();
    let id = text_system.resolve_font(font);
    let ch_width = text_system
        .ch_width(id, font_size)
        .unwrap_or(font_size / 2.);
    let ch_advance = text_system.ch_advance(id, font_size).unwrap_or(ch_width);
    (ch_width, ch_advance)
}

/// `gutter_widths` in `font`.
pub(super) fn gutter_size(
    lines: usize,
    font: &Font,
    font_size: Pixels,
    window: &Window,
) -> (Pixels, Pixels) {
    let (ch_width, ch_advance) = ch_sizes(font, font_size, window);
    gutter_widths(lines, ch_width, ch_advance)
}

/// The gutter of a diff side whose largest line number is `most`.
pub(super) fn diff_gutter_size(
    most: usize,
    font: &Font,
    font_size: Pixels,
    window: &Window,
) -> (Pixels, Pixels) {
    let (ch_width, ch_advance) = ch_sizes(font, font_size, window);
    widths(most, ch_width, ch_advance, (DIFF_LEFT_PAD, DIFF_RIGHT_PAD))
}

/// The rows sticky scroll pins with the view scrolled `scroll_top` rows down. `items` are the
/// outline items around the top lines, in text order (outer items first); `row_of` gives the
/// shown row of an offset. The rule:
/// - each item with more than its head row and the row before its end pins its head row once;
/// - pinned rows stack: an item sits below the pinned items still open on its head row;
/// - it is pinned while its head row is scrolled above its slot and its end is below it;
/// - it never goes below the row before its end, so it moves up as its item ends.
pub(super) fn sticky_rows(
    items: &[OutlineItem],
    text: &str,
    scroll_top: f32,
    row_of: impl Fn(usize) -> usize,
) -> Vec<Sticky> {
    // Each pinned row with the shown row its item ends on.
    let mut pinned: Vec<(Sticky, usize)> = Vec::new();
    for item in items {
        let head = head_offset(text, item);
        let row = row_of(head);
        let end = row_of(item.range.end);
        let same_head = pinned.last().is_some_and(|(last, _)| last.row == row);
        if same_head || end < row + 2 {
            continue;
        }
        // Outline ranges nest or follow each other: the open ones are the items around this one.
        let above = pinned.iter().filter(|(_, ends)| *ends > row).count() as f32;
        let slot = scroll_top + above;
        let scrolled_past = (row as f32) < slot;
        let still_open = slot < end as f32;
        if scrolled_past && still_open {
            let lowest = (end - 1) as f32 - scroll_top;
            let sticky = Sticky {
                row,
                line: text[..head].matches('\n').count(),
                offset: above.min(lowest),
            };
            pinned.push((sticky, end));
        }
    }
    pinned.into_iter().map(|(sticky, _)| sticky).collect()
}

/// Where a pinned row starts: the item's first shown part when it is on the line of its name,
/// else the start of that line.
pub(super) fn head_offset(text: &str, item: &OutlineItem) -> usize {
    let name_line = motion::line_start(text, item.selection.start);
    if motion::line_start(text, item.source.start) == name_line {
        item.source.start
    } else {
        name_line
    }
}

/// The lines the selection `range` of `text` touches, both ends included.
pub(super) fn selected_lines(text: &str, range: Range<usize>) -> Range<usize> {
    let line = |at: usize| text[..at.min(text.len())].matches('\n').count();
    line(range.start)..line(range.end) + 1
}

impl TextInput {
    /// The guides of the text and the ones active with the caret on `caret_line`.
    pub(super) fn cached_guides(&self, caret_line: usize) -> (Rc<Vec<Guide>>, Rc<Vec<usize>>) {
        let Some(code) = self.code.as_ref() else {
            return (Rc::default(), Rc::default());
        };
        let version = self.model.version();
        let text = self.model.text();
        let mut kept = code.caches.borrow_mut();
        let caches = &mut *kept;
        let found = match &caches.guides {
            Some((at, found)) if *at == version => found.clone(),
            _ => {
                let found = Rc::new(guides(text));
                caches.guides = Some((version, found.clone()));
                caches.active = None;
                found
            }
        };
        let active = match &caches.active {
            Some((at, line, active)) if *at == version && *line == caret_line => active.clone(),
            _ => {
                let active = Rc::new(active_guides(text, &found, caret_line));
                caches.active = Some((version, caret_line, active.clone()));
                active
            }
        };
        (found, active)
    }

    /// The outline items of the text, once it has a tree.
    pub(super) fn cached_outline(&self) -> Rc<Vec<OutlineItem>> {
        let Some(code) = self.code.as_ref() else {
            return Rc::default();
        };
        let (Some(lang), Some(tree)) = (code.lang, code.tree()) else {
            return Rc::default();
        };
        let mut caches = code.caches.borrow_mut();
        if let Some((colors, items)) = &caches.outline
            && *colors == code.colors
        {
            return items.clone();
        }
        let items = Rc::new(outline(lang, tree, self.model.text()));
        caches.outline = Some((code.colors, items.clone()));
        items
    }

    /// The outline items that meet `near` (the bytes of the top rows), each as deep as the found
    /// items around it: what sticky scroll looks at. Only the items there are looked for, so a
    /// key in a big file does not find the whole outline again.
    pub(super) fn near_outline(&self, near: Range<usize>) -> Rc<Vec<OutlineItem>> {
        let Some(code) = self.code.as_ref() else {
            return Rc::default();
        };
        let (Some(lang), Some(tree)) = (code.lang, code.tree()) else {
            return Rc::default();
        };
        let mut caches = code.caches.borrow_mut();
        if let Some((colors, bytes, items)) = &caches.near
            && *colors == code.colors
            && *bytes == near
        {
            return items.clone();
        }
        let found = outline_in(lang, tree, self.model.text(), Some(near.clone()));
        let items = Rc::new(containing(&found, near.clone()));
        caches.near = Some((code.colors, near, items.clone()));
        items
    }

    /// A point left of the text, in the gutter.
    pub(super) fn in_gutter(&self, position: gpui::Point<Pixels>) -> bool {
        let gutter = self.laid.as_ref().map_or(px(0.), |laid| laid.key.gutter);
        gutter > px(0.)
            && self
                .last_bounds
                .is_some_and(|bounds| position.x < bounds.left() + gutter)
    }

    /// The gutter and the pinned rows of a code view drawn over `bounds`, where the text starts
    /// at `origin`.
    pub(super) fn gutter_paint(
        &self,
        laid: &Layout,
        bounds: Bounds<Pixels>,
        origin: gpui::Point<Pixels>,
        paint: &mut CodePaint,
    ) {
        let Some(code) = self.code.as_ref() else {
            return;
        };
        let text = self.model.text();
        let line_height = laid.line_height;
        if laid.key.gutter > px(0.) {
            let selected = selected_lines(text, self.model.selection().range());
            let diff = code.diff.as_ref();
            let mut added = Vec::new();
            let numbers = laid
                .lines
                .iter()
                .enumerate()
                .filter_map(|(index, line)| {
                    let top = origin.y + line_height * line.first_row as f32;
                    let shown = top + line_height >= bounds.top() && top <= bounds.bottom();
                    // A diff side numbers its lines as in their file; deleted lines have none.
                    let (number, is_added) = match diff {
                        Some(diff) => {
                            let line = diff.rows.lines.get(index)?;
                            (line.number? as usize, line.status == Status::Added)
                        }
                        None => (index + 1, false),
                    };
                    let number = shown.then(|| (top, number, selected.contains(&index)))?;
                    added.push(is_added);
                    Some(number)
                })
                .collect();
            paint.gutter = Some(GutterPaint {
                bounds: Bounds::new(bounds.origin, size(laid.key.gutter, bounds.size.height)),
                right: bounds.left() + laid.key.gutter - laid.key.gutter_pad,
                numbers,
                added,
                background: diff.is_none(),
            });
        }
        paint.sticky = self.sticky_paint(laid, bounds);
    }

    /// The pinned rows of a code view with sticky scroll, drawn over `bounds`.
    pub(super) fn sticky_paint(&self, laid: &Layout, bounds: Bounds<Pixels>) -> Vec<StickyPaint> {
        let line_height = laid.line_height;
        if !self.code.as_ref().is_some_and(|code| code.sticky) || line_height <= px(0.) {
            return Vec::new();
        }
        let text = self.model.text();
        let scroll_top = self.scroll.y / line_height;
        let top_line = laid.line_index_of_row(scroll_top as usize);
        let last_line = laid.lines.len().saturating_sub(1);
        let near = line_bytes(text, &(top_line..(top_line + STICKY_LINES).min(last_line)));
        let around = self.near_outline(near);
        if around.is_empty() {
            return Vec::new();
        }
        let row_of = |at: usize| laid.row_of(self.to_display(at), false);
        sticky_rows(&around, text, scroll_top, row_of)
            .into_iter()
            .map(|sticky| StickyPaint {
                row: sticky.row,
                number: sticky.line + 1,
                top: bounds.top() + line_height * sticky.offset,
            })
            .collect()
    }
}

/// Paints the gutter: its background over any text scrolled under it, then the numbers, the
/// lines of the selection in the active colour.
pub(super) fn paint_gutter(gutter: &GutterPaint, laid: &Layout, window: &mut Window, cx: &mut App) {
    let colors = cx.theme().colors();
    let (background, normal, active, added) = (
        colors.editor_gutter_background,
        colors.editor_line_number,
        colors.editor_active_line_number,
        colors.version_control_added,
    );
    if gutter.background {
        window.paint_quad(fill(gutter.bounds, background));
    }
    for (index, (top, number, selected)) in gutter.numbers.iter().enumerate() {
        // A line of the selection beats an added line (shot `changes-diff-focused`).
        let color = if *selected {
            active
        } else if gutter.added.get(index).copied().unwrap_or(false) {
            added
        } else {
            normal
        };
        paint_number(*number, color, gutter.right, *top, laid, window, cx);
    }
}

/// Paints the pinned rows over the text, deepest first, each with the gutter and the text
/// background (tinted on the `hovered` one, as Zed's sticky scroll did), its line number, and a
/// line under the last one.
pub(super) fn paint_sticky(
    paint: &CodePaint,
    laid: &Layout,
    bounds: Bounds<Pixels>,
    origin: gpui::Point<Pixels>,
    hovered: Option<usize>,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(last) = paint.sticky.last() else {
        return;
    };
    let colors = cx.theme().colors();
    let (gutter_background, background, number, border, overlay) = (
        colors.editor_gutter_background,
        colors.editor_background,
        colors.editor_line_number,
        colors.border_variant,
        colors.panel_overlay_hover,
    );
    let line_height = laid.line_height;
    let gutter = laid.key.gutter;
    for (index, sticky) in paint.sticky.iter().enumerate().rev() {
        let row = Bounds::new(
            point(bounds.left(), sticky.top),
            size(bounds.size.width, line_height),
        );
        window.paint_layer(row, |window| {
            let gutter_box = Bounds::new(row.origin, size(gutter, line_height));
            window.paint_quad(fill(gutter_box, gutter_background));
            let text_box = Bounds::from_corners(
                point(bounds.left() + gutter, sticky.top),
                point(bounds.right(), sticky.top + line_height),
            );
            window.paint_quad(fill(text_box, background));
            if hovered == Some(index) {
                window.paint_quad(fill(gutter_box, overlay));
                window.paint_quad(fill(text_box, overlay));
            }
            if let Some((line, in_line)) = laid.line_of_row(sticky.row) {
                window.with_content_mask(Some(gpui::ContentMask { bounds: text_box }), |window| {
                    line.paint_row(
                        in_line,
                        point(origin.x, sticky.top),
                        line_height,
                        window,
                        cx,
                    );
                });
            }
            if gutter > px(0.) {
                let right = bounds.left() + gutter - laid.key.gutter_pad;
                paint_number(sticky.number, number, right, sticky.top, laid, window, cx);
            }
        });
    }
    let border_top = last.top + line_height;
    window.paint_quad(fill(
        Bounds::from_corners(
            point(bounds.left(), border_top),
            point(bounds.right(), border_top + px(1.)),
        ),
        border,
    ));
}

/// Paints `number` in the text font, its right edge at `right`.
fn paint_number(
    number: usize,
    color: gpui::Hsla,
    right: Pixels,
    top: Pixels,
    laid: &Layout,
    window: &mut Window,
    cx: &mut App,
) {
    let text = number.to_string();
    let run = TextRun {
        len: text.len(),
        font: laid.key.font.clone(),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let shaped = window
        .text_system()
        .shape_line(text.into(), laid.key.font_size, &[run], None);
    let x = right - shaped.width;
    shaped
        .paint(
            point(x, top),
            laid.line_height,
            TextAlign::Left,
            None,
            window,
            cx,
        )
        .ok();
}

#[cfg(test)]
#[path = "code_gutter_tests.rs"]
mod tests;
