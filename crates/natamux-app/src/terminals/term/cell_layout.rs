//! What the cells of a screen draw (docs/plan-mit-license.md 6.2.6): runs of text, background
//! rects and the rects of block elements, from the copy of the screen the last sync made. It
//! needs no window, so the golden table T-P1 checks it as it is.

use std::collections::HashMap;
use std::ops::RangeInclusive;

use alacritty_terminal::index::Point;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::vte::ansi::{Color, NamedColor};
use gpui::{FontWeight, Hsla, StrikethroughStyle, UnderlineStyle, px};
use theme::Theme;

use super::colors::convert;
use super::contrast::with_minimum_contrast;
use super::model::IndexedCell;

/// The minimum APCA contrast of terminal text.
pub const MINIMUM_CONTRAST: f32 = 45.;
/// Dim text keeps this much of its alpha.
const DIM_ALPHA: f32 = 0.7;
/// A block element is drawn on a grid of this many parts per cell.
pub const BLOCK_COLUMNS: usize = 8;
pub const BLOCK_LINES: usize = 24;

/// How the text of a run looks.
#[derive(Clone, Debug, PartialEq)]
pub struct CellStyle {
    pub color: Hsla,
    pub weight: FontWeight,
    pub italic: bool,
    pub underline: Option<UnderlineStyle>,
    pub strikethrough: Option<StrikethroughStyle>,
}

/// Neighbouring cells of one line drawn as one piece of text. `line` is the shown row.
#[derive(Clone, Debug, PartialEq)]
pub struct TextRun {
    pub line: usize,
    pub column: usize,
    pub text: String,
    /// The cells it covers, a wide character counting once.
    pub cells: usize,
    pub style: CellStyle,
}

/// A run of cells of one line with the same background.
#[derive(Clone, Debug, PartialEq)]
pub struct BackgroundRect {
    pub line: usize,
    pub column: usize,
    pub cells: usize,
    pub color: Hsla,
}

/// A rect of block elements, in parts of a cell: `BLOCK_COLUMNS` across and `BLOCK_LINES` down.
#[derive(Clone, Debug, PartialEq)]
pub struct BlockRect {
    pub line: usize,
    pub column: usize,
    pub columns: usize,
    pub lines: usize,
    pub color: Hsla,
}

#[derive(Default)]
pub struct CellLayout {
    pub runs: Vec<TextRun>,
    pub rects: Vec<BackgroundRect>,
    pub blocks: Vec<BlockRect>,
}

/// Lays out `cells` (grid points, `display_offset` lines scrolled up) in `theme`. The cells of
/// `hovered`, the link under the pointer, take the hover colour and an underline.
pub fn layout(
    cells: &[IndexedCell],
    display_offset: usize,
    theme: &Theme,
    hovered: Option<&RangeInclusive<Point>>,
    minimum_contrast: f32,
) -> CellLayout {
    let mut out = CellLayout::default();
    let mut blocks = Blocks::default();
    // The cell before had zero-width characters, such as an emoji's presentation selector.
    let mut after_extras = false;
    let mut open_run = false;
    for indexed in cells {
        let cell = &indexed.cell;
        let Some(line) = usize::try_from(indexed.point.line.0 + display_offset as i32).ok() else {
            continue;
        };
        let column = indexed.point.column.0;
        let (mut fg, mut bg) = (cell.fg, cell.bg);
        if cell.flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut fg, &mut bg);
        }
        let default_bg = bg == Color::Named(NamedColor::Background);
        let bg_colour = convert(&bg, theme);
        if !default_bg {
            push_rect(&mut out.rects, line, column, bg_colour);
        }
        let had_extras = after_extras;
        after_extras = cell.zerowidth().is_some_and(|extras| !extras.is_empty());
        if cell
            .flags
            .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
        {
            continue;
        }
        let dim = cell.flags.intersects(Flags::DIM);
        if let Some(parts) = block_parts(cell.c) {
            let mut colour = convert(&fg, theme);
            if dim {
                colour.a *= DIM_ALPHA;
            }
            for part in parts {
                let mut colour = colour;
                colour.a *= part.alpha;
                blocks.push(
                    &mut out.blocks,
                    BlockRect {
                        line: line * BLOCK_LINES + part.top,
                        column: column * BLOCK_COLUMNS + part.left,
                        columns: part.width,
                        lines: part.height,
                        color: colour,
                    },
                );
            }
            open_run = false;
            continue;
        }
        let link = cell.hyperlink().is_some();
        let blank = cell.c == ' '
            && default_bg
            && !link
            && !cell
                .flags
                .intersects(Flags::ALL_UNDERLINES | Flags::INVERSE | Flags::STRIKEOUT);
        if blank || (cell.c == ' ' && had_extras) {
            open_run = false;
            continue;
        }
        let hover = hovered.is_some_and(|range| range.contains(&indexed.point));
        let style = cell_style(cell, fg, bg_colour, link, hover, theme, minimum_contrast);
        let mut text = String::from(cell.c);
        if let Some(extras) = cell.zerowidth() {
            text.extend(extras);
        }
        if open_run
            && let Some(run) = out.runs.last_mut()
            && run.line == line
            && run.column + run.cells == column
            && run.style == style
        {
            run.text.push_str(&text);
            run.cells += 1;
            continue;
        }
        out.runs.push(TextRun {
            line,
            column,
            text,
            cells: 1,
            style,
        });
        open_run = true;
    }
    out
}

