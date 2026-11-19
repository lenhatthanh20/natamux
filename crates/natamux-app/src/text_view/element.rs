//! The element of the own text input (plan 6.3.3, 6.3.4): it lays the text out with gpui's text
//! system and paints, in order, the background (full mode), the current line, the selection, the
//! whitespace dots inside it (full mode), the text with the input method's underline, and the
//! caret.

use std::cell::OnceCell;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    App, AvailableSpace, Bounds, ContentMask, DispatchPhase, Element, ElementId,
    ElementInputHandler, Entity, Font, GlobalElementId, Hsla, IndentAdjustment, InspectorElementId,
    IntoElement, LayoutId, LineFragment, MouseButton, MouseMoveEvent, MouseUpEvent, Path, Pixels,
    Point, SharedString, Style, TextAlign, TextRun, Window, WindowTextSystem, WrappedLine, fill,
    point, px, relative, size,
};
use theme::ActiveTheme;

use super::code_sticky::Pointer;
use super::text_input::{Mode, TAB_SIZE, TextInput};
use crate::terminals::term::element::rounded_path;

/// The width of the caret.
pub const CARET_WIDTH: Pixels = px(2.);
/// The corner radius of the selection, and how far it reaches past a line end, per line height.
const SELECTION_ROUNDING: f32 = 0.15;

/// What a layout was made from: when one of these changes, the text is laid out again.
#[derive(Clone, Debug, PartialEq)]
pub struct LayoutKey {
    pub version: u64,
    pub width: Option<Pixels>,
    pub font: Font,
    pub font_size: Pixels,
    pub line_height: Pixels,
    pub color: Hsla,
    pub masked: bool,
    pub marked: Option<Range<usize>>,
    pub placeholder: Option<SharedString>,
    /// How far right of the box's left edge the text starts: `text_inset` in an area or a full
    /// box, none in a one-line box (where Zed's editor drew it, B1 shots).
    pub inset: Pixels,
    /// The colour count of a code view: its syntax colours changed with the text the same.
    pub colors: u64,
    /// The gutter's width (part of `inset`) and the room right of its numbers; zero without one.
    pub gutter: Pixels,
    pub gutter_pad: Pixels,
    /// The width the text has room for: the wrap width, also when the lines do not wrap.
    pub view: Pixels,
    /// The lines the syntax colours were asked for (`TextInput::query_lines`), and the head
    /// lines of the items sticky scroll may pin (`TextInput::sticky_heads`).
    pub lines: Option<Range<usize>>,
    pub heads: Vec<usize>,
    /// The painted backgrounds the text keeps its contrast with (`code_contrast.rs`).
    pub marks: Vec<(Range<usize>, Hsla)>,
    /// Empty rows above each line, and one more entry for the rows below the last line: the
    /// spacers and gaps of a diff side (`diff_side.rs`). None for every other view.
    pub blocks: Option<Rc<[usize]>>,
}

/// One line of the text (between two `\n`) and the rows it wraps into.
pub struct Line {
    /// The offset of its first char in the shown text.
    pub start: usize,
    pub len: usize,
    /// The row of the whole text its first row is.
    pub first_row: usize,
    /// The offsets in the line where its rows start; the first is 0. Shared with the line of the
    /// last layout it was kept from.
    pub row_starts: Rc<[usize]>,
    /// The line with its tabs as spaces, in rows.
    rows: Rc<Rows>,
    tabs: Vec<Tab>,
}

/// A line with its tabs as spaces, and its rows, each shaped alone as Zed's editor shaped its
/// display rows: a glyph at a wrap point does not join the glyph across it.
struct Rows {
    text: String,
    /// The runs it is shaped with: a later layout takes the rows as they are for the same text
    /// and runs.
    runs: Vec<TextRun>,
    rows: Vec<Row>,
    font_size: Pixels,
    text_system: Arc<WindowTextSystem>,
}

struct Row {
    /// The offset of its first char in the line with its tabs as spaces.
    shown: usize,
    /// Its chars: the row with the most gives the width of the text.
    chars: usize,
    /// The room left of its text: the line's indent on a row after a wrap.
    indent: Pixels,
    /// Shaped the first time it is drawn or measured, so a long text shapes only the rows in
    /// view, as Zed's editor did.
    shaped: OnceCell<WrappedLine>,
}

impl Rows {
    /// Row `index` and its shape.
    fn row(&self, index: usize) -> Option<(&Row, &WrappedLine)> {
        let row = self.rows.get(index)?;
        let shaped = row.shaped.get_or_init(|| {
            let end = self
                .rows
                .get(index + 1)
                .map_or(self.text.len(), |next| next.shown);
            let part = SharedString::from(self.text[row.shown..end].to_owned());
            shape(
                &part,
                &runs_in(&self.runs, row.shown..end),
                self.font_size,
                &self.text_system,
            )
        });
        Some((row, shaped))
    }
}

/// A tab of a line, shaped as spaces to the next tab stop.
#[derive(Clone, Copy, Debug)]
struct Tab {
    /// Its offset in the line.
    at: usize,
    /// The offset of its first space in the shaped line.
    shown: usize,
    /// How many spaces it shows as.
    width: usize,
}

/// `text` with each tab as spaces to the next tab stop (in chars from the line start), `runs`
/// stretched to match, and the tabs.
fn expand_tabs(text: &str, runs: &[TextRun]) -> (String, Vec<TextRun>, Vec<Tab>) {
    let mut tabs = Vec::new();
    if !text.contains('\t') {
        return (text.to_owned(), runs.to_vec(), tabs);
    }
    let mut shown = String::with_capacity(text.len() + 8);
    let mut column = 0;
    for (at, c) in text.char_indices() {
        match c {
            '\t' => {
                let width = TAB_SIZE - column % TAB_SIZE;
                tabs.push(Tab {
                    at,
                    shown: shown.len(),
                    width,
                });
                shown.push_str(&" ".repeat(width));
                column += width;
            }
            '\n' => {
                shown.push(c);
                column = 0;
            }
            _ => {
                shown.push(c);
                column += 1;
            }
        }
    }
    let mut runs = runs.to_vec();
    let mut start = 0;
    for run in &mut runs {
        let end = start + run.len;
        run.len += tabs
            .iter()
            .filter(|tab| (start..end).contains(&tab.at))
            .map(|tab| tab.width - 1)
            .sum::<usize>();
        start = end;
    }
    (shown, runs, tabs)
}

