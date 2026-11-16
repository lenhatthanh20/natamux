//! The element of the own text input (plan 6.3.3, 6.3.4): it lays the text out with gpui's text
//! system and paints, in order, the background (full mode), the current line, the selection, the
//! whitespace dots inside it (full mode), the text with the input method's underline, and the
//! caret.

use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use gpui::{
    App, AvailableSpace, Bounds, ContentMask, DispatchPhase, Element, ElementId,
    ElementInputHandler, Entity, Font, GlobalElementId, Hsla, IndentAdjustment, InspectorElementId,
    IntoElement, LayoutId, LineFragment, MouseButton, MouseMoveEvent, MouseUpEvent, Path, Pixels,
    Point, SharedString, Style, TextAlign, TextRun, Window, WrappedLine, fill, point, px, relative,
    size,
};
use theme::ActiveTheme;

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
}

/// One line of the text (between two `\n`) and the rows it wraps into.
pub struct Line {
    /// The offset of its first char in the shown text.
    pub start: usize,
    pub len: usize,
    /// The row of the whole text its first row is.
    pub first_row: usize,
    /// The offsets in the line where its rows start; the first is 0.
    pub row_starts: Vec<usize>,
    /// The line with its tabs as spaces, in rows.
    rows: Rc<Rows>,
    tabs: Vec<Tab>,
    /// It is drawn in one run of the text colour, so a later layout may take it as it is.
    plain: bool,
}

/// A line with its tabs as spaces, and its rows, each shaped alone as Zed's editor shaped its
/// display rows: a glyph at a wrap point does not join the glyph across it.
struct Rows {
    text: String,
    rows: Vec<Row>,
}