fn push_rect(rects: &mut Vec<BackgroundRect>, line: usize, column: usize, color: Hsla) {
    if let Some(last) = rects.last_mut()
        && last.line == line
        && last.column + last.cells == column
        && last.color == color
    {
        last.cells += 1;
        return;
    }
    rects.push(BackgroundRect {
        line,
        column,
        cells: 1,
        color,
    });
}

fn cell_style(
    cell: &alacritty_terminal::term::cell::Cell,
    fg: Color,
    background: Hsla,
    link: bool,
    hover: bool,
    theme: &Theme,
    minimum_contrast: f32,
) -> CellStyle {
    let mut color = convert(&fg, theme);
    let chosen_by_app = matches!(fg, Color::Spec(_) | Color::Indexed(16..=255));
    if !chosen_by_app && !decorative(cell.c) {
        color = with_minimum_contrast(color, background, minimum_contrast);
    }
    if cell.flags.intersects(Flags::DIM) {
        color.a *= DIM_ALPHA;
    }
    if hover {
        color = theme.colors().link_text_hover;
    }
    let flags = cell.flags;
    // A double underline draws none, as the old view did.
    let underlined = link
        || hover
        || flags.intersects(
            Flags::UNDERLINE | Flags::UNDERCURL | Flags::DOTTED_UNDERLINE | Flags::DASHED_UNDERLINE,
        );
    let underline = underlined.then(|| UnderlineStyle {
        thickness: px(1.),
        color: Some(color),
        wavy: flags.contains(Flags::UNDERCURL) && !link && !hover,
    });
    let strikethrough = flags
        .contains(Flags::STRIKEOUT)
        .then(|| StrikethroughStyle {
            thickness: px(1.),
            color: Some(color),
        });
    CellStyle {
        color,
        weight: if flags.intersects(Flags::BOLD) {
            FontWeight::BOLD
        } else {
            FontWeight::NORMAL
        },
        italic: flags.intersects(Flags::ITALIC),
        underline,
        strikethrough,
    }
}

/// Lines, blocks, shapes and Powerline glyphs keep their colour: a fix of the contrast would
/// break the shapes they make together.
fn decorative(c: char) -> bool {
    matches!(
        u32::from(c),
        0x2500..=0x257F
            | 0x2580..=0x259F
            | 0x25A0..=0x25FF
            | 0x1FB00..=0x1FB3B
            | 0xE0B0..=0xE0BF
            | 0xE0C0..=0xE0CA
            | 0xE0CC..=0xE0D1
            | 0xE0D2..=0xE0D7
    )
}

/// A filled part of a cell, in the parts of `BLOCK_COLUMNS` x `BLOCK_LINES`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Part {
    left: usize,
    top: usize,
    width: usize,
    height: usize,
    alpha: f32,
}

fn part(left: usize, top: usize, width: usize, height: usize) -> Part {
    Part {
        left,
        top,
        width,
        height,
        alpha: 1.,
    }
}

/// The parts a block element fills, top row first, or None for any other character.
fn block_parts(c: char) -> Option<Vec<Part>> {
    let code = u32::from(c);
    let (full_w, full_h) = (BLOCK_COLUMNS, BLOCK_LINES);
    let eighth_h = full_h / 8;
    let parts = match code {
        // Upper half.
        0x2580 => vec![part(0, 0, full_w, full_h / 2)],
        // Lower one eighth to seven eighths.
        0x2581..=0x2587 => {
            let height = (code - 0x2580) as usize * eighth_h;
            vec![part(0, full_h - height, full_w, height)]
        }
        0x2588 => vec![part(0, 0, full_w, full_h)],
        // Left seven eighths down to one eighth.
        0x2589..=0x258F => vec![part(0, 0, 8 - (code - 0x2588) as usize, full_h)],
        // Right half.
        0x2590 => vec![part(full_w / 2, 0, full_w / 2, full_h)],
        // Light, medium and dark shade.
        0x2591..=0x2593 => {
            let alpha = (code - 0x2590) as f32 * 0.25;
            vec![Part {
                alpha,
                ..part(0, 0, full_w, full_h)
            }]
        }
        // Upper one eighth, right one eighth.
        0x2594 => vec![part(0, 0, full_w, eighth_h)],
        0x2595 => vec![part(full_w - 1, 0, 1, full_h)],
        0x2596..=0x259F => grid_parts(quadrant_cells(code), 2),
        0x1FB00..=0x1FB3B => grid_parts(sextant_cells(code), 3),
        _ => return None,
    };
    Some(parts)
}