impl Line {
    fn row_end(&self, row: usize) -> usize {
        self.row_starts.get(row + 1).copied().unwrap_or(self.len)
    }

    /// The offset in the shaped line of `local`.
    fn shown(&self, local: usize) -> usize {
        let extra: usize = self
            .tabs
            .iter()
            .take_while(|tab| tab.at < local)
            .map(|tab| tab.width - 1)
            .sum();
        local + extra
    }

    /// The offset in the line of `shown`. One inside a tab's spaces goes to the nearer side of
    /// the tab, or with `ceil` to its end.
    fn local(&self, shown: usize, ceil: bool) -> usize {
        let mut extra = 0;
        for tab in &self.tabs {
            if shown <= tab.shown {
                break;
            }
            if shown < tab.shown + tab.width {
                let after = ceil || (shown - tab.shown) * 2 >= tab.width;
                return tab.at + usize::from(after);
            }
            extra += tab.width - 1;
        }
        shown - extra
    }

    /// The x of `local` in `row`, from the left of the row.
    fn x_in_row(&self, row: usize, local: usize) -> Pixels {
        self.rows.row(row).map_or(px(0.), |(row, shaped)| {
            let at = self.shown(local).saturating_sub(row.shown);
            row.indent + shaped.unwrapped_layout.x_for_index(at)
        })
    }

    /// Paints `row` with its top left at `origin` (the left of the text).
    pub fn paint_row(
        &self,
        row: usize,
        origin: Point<Pixels>,
        line_height: Pixels,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some((row, shaped)) = self.rows.row(row) {
            shaped
                .paint(
                    origin + point(row.indent, px(0.)),
                    line_height,
                    TextAlign::Left,
                    None,
                    window,
                    cx,
                )
                .ok();
        }
    }
}

/// The shown text laid out. Offsets are in the shown text (one `*` per char when masked).
pub struct Layout {
    pub key: LayoutKey,
    pub lines: Vec<Line>,
    pub line_height: Pixels,
    /// The width of one `m`: the size of the rect an input method asks for.
    pub em: Pixels,
    pub rows: usize,
    /// The widest line, unwrapped.
    pub width: Pixels,
    /// How far right of the box's left edge the text starts (`LayoutKey::inset`).
    pub inset: Pixels,
}

/// How far right of the box's left edge the text of an area or a full box starts: one descent of
/// the font, where Zed's editor drew it (measured on the B1 shots: 3.85 px for the 14 px UI font).
/// A one-line box starts at the edge.
pub fn text_inset(font: &Font, font_size: Pixels, window: &Window) -> Pixels {
    let text_system = window.text_system();
    text_system
        .descent(text_system.resolve_font(font), font_size)
        .abs()
}

/// The width the text of a box `width` wide wraps at: the inset and two `m` widths less, where
/// Zed's editor wrapped (measured on the B1 shots of the Start a task notes, the edit form and the
/// stage prompt).
pub fn wrap_width(font: &Font, font_size: Pixels, width: Pixels, window: &Window) -> Pixels {
    let text_system = window.text_system();
    let em = text_system
        .em_width(text_system.resolve_font(font), font_size)
        .unwrap_or(font_size);
    (width - text_inset(font, font_size, window) - em * 2.).max(px(0.))
}

/// Shapes `text` as one row.
fn shape(
    text: &SharedString,
    runs: &[TextRun],
    font_size: Pixels,
    text_system: &WindowTextSystem,
) -> WrappedLine {
    text_system
        .shape_text(text.clone(), font_size, runs, None, None)
        .ok()
        .and_then(|lines| lines.into_iter().next())
        .unwrap_or_default()
}

#[cfg(test)]
thread_local! {
    /// How many lines `wrap_rows` wrapped on this thread, for tests.
    pub(super) static WRAPS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Where a line of `text` (tabs as spaces) wraps at `wrap` px, as Zed's editor wrapped it: by
/// the widths of the chars in `font`, and a row after a wrap indented as far as the line. One
/// item per row: the offset it starts at and its indent in spaces; the first is (0, 0).
pub fn wrap_rows(
    text: &str,
    font: &Font,
    font_size: Pixels,
    wrap: Option<Pixels>,
    window: &Window,
) -> Vec<(usize, u32)> {
    #[cfg(test)]
    WRAPS.with(|wraps| wraps.set(wraps.get() + 1));
    let mut rows = vec![(0, 0)];
    if let Some(wrap) = wrap {
        let mut wrapper = window.text_system().line_wrapper(font.clone(), font_size);
        let fragments = [LineFragment::text(text)];
        rows.extend(
            wrapper
                .wrap_line(&fragments, wrap, IndentAdjustment::SameIndent)
                .map(|boundary| (boundary.ix, boundary.next_indent)),
        );
    }
    rows
}

/// The rows of a line of `text` (tabs as spaces), each to be shaped alone.
fn shape_rows(text: String, runs: &[TextRun], key: &LayoutKey, window: &Window) -> Rows {
    let starts = wrap_rows(&text, &key.font, key.font_size, key.width, window);
    let space = if starts.len() > 1 {
        let text_system = window.text_system();
        text_system.layout_width(text_system.resolve_font(&key.font), key.font_size, ' ')
    } else {
        px(0.)
    };
    let rows = starts
        .iter()
        .enumerate()
        .map(|(index, &(start, indent))| {
            let end = starts.get(index + 1).map_or(text.len(), |next| next.0);
            Row {
                shown: start,
                chars: text[start..end].chars().count(),
                indent: space * indent as f32,
                shaped: OnceCell::new(),
            }
        })
        .collect();
    Rows {
        text,
        runs: runs.to_vec(),
        rows,
        font_size: key.font_size,
        text_system: window.text_system().clone(),
    }
}

/// The part of `runs` that covers `range` of the text.
fn runs_in(runs: &[TextRun], range: Range<usize>) -> Vec<TextRun> {
    let mut out = Vec::new();
    RunCursor::new(runs).take_into(range, &mut out);
    out
}

/// Gives the parts of `runs` for ranges of the text that come one after the other, without
/// walking the runs again from the start for each: a layout cuts them line by line.
struct RunCursor<'a> {
    runs: &'a [TextRun],
    /// The first run that may still meet a later range, and the offset it starts at.
    index: usize,
    at: usize,
}