struct Row {
    /// The offset of its first char in the line with its tabs as spaces.
    shown: usize,
    /// The room left of its text: the line's indent on a row after a wrap.
    indent: Pixels,
    shaped: WrappedLine,
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
        self.rows.rows.get(row).map_or(px(0.), |row| {
            let at = self.shown(local).saturating_sub(row.shown);
            row.indent + row.shaped.unwrapped_layout.x_for_index(at)
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
        if let Some(row) = self.rows.rows.get(row) {
            row.shaped
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

/// Shapes `text` in rows of at most `wrap` px.
pub fn shape(
    text: &SharedString,
    runs: &[TextRun],
    font_size: Pixels,
    wrap: Option<Pixels>,
    window: &Window,
) -> Vec<WrappedLine> {
    window
        .text_system()
        .shape_text(text.clone(), font_size, runs, wrap, None)
        .map(|lines| lines.into_iter().collect())
        .unwrap_or_default()
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

/// Shapes the rows of a line of `text` (tabs as spaces), each alone.
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
            let part = SharedString::from(text[start..end].to_owned());
            let shaped = shape(
                &part,
                &runs_in(runs, start..end),
                key.font_size,
                None,
                window,
            )
            .into_iter()
            .next()
            .unwrap_or_default();
            Row {
                shown: start,
                indent: space * indent as f32,
                shaped,
            }
        })
        .collect();
    Rows { text, rows }
}

/// The part of `runs` that covers `range` of the text.
fn runs_in(runs: &[TextRun], range: Range<usize>) -> Vec<TextRun> {
    let mut out = Vec::new();
    let mut at = 0;
    for run in runs {
        let (start, end) = (at.max(range.start), (at + run.len).min(range.end));
        at += run.len;
        if start < end {
            out.push(TextRun {
                len: end - start,
                ..run.clone()
            });
        }
    }
    out
}

impl Layout {
    /// Lays `text` out line by line. A line of `previous` with the same text, drawn in one run of
    /// the same font, size, colour and width, is taken as it is: a key shapes only the lines it
    /// changed.
    pub fn build(
        key: LayoutKey,
        text: &SharedString,
        runs: &[TextRun],
        previous: Option<&Layout>,
        window: &Window,
    ) -> Self {
        let mut ready: HashMap<&str, &Rc<Rows>> = HashMap::new();
        if let Some(previous) = previous.filter(|previous| {
            let old = &previous.key;
            (old.width, &old.font, old.font_size, old.color)
                == (key.width, &key.font, key.font_size, key.color)
        }) {
            for line in previous.lines.iter().filter(|line| line.plain) {
                ready.entry(line.rows.text.as_str()).or_insert(&line.rows);
            }
        }
        let mut lines = Vec::new();
        let (mut start, mut row, mut width) = (0, 0, px(0.));
        for source in text.split('\n') {
            let len = source.len();
            let line_runs = runs_in(runs, start..start + len);
            let plain = line_runs
                .iter()
                .all(|run| run.color == key.color && run.underline.is_none());
            let (shown, line_runs, tabs) = expand_tabs(source, &line_runs);
            let cached = ready
                .get(shown.as_str())
                .filter(|_| plain)
                .copied()
                .cloned();
            let rows =
                cached.unwrap_or_else(|| Rc::new(shape_rows(shown, &line_runs, &key, window)));
            width = rows.rows.iter().fold(width, |width, row| {
                width.max(row.indent + row.shaped.unwrapped_layout.width)
            });
            let shown_starts: Vec<usize> = rows.rows.iter().map(|row| row.shown).collect();
            let mut line = Line {
                start,
                len,
                first_row: row,
                row_starts: Vec::new(),
                rows,
                tabs,
                plain,
            };
            // A row that starts inside a tab's spaces keeps the tab on the row above.
            line.row_starts = shown_starts
                .iter()
                .map(|&at| line.local(at, true))
                .collect();
            let count = line.row_starts.len();
            lines.push(line);
            row += count;
            start += len + 1;
        }
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
        let shown = line.rows.rows.get(in_line).map_or(0, |row| {
            row.shown
                + row
                    .shaped
                    .unwrapped_layout
                    .closest_index_for_x(x - row.indent)
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
        self.rects_past(range, self.line_height * SELECTION_ROUNDING)
    }

    /// `selection_rects` with `overshoot` more on the right of a row the range runs past.
    pub fn rects_past(&self, range: Range<usize>, overshoot: Pixels) -> Vec<Bounds<Pixels>> {
        let mut rects = Vec::new();
        if range.is_empty() {
            return rects;
        }
        for line in &self.lines {
            if line.start + line.len < range.start || line.start > range.end {
                continue;
            }
            for row in 0..line.row_starts.len() {
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
    type PrepaintState = Option<Rc<Layout>>;

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
                let (text, placeholder) = input.display();
                let (text, _, _) = expand_tabs(&text, &[]);
                let (min, max) = (min.max(1), max.max(min).max(1));
                window.request_measured_layout(layout, move |known, available, window, _| {
                    let width = known.width.or(match available.width {
                        AvailableSpace::Definite(width) => Some(width),
                        _ => None,
                    });
                    // The placeholder does not grow the box: it is cut to one row, as in Zed.
                    let rows: usize = if placeholder {
                        1
                    } else {
                        let wrap = width.map(|width| wrap_width(&font, font_size, width, window));
                        text.split('\n')
                            .map(|line| wrap_rows(line, &font, font_size, wrap, window).len())
                            .sum()
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
            input.clamp_scroll(&laid, bounds);
            // A scroll may move the lines a code view colours: lay out again (only then).
            input.layout(window, cx)
        });
        Some(laid)
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
        let Some(laid) = laid.take() else {
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
        // Spaces and tabs inside the selection of a full box show as dots and arrows; in a code
        // view all of them while Show whitespace is on.
        let shown = input.display().0;
        let whitespace = if full && code.all_whitespace {
            laid.whitespace(&shown, 0..shown.len())
        } else if full && !selection.is_empty() {
            laid.whitespace(&shown, selection.clone())
        } else {
            Vec::new()
        };
        let colors = cx.theme().colors();
        let (background, active_line) = (
            colors.editor_background,
            colors.editor_active_line_background,
        );
        let player = cx.theme().players().local();
        let selection_color = if focused {
            player.selection
        } else {
            player.selection.opacity(0.5)
        };
        // A read-only box draws a grey caret, as Zed's editor did (B1 shot
        // `workspace-docs-source-focused`).
        let cursor = if input.read_only {
            cx.theme().players().read_only().cursor
        } else {
            player.cursor
        };
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
            if current_line && selection.is_empty() {
                let y = laid.position(head, end_bias).y;
                window.paint_quad(fill(
                    Bounds::new(
                        point(bounds.left(), origin.y + y),
                        size(bounds.size.width, line_height),
                    ),
                    active_line,
                ));
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
                // The selection runs twice its corner radius past a line end, as in Zed's editor.
                .rects_past(selection.clone(), line_height * SELECTION_ROUNDING * 2.)
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
            if let Some(gutter) = &code.gutter {
                super::code_gutter::paint_gutter(gutter, &laid, window, cx);
            }
            super::code_gutter::paint_sticky(&code, &laid, bounds, origin, window, cx);
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
            // The horizontal bar has its border line on top.
            if let Some((bar, thumb)) = &hbar {
                let edge =
                    |rect: &Bounds<Pixels>| Bounds::new(rect.origin, size(rect.size.width, px(1.)));
                window.paint_quad(fill(bar.track, track));
                window.paint_quad(fill(edge(&bar.track), track_border));
                window.paint_quad(fill(bar.thumb, *thumb));
                window.paint_quad(fill(edge(&bar.thumb), thumb_border));
            }
        });
    }
}
