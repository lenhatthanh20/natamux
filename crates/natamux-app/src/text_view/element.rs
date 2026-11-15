//! The element of the own text input (plan 6.3.3, 6.3.4): it lays the text out with gpui's text
//! system and paints, in order, the background (full mode), the current line, the selection, the
//! whitespace dots inside it (full mode), the text with the input method's underline, and the
//! caret.

use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use gpui::{
    App, AvailableSpace, Bounds, ContentMask, DispatchPhase, Element, ElementId,
    ElementInputHandler, Entity, Font, GlobalElementId, Hsla, InspectorElementId, IntoElement,
    LayoutId, MouseButton, MouseMoveEvent, MouseUpEvent, Path, Pixels, Point, SharedString, Style,
    TextAlign, TextRun, Window, WrappedLine, fill, point, px, relative, size,
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
    /// The x in the unwrapped line where each row starts.
    row_x: Vec<Pixels>,
    /// The line with its tabs as spaces.
    pub shaped: Rc<WrappedLine>,
    tabs: Vec<Tab>,
    /// It is drawn in one run of the text colour, so a later layout may take it as it is.
    plain: bool,
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

    /// The x of `local` in the unwrapped line.
    fn x(&self, local: usize) -> Pixels {
        self.shaped.unwrapped_layout.x_for_index(self.shown(local))
    }

    /// The x of `local` in `row`, from the left of the row.
    fn x_in_row(&self, row: usize, local: usize) -> Pixels {
        self.x(local) - self.row_x.get(row).copied().unwrap_or_default()
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
        let mut ready: HashMap<&str, &Rc<WrappedLine>> = HashMap::new();
        if let Some(previous) = previous.filter(|previous| {
            let old = &previous.key;
            (old.width, &old.font, old.font_size, old.color)
                == (key.width, &key.font, key.font_size, key.color)
        }) {
            for line in previous.lines.iter().filter(|line| line.plain) {
                ready
                    .entry(line.shaped.text.as_ref())
                    .or_insert(&line.shaped);
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
            let shaped = cached.unwrap_or_else(|| {
                let shown = SharedString::from(shown);
                let shaped = shape(&shown, &line_runs, key.font_size, key.width, window)
                    .into_iter()
                    .next()
                    .unwrap_or_default();
                Rc::new(shaped)
            });
            let mut shown_starts = vec![0];
            shown_starts.extend(shaped.wrap_boundaries().iter().filter_map(|boundary| {
                let run = shaped.runs().get(boundary.run_ix)?;
                Some(run.glyphs.get(boundary.glyph_ix)?.index)
            }));
            let layout = &shaped.unwrapped_layout;
            let row_x = shown_starts
                .iter()
                .map(|&at| layout.x_for_index(at))
                .collect();
            width = width.max(layout.width);
            let mut line = Line {
                start,
                len,
                first_row: row,
                row_starts: Vec::new(),
                row_x,
                shaped,
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

    fn line_of_row(&self, row: usize) -> Option<(&Line, usize)> {
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
        let row_x = line.row_x.get(in_line).copied().unwrap_or_default();
        let shown = line.shaped.unwrapped_layout.closest_index_for_x(x + row_x);
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

    /// The rects of the selection `range`, one per row it covers, from the top left of the text.
    /// A row the selection runs past (its line end or wrap point) gets a bit more on the right.
    pub fn selection_rects(&self, range: Range<usize>) -> Vec<Bounds<Pixels>> {
        let mut rects = Vec::new();
        if range.is_empty() {
            return rects;
        }
        let overshoot = self.line_height * SELECTION_ROUNDING;
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
                let left = line.x_in_row(row, start - line.start);
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
                    line.x(local + c.len_utf8()) - line.x(local)
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
                let runs = input.runs(&style, text.len(), placeholder, cx);
                let (text, runs, _) = expand_tabs(&text, &runs);
                let text = SharedString::from(text);
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
                        shape(&text, &runs, font_size, wrap, window)
                            .iter()
                            .map(|line| line.wrap_boundaries().len() + 1)
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
            laid
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
            .map(|(bar, hovered, dragged)| (bar, thumb_color(hovered, dragged)));
        let markers = code.markers;

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
                window.paint_quad(fill(*bounds, if *active { active_guide } else { guide }));
            }
            for bounds in &code.backgrounds {
                window.paint_quad(fill(*bounds, highlight));
            }
            let rects: Vec<_> = laid
                .selection_rects(selection.clone())
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
                line.shaped
                    .paint(
                        point(origin.x, top),
                        line_height,
                        TextAlign::Left,
                        None,
                        window,
                        cx,
                    )
                    .ok();
            }
            let carets = caret.then_some((head, end_bias)).into_iter();
            for (at, bias) in carets.chain(drop_at.map(|at| (at, false))) {
                let at = laid.position(at, bias);
                window.paint_quad(fill(
                    Bounds::new(origin + at, size(CARET_WIDTH, line_height)),
                    cursor,
                ));
            }
            if let Some((bar, thumb)) = &bar {
                let edge = |rect: &Bounds<Pixels>| {
                    Bounds::new(rect.origin, size(px(1.), rect.size.height))
                };
                window.paint_quad(fill(bar.track, track));
                window.paint_quad(fill(edge(&bar.track), track_border));
                for (marker, color) in &markers {
                    window.paint_quad(fill(*marker, *color));
                }
                window.paint_quad(fill(bar.thumb, *thumb));
                window.paint_quad(fill(edge(&bar.thumb), thumb_border));
            }
        });
    }
}