impl<'a> RunCursor<'a> {
    fn new(runs: &'a [TextRun]) -> Self {
        Self {
            runs,
            index: 0,
            at: 0,
        }
    }

    /// Puts in `out` (cleared, so a layout reuses one buffer for all its lines) the part of the
    /// runs that covers `range`, which starts at or after the last one's start.
    fn take_into(&mut self, range: Range<usize>, out: &mut Vec<TextRun>) {
        while let Some(run) = self.runs.get(self.index)
            && self.at + run.len <= range.start
        {
            self.at += run.len;
            self.index += 1;
        }
        out.clear();
        let mut at = self.at;
        for run in &self.runs[self.index.min(self.runs.len())..] {
            if at >= range.end {
                break;
            }
            let (start, end) = (at.max(range.start), (at + run.len).min(range.end));
            at += run.len;
            if start < end {
                out.push(TextRun {
                    len: end - start,
                    ..run.clone()
                });
            }
        }
    }
}

impl Layout {
    /// Lays `text` out line by line. A line of `previous` with the same text and runs, at the same
    /// font, size, colour and width, is taken as it is: a key shapes only the lines it changed,
    /// also the coloured ones, whose shapes gpui keeps only from one frame to the next.
    pub fn build(
        key: LayoutKey,
        text: &SharedString,
        runs: &[TextRun],
        previous: Option<&Layout>,
        window: &Window,
    ) -> Self {
        let old_lines: &[Line] = previous
            .filter(|previous| {
                let old = &previous.key;
                (old.width, &old.font, old.font_size, old.color)
                    == (key.width, &key.font, key.font_size, key.color)
            })
            .map_or(&[], |previous| previous.lines.as_slice());
        // After an edit, the lines below it moved by the lines it added or removed.
        let shift = old_lines.len() as isize - (text.matches('\n').count() + 1) as isize;
        // Every old line by its text, made only when a line is not found at its place.
        let mut ready: Option<HashMap<&str, Vec<&Rc<Rows>>>> = None;
        let mut lines = Vec::new();
        let (mut start, mut row) = (0, 0);
        let (mut cursor, mut line_runs) = (RunCursor::new(runs), Vec::new());
        let blocks = key.blocks.clone();
        let above = |index: usize| blocks.as_ref().and_then(|b| b.get(index).copied());
        for (index, source) in text.split('\n').enumerate() {
            let len = source.len();
            row += above(index).unwrap_or(0);
            cursor.take_into(start..start + len, &mut line_runs);
            let same = |old: &&Line| *old.rows.text == *source && old.rows.runs == line_runs;
            // A line with no tab shows as it is: compare it with the old line at its place.
            let kept = if source.contains('\t') {
                None
            } else {
                old_lines.get(index).filter(same).or_else(|| {
                    let moved = index.checked_add_signed(shift)?;
                    old_lines.get(moved).filter(same)
                })
            };
            let line = if let Some(old) = kept {
                Line {
                    start,
                    len,
                    first_row: row,
                    row_starts: old.row_starts.clone(),
                    rows: old.rows.clone(),
                    tabs: Vec::new(),
                }
            } else {
                let (shown, line_runs, tabs) = expand_tabs(source, &line_runs);
                let ready = ready.get_or_insert_with(|| {
                    let mut ready: HashMap<&str, Vec<&Rc<Rows>>> = HashMap::new();
                    for line in old_lines {
                        ready
                            .entry(line.rows.text.as_str())
                            .or_default()
                            .push(&line.rows);
                    }
                    ready
                });
                let cached = ready.get(shown.as_str()).and_then(|found| {
                    found
                        .iter()
                        .find(|rows| rows.runs == line_runs)
                        .map(|rows| (*rows).clone())
                });
                let rows =
                    cached.unwrap_or_else(|| Rc::new(shape_rows(shown, &line_runs, &key, window)));
                let mut line = Line {
                    start,
                    len,
                    first_row: row,
                    row_starts: Rc::from([]),
                    rows,
                    tabs,
                };
                // A row that starts inside a tab's spaces keeps the tab on the row above.
                let starts = line.rows.rows.iter().map(|at| line.local(at.shown, true));
                line.row_starts = starts.collect();
                line
            };
            let count = line.row_starts.len();
            lines.push(line);
            row += count;
            start += len + 1;
        }
        row += above(lines.len()).unwrap_or(0);
        // The text is as wide as its row with the most chars, as Zed's editor measured it: only
        // that row is shaped for it (the first of equal ones).
        let mut longest: Option<(&Line, usize, usize)> = None;
        for line in &lines {
            for (index, shown) in line.rows.rows.iter().enumerate() {
                if longest.is_none_or(|(_, _, chars)| shown.chars > chars) {
                    longest = Some((line, index, shown.chars));
                }
            }
        }
        let width = longest
            .and_then(|(line, index, _)| line.rows.row(index))
            .map_or(px(0.), |(row, shaped)| {
                row.indent + shaped.unwrapped_layout.width
            });
        let text_system = window.text_system();
        let em = text_system
            .em_advance(text_system.resolve_font(&key.font), key.font_size)
            .unwrap_or(key.font_size);
        Layout {
            line_height: key.line_height,
            em,
            // The placeholder may wrap, but an empty box is one row, as in Zed's editor.
            rows: if key.placeholder.is_some() {
                1
            } else {
                row.max(1)
            },
            width,
            inset: key.inset,
            lines,
            key,
        }
    }

