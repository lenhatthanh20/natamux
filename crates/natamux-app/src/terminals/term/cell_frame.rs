//! The cells of a frame, placed and shaped: background rects, runs of text and block elements.
//! Most frames change no cell (a blink of the cursor, a key before its echo, a fade of the
//! scrollbar), so the cells of the last frame are kept and used again while all they are made
//! from is the same.

use std::ops::RangeInclusive;
use std::rc::Rc;
use std::sync::Arc;

use alacritty_terminal::index::Point as GridPoint;
use gpui::{
    Bounds, Font, FontStyle, Hsla, Pixels, Point, ShapedLine, TextRun, WindowTextSystem, point,
    size,
};
use theme::Theme;

use super::{Metrics, merged_rects};
use crate::terminals::term::cell_layout::{self, BLOCK_COLUMNS, BLOCK_LINES, MINIMUM_CONTRAST};
use crate::terminals::term::model::Content;

/// What a frame paints for its cells, in window pixels.
#[derive(Default)]
pub(super) struct CellFrame {
    pub(super) rects: Vec<(Bounds<Pixels>, Hsla)>,
    pub(super) runs: Vec<(Point<Pixels>, ShapedLine)>,
    pub(super) blocks: Vec<(Bounds<Pixels>, Hsla)>,
}

/// All the cells of a frame are made from, but the theme. The version stands for the cells:
/// the model gives another one whenever a copy changed a cell.
#[derive(PartialEq)]
struct Key {
    cells_version: u64,
    display_offset: usize,
    hovered: Option<RangeInclusive<GridPoint>>,
    font: Font,
    font_size: Pixels,
    cell_width: Pixels,
    line_height: Pixels,
    origin: Point<Pixels>,
}

/// The cells of the last frame and what they were made from.
pub struct FrameMemo {
    key: Key,
    /// Held, so no other theme can take its address while it is compared by address.
    theme: Arc<Theme>,
    cells: Rc<CellFrame>,
    #[cfg(test)]
    laid: cell_layout::CellLayout,
}

/// The cells of `content` with the grid's top left at `origin`: those of the last frame when
/// they were made from the same, else laid out and shaped now and kept in `memo`.
pub(super) fn cells(
    memo: &mut Option<FrameMemo>,
    content: &Content,
    theme: &Arc<Theme>,
    metrics: &Metrics,
    origin: Point<Pixels>,
    text_system: &WindowTextSystem,
) -> Rc<CellFrame> {
    let hovered = content.last_hovered_word.as_ref().map(|word| &word.range);
    let key = Key {
        cells_version: content.cells_version,
        display_offset: content.display_offset,
        hovered: hovered.cloned(),
        font: metrics.font.clone(),
        font_size: metrics.font_size,
        cell_width: metrics.cell_width,
        line_height: metrics.line_height,
        origin,
    };
    let lay_out = || {
        cell_layout::layout(
            &content.cells,
            content.display_offset,
            theme,
            hovered,
            MINIMUM_CONTRAST,
        )
    };
    if let Some(kept) = memo.as_ref()
        && kept.key == key
        && Arc::ptr_eq(&kept.theme, theme)
    {
        // A part missing from the key would draw an old screen, so every kept frame of every
        // test is checked against a new layout.
        #[cfg(test)]
        assert!(
            kept.laid == lay_out(),
            "the kept cells are not this frame's"
        );
        return kept.cells.clone();
    }
    #[cfg(test)]
    super::LAYOUTS.with(|count| count.set(count.get() + 1));
    let laid = lay_out();
    #[cfg(test)]
    let checked = laid.clone();
    let (cell_width, line_height) = (metrics.cell_width, metrics.line_height);
    let at = |line: usize, column: usize| {
        point(
            origin.x + cell_width * column as f32,
            origin.y + line_height * line as f32,
        )
    };
    let rects = merged_rects(&laid.rects)
        .into_iter()
        .map(|(rect, lines)| {
            let start = at(rect.line, rect.column);
            let bounds = Bounds {
                origin: point(start.x.floor(), start.y),
                size: size(
                    (cell_width * rect.cells as f32).ceil(),
                    line_height * lines as f32,
                ),
            };
            (bounds, rect.color)
        })
        .collect();
    let runs = laid
        .runs
        .into_iter()
        .map(|run| {
            let font = Font {
                weight: run.style.weight,
                style: if run.style.italic {
                    FontStyle::Italic
                } else {
                    FontStyle::Normal
                },
                ..metrics.font.clone()
            };
            let text_run = TextRun {
                len: run.text.len(),
                font,
                color: run.style.color,
                background_color: None,
                underline: run.style.underline,
                strikethrough: run.style.strikethrough,
            };
            let line = text_system.shape_line(
                run.text.into(),
                metrics.font_size,
                &[text_run],
                Some(cell_width),
            );
            (at(run.line, run.column), line)
        })
        .collect();
    let (part_width, part_height) = (
        cell_width / BLOCK_COLUMNS as f32,
        line_height / BLOCK_LINES as f32,
    );
    let blocks = laid
        .blocks
        .iter()
        .map(|block| {
            let bounds = Bounds {
                origin: point(
                    origin.x + part_width * block.column as f32,
                    origin.y + part_height * block.line as f32,
                ),
                size: size(
                    part_width * block.columns as f32,
                    part_height * block.lines as f32,
                ),
            };
            (bounds, block.color)
        })
        .collect();
    let cells = Rc::new(CellFrame {
        rects,
        runs,
        blocks,
    });
    *memo = Some(FrameMemo {
        key,
        theme: theme.clone(),
        cells: cells.clone(),
        #[cfg(test)]
        laid: checked,
    });
    cells
}