/// The filled cells of a quadrant character, as bits: upper left 1, upper right 2, lower left
/// 4, lower right 8.
fn quadrant_cells(code: u32) -> u32 {
    match code {
        0x2596 => 4,
        0x2597 => 8,
        0x2598 => 1,
        0x2599 => 1 | 4 | 8,
        0x259A => 1 | 8,
        0x259B => 1 | 2 | 4,
        0x259C => 1 | 2 | 8,
        0x259D => 2,
        0x259E => 2 | 4,
        _ => 2 | 4 | 8,
    }
}

/// The filled cells of a sextant (two across, three down), as bits from the upper left, row by
/// row. The Unicode block leaves out the empty, the full, and the left and right half patterns,
/// which other characters have.
fn sextant_cells(code: u32) -> u32 {
    let mut bits = code - 0x1FB00 + 1;
    for skipped in [21, 42] {
        if bits >= skipped {
            bits += 1;
        }
    }
    bits
}

/// The parts of a pattern of `rows` x 2 cells: per row, both cells as one part, else the one.
fn grid_parts(bits: u32, rows: usize) -> Vec<Part> {
    let (width, height) = (BLOCK_COLUMNS / 2, BLOCK_LINES / rows);
    let mut parts = Vec::new();
    for row in 0..rows {
        let left = bits & (1 << (row * 2)) != 0;
        let right = bits & (1 << (row * 2 + 1)) != 0;
        let top = row * height;
        match (left, right) {
            (true, true) => parts.push(part(0, top, BLOCK_COLUMNS, height)),
            (true, false) => parts.push(part(0, top, width, height)),
            (false, true) => parts.push(part(width, top, width, height)),
            (false, false) => {}
        }
    }
    parts
}

/// Joins each new block rect to one before it that it continues: to the right on the same lines,
/// or below on the same columns, in the same colour. A joined rect keeps its place in the list.
#[derive(Default)]
struct Blocks {
    /// (line, lines, end column) of each rect, for the join to the right.
    by_right: HashMap<(usize, usize, usize), usize>,
    /// (column, columns, end line) of each rect, for the join below.
    by_bottom: HashMap<(usize, usize, usize), usize>,
}

impl Blocks {
    fn push(&mut self, rects: &mut Vec<BlockRect>, rect: BlockRect) {
        let right_key = (rect.line, rect.lines, rect.column);
        let bottom_key = (rect.column, rect.columns, rect.line);
        let found = self
            .by_right
            .get(&right_key)
            .copied()
            .filter(|&i| rects[i].color == rect.color)
            .map(|i| (i, true))
            .or_else(|| {
                self.by_bottom
                    .get(&bottom_key)
                    .copied()
                    .filter(|&i| rects[i].color == rect.color)
                    .map(|i| (i, false))
            });
        let index = match found {
            Some((i, to_the_right)) => {
                self.forget(&rects[i], i);
                if to_the_right {
                    rects[i].columns += rect.columns;
                } else {
                    rects[i].lines += rect.lines;
                }
                i
            }
            None => {
                rects.push(rect);
                rects.len() - 1
            }
        };
        let joined = &rects[index];
        self.by_right.insert(
            (joined.line, joined.lines, joined.column + joined.columns),
            index,
        );
        self.by_bottom.insert(
            (joined.column, joined.columns, joined.line + joined.lines),
            index,
        );
    }

    fn forget(&mut self, rect: &BlockRect, index: usize) {
        let right = (rect.line, rect.lines, rect.column + rect.columns);
        if self.by_right.get(&right) == Some(&index) {
            self.by_right.remove(&right);
        }
        let bottom = (rect.column, rect.columns, rect.line + rect.lines);
        if self.by_bottom.get(&bottom) == Some(&index) {
            self.by_bottom.remove(&bottom);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sextant_skips_the_half_patterns() {
        assert_eq!(sextant_cells(0x1FB00), 1);
        assert_eq!(sextant_cells(0x1FB13), 20);
        assert_eq!(sextant_cells(0x1FB14), 22);
        assert_eq!(sextant_cells(0x1FB3B), 62);
    }

    #[test]
    fn a_rect_joins_the_one_it_continues() {
        let colour = gpui::black();
        let rect = |line, column, columns, lines| BlockRect {
            line,
            column,
            columns,
            lines,
            color: colour,
        };
        let mut blocks = Blocks::default();
        let mut rects = Vec::new();
        blocks.push(&mut rects, rect(0, 0, 8, 24));
        blocks.push(&mut rects, rect(0, 8, 7, 24));
        blocks.push(&mut rects, rect(24, 0, 8, 12));
        blocks.push(&mut rects, rect(0, 16, 6, 24));
        assert_eq!(
            rects,
            [rect(0, 0, 15, 24), rect(24, 0, 8, 12), rect(0, 16, 6, 24)]
        );
    }
}