    /// The first shown row of the empty rows above line `index`, or of the rows below the last
    /// line for `index == lines.len()`.
    pub fn blocks_top(&self, index: usize) -> usize {
        let above = self
            .key
            .blocks
            .as_ref()
            .and_then(|blocks| blocks.get(index).copied())
            .unwrap_or(0);
        let row = self
            .lines
            .get(index)
            .map_or(self.rows, |line| line.first_row);
        row.saturating_sub(above)
    }

    fn line_at(&self, at: usize) -> Option<&Line> {
        let index = self.lines.partition_point(|line| line.start <= at);
        self.lines.get(index.checked_sub(1)?)
    }

    /// The index of the line that holds shown row `row`.
    pub fn line_index_of_row(&self, row: usize) -> usize {
        self.lines
            .partition_point(|line| line.first_row <= row)
            .saturating_sub(1)
    }

    /// The line that holds shown row `row`, and the row inside it.
    pub(super) fn line_of_row(&self, row: usize) -> Option<(&Line, usize)> {
        let index = self.lines.partition_point(|line| line.first_row <= row);
        let line = self.lines.get(index.checked_sub(1)?)?;
        let in_line = (row - line.first_row).min(line.row_starts.len() - 1);
        Some((line, in_line))
    }

    /// The row inside `line` of `local`. At a wrap point the caret shows on the later row, or on
    /// the earlier one with `end_bias` (after End).
    fn row_in_line(line: &Line, local: usize, end_bias: bool) -> usize {
        let row = line
            .row_starts
            .partition_point(|&start| start <= local)
            .saturating_sub(1);
        if end_bias && row > 0 && line.row_starts.get(row) == Some(&local) {
            row - 1
        } else {
            row
        }
    }

    /// The top left of the char at `at`, from the top left of the text.
    pub fn position(&self, at: usize, end_bias: bool) -> Point<Pixels> {
        let Some(line) = self.line_at(at) else {
            return Point::default();
        };
        let local = (at - line.start).min(line.len);
        let row = Self::row_in_line(line, local, end_bias);
        point(
            line.x_in_row(row, local),
            self.line_height * (line.first_row + row) as f32,
        )
    }

    pub fn row_of(&self, at: usize, end_bias: bool) -> usize {
        self.line_at(at).map_or(0, |line| {
            let local = (at - line.start).min(line.len);
            line.first_row + Self::row_in_line(line, local, end_bias)
        })
    }

    /// The offsets a row covers, its line end or wrap point included.
    pub fn row_range(&self, row: usize) -> Range<usize> {
        self.line_of_row(row).map_or(0..0, |(line, in_line)| {
            let start = line.row_starts.get(in_line).copied().unwrap_or(0);
            line.start + start..line.start + line.row_end(in_line)
        })
    }

    /// The offset nearest to `x` in `row`, and whether it is the wrap point at the row's end.
    pub fn index_in_row(&self, row: usize, x: Pixels) -> (usize, bool) {
        let Some((line, in_line)) = self.line_of_row(row) else {
            return (0, false);
        };
        let start = line.row_starts.get(in_line).copied().unwrap_or(0);
        let end = line.row_end(in_line);
        let shown = line.rows.row(in_line).map_or(0, |(row, shaped)| {
            row.shown + shaped.unwrapped_layout.closest_index_for_x(x - row.indent)
        });
        let local = line.local(shown, false).clamp(start, end);
        let wraps = in_line + 1 < line.row_starts.len();
        (line.start + local, wraps && local == end)
    }

    /// The offset nearest to `at`, from the top left of the text.
    pub fn index_at(&self, at: Point<Pixels>) -> (usize, bool) {
        let row = if at.y < px(0.) {
            0
        } else {
            ((at.y / self.line_height) as usize).min(self.rows - 1)
        };
        self.index_in_row(row, at.x)
    }

    /// The rects of a highlighted `range`, one per row it covers, from the top left of the text.
    /// A row the range runs past (its line end or wrap point) gets a bit more on the right.
    pub fn selection_rects(&self, range: Range<usize>) -> Vec<Bounds<Pixels>> {
        self.selection_rects_in(range, 0..usize::MAX)
    }

    /// `selection_rects` only on the shown rows `rows`.
    pub fn selection_rects_in(
        &self,
        range: Range<usize>,
        rows: Range<usize>,
    ) -> Vec<Bounds<Pixels>> {
        self.rects_past(range, self.line_height * SELECTION_ROUNDING, rows)
    }

    /// `selection_rects` with `overshoot` more on the right of a row the range runs past, only on
    /// the shown rows `rows`. Each row still sees the whole `range`, so its rect is the same as
    /// without the window.
    pub fn rects_past(
        &self,
        range: Range<usize>,
        overshoot: Pixels,
        rows: Range<usize>,
    ) -> Vec<Bounds<Pixels>> {
        let mut rects = Vec::new();
        if range.is_empty() || rows.is_empty() {
            return rects;
        }
        // Lines are in order of offset and of row: skip those before the range or the rows.
        let skip = self
            .lines
            .partition_point(|line| line.start + line.len < range.start);
        let from = self.line_index_of_row(rows.start).max(skip);
        for line in self.lines.get(from..).unwrap_or_default() {
            if line.first_row >= rows.end || line.start > range.end {
                break;
            }
            for row in 0..line.row_starts.len() {
                if !rows.contains(&(line.first_row + row)) {
                    continue;
                }
                let row_start = line.start + line.row_starts[row];
                let row_end = line.start + line.row_end(row);
                let start = range.start.max(row_start);
                let end = range.end.min(row_end);
                let past = range.end > row_end;
                if start > end || (start == end && !past) {
                    continue;
                }
                // A range that goes on from the row above covers the row's indent too, as in
                // Zed's editor (S3.3 shot `workspace-docs-edit-selection` at 1024 x 720).
                let left = if range.start < row_start {
                    px(0.)
                } else {
                    line.x_in_row(row, start - line.start)
                };
                let mut right = line.x_in_row(row, end - line.start);
                if past {
                    right += overshoot;
                }
                if right <= left {
                    continue;
                }
                rects.push(Bounds::new(
                    point(left, self.line_height * (line.first_row + row) as f32),
                    size(right - left, self.line_height),
                ));
            }
        }
        rects
    }

