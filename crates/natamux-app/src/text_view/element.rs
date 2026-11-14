//! The element of the own text input (plan 6.3.3, 6.3.4): it lays the text out with gpui's text
//! system and paints, in order, the background (full mode), the current line, the selection, the
//! text with the input method's underline, and the caret.

use std::ops::Range;
use std::rc::Rc;

use gpui::{
    App, AvailableSpace, Bounds, ContentMask, DispatchPhase, Element, ElementId, ElementInputHandler,
    Entity, Font, GlobalElementId, Hsla, InspectorElementId, IntoElement, LayoutId, MouseButton,
    MouseMoveEvent, MouseUpEvent, Pixels, Point, SharedString, Style, TextAlign, TextRun, Window,
    WrappedLine, fill, point, px, relative, size,
};
use theme::ActiveTheme;

use super::text_input::{Mode, TextInput};

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
    pub shaped: WrappedLine,
}

impl Line {
    fn row_end(&self, row: usize) -> usize {
        self.row_starts.get(row + 1).copied().unwrap_or(self.len)
    }

    /// The x of `local` in `row`, from the left of the row.
    fn x_in_row(&self, row: usize, local: usize) -> Pixels {
        let layout = &self.shaped.unwrapped_layout;
        let start = self.row_starts.get(row).copied().unwrap_or(0);
        layout.x_for_index(local) - layout.x_for_index(start)
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

impl Layout {
    pub fn build(key: LayoutKey, text: &SharedString, runs: &[TextRun], window: &Window) -> Self {
        let shaped = shape(text, runs, key.font_size, key.width, window);
        let mut lines = Vec::with_capacity(shaped.len());
        let (mut start, mut row, mut width) = (0, 0, px(0.));
        for shaped in shaped {
            let len = shaped.len();
            let mut row_starts = vec![0];
            row_starts.extend(shaped.wrap_boundaries().iter().filter_map(|boundary| {
                let run = shaped.runs().get(boundary.run_ix)?;
                Some(run.glyphs.get(boundary.glyph_ix)?.index)
            }));
            width = width.max(shaped.unwrapped_layout.width);
            let count = row_starts.len();
            lines.push(Line {
                start,
                len,
                first_row: row,
                row_starts,
                shaped,
            });
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
            rows: row.max(1),
            width,
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
        let layout = &line.shaped.unwrapped_layout;
        let local = layout
            .closest_index_for_x(x + layout.x_for_index(start))
            .clamp(start, end);
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
        let rem = window.rem_size();
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
                let (text, placeholder) = input.display();
                let runs = input.runs(&style, text.len(), placeholder, cx);
                let (min, max) = (min.max(1), max.max(min).max(1));
                window.request_measured_layout(layout, move |known, available, window, _| {
                    let width = known.width.or(match available.width {
                        AvailableSpace::Definite(width) => Some(width),
                        _ => None,
                    });
                    let rows: usize = shape(&text, &runs, font_size, width, window)
                        .iter()
                        .map(|line| line.wrap_boundaries().len() + 1)
                        .sum();
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
        let origin = bounds.origin - input.scroll;
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
        let colors = cx.theme().colors();
        let (background, active_line) = (colors.editor_background, colors.editor_active_line_background);
        let player = cx.theme().players().local();
        let selection_color = if focused {
            player.selection
        } else {
            player.selection.opacity(0.5)
        };
        let cursor = player.cursor;
        let line_height = laid.line_height;

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
            for rect in laid.selection_rects(selection.clone()) {
                window.paint_quad(
                    fill(
                        Bounds::new(rect.origin + origin, rect.size),
                        selection_color,
                    )
                    .corner_radii(line_height * SELECTION_ROUNDING),
                );
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
        });
    }
}