    /// The offsets that shown rows `rows` cover; a row past the end gives the text's end.
    pub fn rows_span(&self, rows: Range<usize>) -> Range<usize> {
        let start = self.row_range(rows.start).start;
        let end = self.row_range(rows.end.saturating_sub(1)).end;
        start..end.max(start)
    }

    /// The shown rows that meet `bounds` when the text starts at `origin`, with slack: a
    /// selection corner on a row depends on the rows next to it, and the division may land a
    /// hair off a row edge.
    pub fn rows_near(&self, bounds: Bounds<Pixels>, origin: Point<Pixels>) -> Range<usize> {
        let lh = self.line_height;
        if lh <= px(0.) {
            return 0..usize::MAX;
        }
        let first = ((bounds.top() - origin.y) / lh).floor().max(0.) as usize;
        let end = ((bounds.bottom() - origin.y) / lh).ceil().max(0.) as usize;
        first.saturating_sub(2)..end.saturating_add(2)
    }

    /// The top left and the width of each whitespace char of the selection `range` of `text`
    /// (line ends left out), with the char.
    pub fn whitespace(
        &self,
        text: &str,
        range: Range<usize>,
    ) -> Vec<(Point<Pixels>, Pixels, char)> {
        let Some(selected) = text.get(range.clone()) else {
            return Vec::new();
        };
        selected
            .char_indices()
            .filter(|(_, c)| c.is_whitespace() && *c != '\n')
            .map(|(at, c)| {
                let at = range.start + at;
                let width = self.line_at(at).map_or(px(0.), |line| {
                    let local = at - line.start;
                    let row = Self::row_in_line(line, local, false);
                    line.x_in_row(row, local + c.len_utf8()) - line.x_in_row(row, local)
                });
                (self.position(at, false), width, c)
            })
            .collect()
    }
}

/// The selection as filled outlines with round corners, one per run of rows that touch (a first
/// row that starts right of the end of the next is a run of its own). `rects` come one per row,
/// top to bottom, from `Layout::selection_rects`.
fn selection_paths(rects: &[Bounds<Pixels>], radius: Pixels) -> Vec<Path<Pixels>> {
    let mut paths = Vec::new();
    let mut run: Vec<(Pixels, Pixels)> = Vec::new();
    let mut top = px(0.);
    for (index, rect) in rects.iter().enumerate() {
        let touches = index.checked_sub(1).is_some_and(|before| {
            let before = &rects[before];
            (rect.top() - before.bottom()).abs() < px(0.5)
                && rect.left() <= before.right()
                && rect.right() >= before.left()
        });
        if !touches && !run.is_empty() {
            paths.extend(rounded_path(&run, top, rect.size.height, radius));
            run.clear();
        }
        if run.is_empty() {
            top = rect.top();
        }
        run.push((rect.left(), rect.right()));
    }
    if let Some(rect) = rects.last() {
        paths.extend(rounded_path(&run, top, rect.size.height, radius));
    }
    paths
}

pub struct TextElement {
    input: Entity<TextInput>,
}

impl TextElement {
    pub fn new(input: Entity<TextInput>) -> Self {
        Self { input }
    }
}

impl IntoElement for TextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextElement {
    type RequestLayoutState = ();
    type PrepaintState = Option<(Rc<Layout>, Option<Pointer>)>;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let input = self.input.read(cx);
        let mode = input.mode;
        let style = input.text_style(cx);
        let rem = input.rem_size(&style, window);
        let line_height = style.line_height_in_pixels(rem);
        let mut layout = Style::default();
        layout.size.width = relative(1.).into();
        let id = match mode {
            Mode::OneLine => {
                layout.size.height = line_height.into();
                window.request_layout(layout, [], cx)
            }
            Mode::Full { .. } => {
                layout.size.height = relative(1.).into();
                window.request_layout(layout, [], cx)
            }
            Mode::Area { min, max } => {
                let font_size = style.font_size.to_pixels(rem);
                let font = style.font();
                let (min, max) = (min.max(1), max.max(min).max(1));
                let entity = self.input.clone();
                window.request_measured_layout(layout, move |known, available, window, cx| {
                    let width = known.width.or(match available.width {
                        AvailableSpace::Definite(width) => Some(width),
                        _ => None,
                    });
                    let wrap = width.map(|width| wrap_width(&font, font_size, width, window));
                    let input = entity.read(cx);
                    // The last layout at this text and wrap width has the rows already: wrap
                    // the text again only when it is stale.
                    let cached = input.laid.as_ref().filter(|laid| {
                        laid.key.version == input.model.version()
                            && laid.key.masked == input.masked
                            && laid.key.width == wrap
                            && laid.key.font == font
                            && laid.key.font_size == font_size
                    });
                    let rows: usize = match cached {
                        Some(laid) => laid.rows,
                        None => {
                            let (text, placeholder) = input.display();
                            // The placeholder does not grow the box: it is cut to one row, as
                            // in Zed.
                            if placeholder {
                                1
                            } else {
                                let (text, _, _) = expand_tabs(&text, &[]);
                                text.split('\n')
                                    .map(|line| {
                                        wrap_rows(line, &font, font_size, wrap, window).len()
                                    })
                                    .sum()
                            }
                        }
                    };
                    size(
                        width.unwrap_or_default(),
                        line_height * rows.clamp(min, max) as f32,
                    )
                })
            }
        };
        (id, ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let laid = self.input.update(cx, |input, cx| {
            input.last_bounds = Some(bounds);
            let laid = input.layout(window, cx);
            if std::mem::take(&mut input.autoscroll) {
                input.scroll_to_head(&laid, bounds);
            }
            let before = input.scroll;
            input.clamp_scroll(&laid, bounds);
            input.sync_peer(before, cx);
            // A scroll may move the lines a code view colours: lay out again (only then).
            input.layout(window, cx)
        });
        let pointer = Pointer::insert(self.input.read(cx), &laid, bounds, window);
        Some((laid, pointer))
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        laid: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some((laid, pointer)) = laid.take() else {
            return;
        };
        let input = self.input.read(cx);
        let focus = input.focus.clone();
        let focused = focus.is_focused(window);
        let origin = bounds.origin + point(laid.inset, px(0.)) - input.scroll;
        let selection = input.model.selection().range();
        let selection = input.to_display(selection.start)..input.to_display(selection.end);
        let head = input.to_display(input.model.head());
        let end_bias = input.end_bias;
        let caret = input.caret_shows(focused, cx);
        let drop_at = input.drop_at.map(|at| input.to_display(at));
        let (full, current_line) = match input.mode {
            Mode::Full { current_line } => (true, current_line),
            _ => (false, false),
        };
        let code = input.code_paint(&laid, bounds, origin, cx);
        // Only the rows near the box: the others draw nothing, and measuring them shapes them.
        let near = laid.rows_near(bounds, origin);
        let span = laid.rows_span(near.clone());
        // Spaces and tabs inside the selection of a full box show as dots and arrows; in a code
        // view all of them while Show whitespace is on.
        let wanted = if full && code.all_whitespace {
            Some(span)
        } else if full && !selection.is_empty() {
            Some(selection.start.max(span.start)..selection.end.min(span.end))
        } else {
            None
        };
        let whitespace = match wanted.filter(|range| range.start < range.end) {
            Some(range) => laid.whitespace(&input.display_text(), range),
            None => Vec::new(),
        };
        let colors = cx.theme().colors();
        let (background, active_line) = (
            colors.editor_background,
            colors.editor_active_line_background,
        );
        // A read-only box draws in the grey player colours, caret and selection, as Zed's editor
        // did (B1 shot `workspace-docs-source-focused`, B2 shot `changes-diff-focused`).
        let player = if input.read_only {
            cx.theme().players().read_only()
        } else {
            cx.theme().players().local()
        };
        let selection_color = if focused {
            player.selection
        } else {
            player.selection.opacity(0.5)
        };
        let cursor = player.cursor;
        let invisible = colors.editor_invisible;
        let line_height = laid.line_height;
        let (guide, active_guide, highlight) = (
            colors.editor_indent_guide,
            colors.editor_indent_guide_active,
            colors.editor_document_highlight_bracket_background,
        );
        let (search, search_active) = (
            colors.search_match_background,
            colors.search_active_match_background,
        );
        let (track, track_border, thumb_border) = (
            colors.scrollbar_track_background,
            colors.scrollbar_track_border,
            colors.scrollbar_thumb_border,
        );
        let thumb_color = |hovered: bool, dragged: bool| {
            if dragged {
                colors.scrollbar_thumb_active_background
            } else if hovered {
                colors.scrollbar_thumb_hover_background
            } else {
                colors.scrollbar_thumb_background
            }
        };
        let bar = code
            .bar
            .as_ref()
            .map(|(bar, hovered, dragged)| (bar, thumb_color(*hovered, *dragged)));
        let hbar = code
            .hbar
            .as_ref()
            .map(|(bar, hovered, dragged)| (bar, thumb_color(*hovered, *dragged)));

        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        // A drag goes on outside the box, and a release there ends it.
        let moved = self.input.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble {
                moved.update(cx, |input, cx| input.mouse_move(event, window, cx));
            }
        });
        let released = self.input.clone();
        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble && event.button == MouseButton::Left {
                released.update(cx, |input, cx| input.mouse_up(event, window, cx));
            }
        });

        // Two empty layers put the text two draw orders higher, as deep as Zed's editor drew.
        // gpui orders a shadow by its bounds without the blur, and paints a div's border after
        // its children as strips, so a menu's shadow tail only covers a nearby border strip
        // when what lies under the menu is this deep (start-task-branch-menu: the tail on the
        // project select's top border).
        window.paint_layer(bounds, |window| window.paint_layer(bounds, |_| {}));
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            if full {
                window.paint_quad(fill(bounds, background));
            }
            // Every row of the caret's line, wrapped rows too, as Zed's editor did (B2 shot
            // `changes-diff-deleted-only` at 800x560).
            if current_line && selection.is_empty() {
                let row = laid.row_of(head, end_bias);
                let line = laid.line_of_row(row).map(|(line, _)| line);
                let (first, rows) = line.map_or((row, 1), |line| {
                    (line.first_row, line.row_starts.len().max(1))
                });
                window.paint_quad(fill(
                    Bounds::new(
                        point(bounds.left(), origin.y + line_height * first as f32),
                        size(bounds.size.width, line_height * rows as f32),
                    ),
                    active_line,
                ));
            }
            // A diff side's hunks lie over the line of the caret (B2 `changes-diff-deleted-only`).
            if let Some(diff) = &code.diff {
                for (rect, color) in &diff.backs {
                    window.paint_quad(fill(*rect, *color));
                }
            }
            // A diff side's gutter has no background to cover the text layer scrolled under it.
            let text_mask = code.diff.as_ref().map(|diff| diff.text_mask(bounds));
            window.with_content_mask(text_mask, |window| {
                if let Some(diff) = &code.diff {
                    for (rect, color) in &diff.words {
                        window.paint_quad(fill(*rect, *color));
                    }
                }
                for (bounds, active) in &code.guides {
                    // Snapped to device pixels, as Zed's editor drew its guides.
                    let bounds = window.pixel_snap_bounds(*bounds);
                    window.paint_quad(fill(bounds, if *active { active_guide } else { guide }));
                }
                for bounds in &code.backgrounds {
                    window.paint_quad(fill(*bounds, highlight));
                }
                // As paths, as Zed's editor drew its highlighted ranges: a part-covered pixel at an
                // edge gets part of the colour.
                for (bounds, active) in &code.search {
                    let color = if *active { search_active } else { search };
                    for path in selection_paths(&[*bounds], px(0.)) {
                        window.paint_path(path, color);
                    }
                }
                let rects: Vec<_> = laid
                    // The selection runs twice its corner radius past a line end, as in Zed's
                    // editor.
                    .rects_past(
                        selection.clone(),
                        line_height * SELECTION_ROUNDING * 2.,
                        near.clone(),
                    )
                    .into_iter()
                    .map(|rect| Bounds::new(rect.origin + origin, rect.size))
                    .collect();
                for path in selection_paths(&rects, line_height * SELECTION_ROUNDING) {
                    window.paint_path(path, selection_color);
                }
                if !whitespace.is_empty() {
                    let shape = |symbol: &'static str, window: &mut Window| {
                        let run = TextRun {
                            len: symbol.len(),
                            font: laid.key.font.clone(),
                            color: invisible,
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        };
                        window.text_system().shape_line(
                            symbol.into(),
                            laid.key.font_size / 2.,
                            &[run],
                            None,
                        )
                    };
                    let (dot, arrow) = (shape("\u{2022}", window), shape("\u{2192}", window));
                    for (at, width, c) in &whitespace {
                        let top = origin.y + at.y;
                        if top + line_height < bounds.top() || top > bounds.bottom() {
                            continue;
                        }
                        let symbol = if *c == '\t' { &arrow } else { &dot };
                        // The symbol sits in the middle of the char's own width.
                        let inset = (*width - symbol.width) / 2.;
                        symbol
                            .paint(
                                point(origin.x + at.x + inset, top),
                                line_height,
                                TextAlign::Left,
                                None,
                                window,
                                cx,
                            )
                            .ok();
                    }
                }
                for line in &laid.lines {
                    let top = origin.y + line_height * line.first_row as f32;
                    let bottom = top + line_height * line.row_starts.len() as f32;
                    if bottom < bounds.top() || top > bounds.bottom() {
                        continue;
                    }
                    for row in 0..line.row_starts.len() {
                        let top = top + line_height * row as f32;
                        if top + line_height < bounds.top() || top > bounds.bottom() {
                            continue;
                        }
                        line.paint_row(row, point(origin.x, top), line_height, window, cx);
                    }
                }
                let carets = caret.then_some((head, end_bias)).into_iter();
                for (at, bias) in carets.chain(drop_at.map(|at| (at, false))) {
                    let at = laid.position(at, bias);
                    window.paint_quad(fill(
                        Bounds::new(origin + at, size(CARET_WIDTH, line_height)),
                        cursor,
                    ));
                }
            });
            if let Some(gutter) = &code.gutter {
                super::code_gutter::paint_gutter(gutter, &laid, window, cx);
            }
            if let Some(diff) = &code.diff {
                for (rect, color) in &diff.strips {
                    super::diff_paint::paint_hollow(*rect, *color, background, window);
                }
            }
            // The scrollbar's track is not part of a pinned row for the pointer.
            let right = bar.map_or(bounds.right(), |(bar, _)| bar.track.left());
            let hovered = pointer.and_then(|pointer| {
                let tops = super::code_sticky::tops(&code.sticky);
                pointer.paint(tops, (bounds.left(), right), line_height, window)
            });
            #[cfg(test)]
            self.input.read(cx).sticky_hovered.set(hovered);
            super::code_gutter::paint_sticky(&code, &laid, bounds, origin, hovered, window, cx);
            if let Some((bar, thumb)) = &bar {
                let edge = |rect: &Bounds<Pixels>| {
                    Bounds::new(rect.origin, size(px(1.), rect.size.height))
                };
                window.paint_quad(fill(bar.track, track));
                window.paint_quad(fill(edge(&bar.track), track_border));
                for (marker, color) in &code.markers {
                    window.paint_quad(fill(*marker, *color));
                }
                window.paint_quad(fill(bar.thumb, *thumb));
                window.paint_quad(fill(edge(&bar.thumb), thumb_border));
            }
            // The horizontal bar has its border line on top; its thumb has none, as in Zed's
            // editor (B1 shot `workspace-editor-scrollbar`).
            if let Some((bar, thumb)) = &hbar {
                let edge =
                    |rect: &Bounds<Pixels>| Bounds::new(rect.origin, size(rect.size.width, px(1.)));
                window.paint_quad(fill(bar.track, track));
                window.paint_quad(fill(edge(&bar.track), track_border));
                window.paint_quad(fill(bar.thumb, *thumb));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use gpui::font;

    use super::*;

    /// The parts of `runs` over `range`, walked from the start: what the cursor must give.
    fn from_the_start(runs: &[TextRun], range: Range<usize>) -> Vec<(usize, f32)> {
        let mut out = Vec::new();
        let mut at = 0;
        for run in runs {
            let (start, end) = (at.max(range.start), (at + run.len).min(range.end));
            at += run.len;
            if start < end {
                out.push((end - start, run.color.h));
            }
        }
        out
    }

    #[test]
    fn the_run_cursor_cuts_lines_as_a_walk_from_the_start_does() {
        let run = |len: usize, hue: f32| TextRun {
            len,
            font: font("Mono"),
            color: Hsla {
                h: hue,
                s: 0.,
                l: 0.,
                a: 1.,
            },
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = vec![
            run(3, 0.1),
            run(0, 0.2),
            run(5, 0.3),
            run(2, 0.4),
            run(4, 0.5),
        ];
        let lines = [0..2, 3..3, 4..9, 10..10, 11..14, 15..15];
        // One buffer for every line, as a layout uses it: no run of a line before may stay in it.
        let (mut cursor, mut out) = (RunCursor::new(&runs), Vec::new());
        for range in lines {
            cursor.take_into(range.clone(), &mut out);
            let cut: Vec<(usize, f32)> = out.iter().map(|run| (run.len, run.color.h)).collect();
            assert_eq!(cut, from_the_start(&runs, range.clone()), "{range:?}");
        }
    }

    /// A key lays out only the lines it changed: the others keep their shaped rows, also below
    /// an added line.
    #[gpui::test]
    fn a_layout_keeps_the_rows_of_the_lines_an_edit_did_not_touch(cx: &mut gpui::TestAppContext) {
        let cx = cx.add_empty_window();
        cx.update(|window, _| {
            let key = |version| LayoutKey {
                version,
                width: None,
                font: font("Mono"),
                font_size: px(14.),
                line_height: px(20.),
                color: Hsla::default(),
                masked: false,
                marked: None,
                placeholder: None,
                inset: px(0.),
                colors: 0,
                gutter: px(0.),
                gutter_pad: px(0.),
                view: px(400.),
                lines: None,
                heads: Vec::new(),
                marks: Vec::new(),
                blocks: None,
            };
            let run = |len| TextRun {
                len,
                font: font("Mono"),
                color: Hsla::default(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let before = SharedString::from("a\nb\nc");
            let old = Layout::build(key(1), &before, &[run(before.len())], None, window);
            let after = SharedString::from("a\nx\nb\nc");
            let new = Layout::build(key(2), &after, &[run(after.len())], Some(&old), window);
            // A kept line shares its row starts too, not a copy of them.
            let kept = |old_line: usize, new_line: usize| {
                let (old, new) = (&old.lines[old_line], &new.lines[new_line]);
                Rc::ptr_eq(&old.rows, &new.rows) && Rc::ptr_eq(&old.row_starts, &new.row_starts)
            };
            assert!(kept(0, 0) && kept(1, 2) && kept(2, 3));
            assert_eq!(new.lines[1].rows.text, "x");
        });
    }

    /// A long text shapes only the row that gives its width, then each row it measures or draws.
    #[gpui::test]
    fn a_layout_shapes_a_row_only_when_it_is_asked_for(cx: &mut gpui::TestAppContext) {
        let cx = cx.add_empty_window();
        cx.update(|window, _| {
            let key = LayoutKey {
                version: 1,
                width: None,
                font: font("Mono"),
                font_size: px(14.),
                line_height: px(20.),
                color: Hsla::default(),
                masked: false,
                marked: None,
                placeholder: None,
                inset: px(0.),
                colors: 0,
                gutter: px(0.),
                gutter_pad: px(0.),
                view: px(400.),
                lines: None,
                heads: Vec::new(),
                marks: Vec::new(),
                blocks: None,
            };
            let lines: Vec<String> = (0..1000).map(|n| format!("line {n}")).collect();
            let text = SharedString::from(lines.join("\n"));
            let run = TextRun {
                len: text.len(),
                font: font("Mono"),
                color: Hsla::default(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let laid = Layout::build(key, &text, &[run], None, window);
            let shaped = |laid: &Layout| -> Vec<usize> {
                (0..laid.lines.len())
                    .filter(|&index| laid.lines[index].rows.rows[0].shaped.get().is_some())
                    .collect()
            };
            // "line 100" is the first line with the most chars.
            assert_eq!(shaped(&laid), vec![100]);
            laid.position(laid.lines[500].start + 3, false);
            assert_eq!(shaped(&laid), vec![100, 500]);
        });
    }

    /// 1,000 lines `line N`, one row each, 20 px high, not wrapped.
    fn long_layout(window: &Window) -> (Layout, SharedString) {
        let key = LayoutKey {
            version: 1,
            width: None,
            font: font("Mono"),
            font_size: px(14.),
            line_height: px(20.),
            color: Hsla::default(),
            masked: false,
            marked: None,
            placeholder: None,
            inset: px(0.),
            colors: 0,
            gutter: px(0.),
            gutter_pad: px(0.),
            view: px(400.),
            lines: None,
            heads: Vec::new(),
            marks: Vec::new(),
            blocks: None,
        };
        let lines: Vec<String> = (0..1000).map(|n| format!("line {n}")).collect();
        let text = SharedString::from(lines.join("\n"));
        let run = TextRun {
            len: text.len(),
            font: font("Mono"),
            color: Hsla::default(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        (Layout::build(key, &text, &[run], None, window), text)
    }

    /// The lines whose first row is shaped.
    fn shaped_lines(laid: &Layout) -> Vec<usize> {
        (0..laid.lines.len())
            .filter(|&index| laid.lines[index].rows.rows[0].shaped.get().is_some())
            .collect()
    }

    /// P-text-1: a selection of the whole text measures only the rows of the window, and their
    /// rects are the ones the whole selection has on those rows.
    #[gpui::test]
    fn a_selection_of_all_shapes_only_the_rows_asked_for(cx: &mut gpui::TestAppContext) {
        let cx = cx.add_empty_window();
        cx.update(|window, _| {
            let (laid, text) = long_layout(window);
            let rects = laid.rects_past(0..text.len(), px(3.), 500..510);
            let mut want = vec![100];
            want.extend(500..510);
            assert_eq!(shaped_lines(&laid), want);
            let (whole, _) = long_layout(window);
            let all: Vec<Bounds<Pixels>> = whole
                .rects_past(0..text.len(), px(3.), 0..usize::MAX)
                .into_iter()
                .filter(|rect| rect.top() >= px(10000.) && rect.top() < px(10200.))
                .collect();
            assert_eq!(rects.len(), 10);
            assert_eq!(rects, all);
            // A range that starts and ends inside the window gives the same rects too.
            let inside = laid.lines[503].start + 2..laid.lines[506].start + 3;
            let near: Vec<Bounds<Pixels>> = whole
                .rects_past(inside.clone(), px(3.), 0..usize::MAX)
                .into_iter()
                .filter(|rect| rect.top() >= px(10000.) && rect.top() < px(10200.))
                .collect();
            assert_eq!(laid.rects_past(inside, px(3.), 500..510), near);
        });
    }

    #[gpui::test]
    fn rows_span_covers_the_rows_asked_for(cx: &mut gpui::TestAppContext) {
        let cx = cx.add_empty_window();
        cx.update(|window, _| {
            let (laid, text) = long_layout(window);
            assert_eq!(
                laid.rows_span(10..12),
                laid.lines[10].start..laid.lines[11].start + laid.lines[11].len
            );
            assert_eq!(laid.rows_span(990..2000), laid.lines[990].start..text.len());
            // Rows 5 to 14 meet a box 100 to 300 px down, with two rows of slack each side.
            let bounds = Bounds::new(point(px(0.), px(100.)), size(px(50.), px(200.)));
            assert_eq!(laid.rows_near(bounds, point(px(0.), px(0.))), 3..17);
        });
    }
}
