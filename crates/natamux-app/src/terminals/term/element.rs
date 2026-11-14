//! The element of an own terminal (docs/plan-mit-license.md 6.2.6). Before each paint it lays
//! out the grid in its bounds, gives the model the size and lets it sync, then draws the copy of
//! the screen: the background, the cell backgrounds, the find matches and the selection, the
//! text, the block elements, the input method's text, the cursor and the scrollbar.

use std::collections::HashMap;
use std::ops::RangeInclusive;

use alacritty_terminal::index::Point as GridPoint;
use alacritty_terminal::term::TermMode;
use alacritty_terminal::vte::ansi::CursorShape;
use gpui::{
    App, BorderStyle, Bounds, ContentMask, Corners, CursorStyle, DispatchPhase, Element, ElementId,
    Entity, FocusHandle, Font, FontFeatures, FontStyle, FontWeight, GlobalElementId, Hitbox,
    HitboxBehavior, Hsla, InspectorElementId, IntoElement, LayoutId, ModifiersChangedEvent,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Path, Pixels, Point, ScrollWheelEvent,
    ShapedLine, Style, TextAlign, TextRun, UnderlineStyle, Window, fill, outline, point, px,
    relative, size,
};
use theme::ActiveTheme;

use super::cell_layout::{self, BLOCK_COLUMNS, BLOCK_LINES, BackgroundRect, MINIMUM_CONTRAST};
use super::grid_layout::grid;
use super::ime::TermInput;
use super::model::{TermBounds, TermModel};
use super::scrollbar::{
    self, BORDER, BORDER_ALPHA, TRACK_WIDTH, ThumbColours, ThumbState, thumb_colour,
};
use super::view::{Laid, TermView};
use crate::app::appearance::Appearance;

/// The line height of the terminal, a factor of its font size.
const LINE_HEIGHT: f32 = 1.3;
/// The corner radius of a highlight, a factor of the line height.
const HIGHLIGHT_RADIUS: f32 = 0.15;
const BAR_CURSOR_WIDTH: Pixels = px(2.);
const UNDERLINE_CURSOR_HEIGHT: Pixels = px(2.);

pub struct TermElement {
    view: Entity<TermView>,
    model: Entity<TermModel>,
    focus: FocusHandle,
}

impl TermElement {
    pub fn new(view: Entity<TermView>, model: Entity<TermModel>, focus: FocusHandle) -> Self {
        TermElement { view, model, focus }
    }
}

impl IntoElement for TermElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

/// The font of the terminal and the size of its cells.
#[derive(Clone)]
struct Metrics {
    font: Font,
    font_size: Pixels,
    cell_width: Pixels,
    line_height: Pixels,
}

/// The user's terminal font, else the buffer font and size (6.1); ligatures off.
fn metrics(window: &Window, cx: &App) -> Metrics {
    let (family, font_size) = match cx.try_global::<Appearance>() {
        Some(appearance) => match &appearance.terminal_font {
            Some((family, size)) => (family.clone(), *size),
            None => (
                appearance.buffer_font.family.clone(),
                appearance.buffer_font_size,
            ),
        },
        None => (".ZedMono".into(), px(13.)),
    };
    let font = Font {
        family,
        features: FontFeatures::disable_ligatures(),
        fallbacks: None,
        weight: FontWeight::NORMAL,
        style: FontStyle::Normal,
    };
    let text_system = window.text_system();
    let font_id = text_system.resolve_font(&font);
    let cell_width = text_system
        .em_advance(font_id, font_size)
        .unwrap_or(font_size * 0.6);
    Metrics {
        font,
        font_size,
        cell_width,
        line_height: font_size * LINE_HEIGHT,
    }
}

#[derive(Clone, Copy, PartialEq)]
enum CursorKind {
    Block,
    Hollow,
    Bar,
    Underline,
}

struct Cursor {
    bounds: Bounds<Pixels>,
    kind: CursorKind,
    color: Hsla,
    /// The character under a block cursor, drawn again in the background colour.
    text: Option<ShapedLine>,
}

struct Scrollbar {
    track: Bounds<Pixels>,
    thumb: Bounds<Pixels>,
    track_colour: Hsla,
    border_colour: Hsla,
    thumb_colour: Hsla,
}

pub struct Frame {
    hitbox: Hitbox,
    /// The scrollbar's track, which the grid leaves free.
    track: Bounds<Pixels>,
    background: Hsla,
    rects: Vec<(Bounds<Pixels>, Hsla)>,
    highlights: Vec<(Path<Pixels>, Hsla)>,
    runs: Vec<(Point<Pixels>, ShapedLine)>,
    blocks: Vec<(Bounds<Pixels>, Hsla)>,
    marked: Option<(Bounds<Pixels>, Hsla, ShapedLine)>,
    cursor: Option<Cursor>,
    scrollbar: Option<Scrollbar>,
    line_height: Pixels,
    /// The pointer shows a hand: a link shows under it.
    over_link: bool,
}

impl Element for TermElement {
    type RequestLayoutState = ();
    type PrepaintState = Frame;

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
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Frame {
        let metrics = metrics(window, cx);
        let (cell_width, line_height) = (metrics.cell_width, metrics.line_height);
        let spare_on_top = {
            let content = self.model.read(cx).last_content();
            content.mode.contains(TermMode::ALT_SCREEN)
                || (content.scrolled_to_bottom && content.bottom_row_used)
        };
        // The grid always leaves the scrollbar's track free (T-G1).
        let area = Bounds {
            origin: bounds.origin,
            size: size(
                (bounds.size.width - TRACK_WIDTH).max(px(0.)),
                bounds.size.height,
            ),
        };
        let laid = grid(
            area,
            cell_width,
            line_height,
            window.scale_factor(),
            spare_on_top,
        );
        let width = (area.size.width - cell_width).max(cell_width * 2.);
        let term_bounds = TermBounds::new(
            line_height,
            cell_width,
            Bounds {
                origin: laid.origin,
                size: size(width, laid.height),
            },
        );
        self.model.update(cx, |model, cx| {
            model.set_size(term_bounds);
            model.sync(cx);
        });
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        let focused = self.focus.is_focused(window);
        let origin = laid.origin;
        let track = Bounds {
            origin: point(bounds.right() - TRACK_WIDTH, bounds.top()),
            size: size(TRACK_WIDTH, bounds.size.height),
        };

        let view = self.view.read(cx);
        let show_cursor = view.should_show_cursor(focused, cx) && view.marked.is_none();
        let marked_text = view.marked.clone();
        let bar_alpha = view.bar_alpha(cx);
        let (bar_hovered, bar_dragged) = (view.bar.hovered, view.bar.grab.is_some());
        let model = self.model.read(cx);
        let content = model.last_content();
        let theme = cx.theme();
        let colours = theme.colors();
        let text_system = window.text_system().clone();
        let shape = |text: String, font: Font, color: Hsla, underline: Option<UnderlineStyle>| {
            let run = TextRun {
                len: text.len(),
                font,
                color,
                background_color: None,
                underline,
                strikethrough: None,
            };
            text_system.shape_line(text.into(), metrics.font_size, &[run], Some(cell_width))
        };

        let hovered = content.last_hovered_word.as_ref().map(|word| &word.range);
        let cells = cell_layout::layout(
            &content.cells,
            content.display_offset,
            theme,
            hovered,
            MINIMUM_CONTRAST,
        );
        let at = |line: usize, column: usize| {
            point(
                origin.x + cell_width * column as f32,
                origin.y + line_height * line as f32,
            )
        };
        let rects = merged_rects(&cells.rects)
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
        let runs = cells
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
        let blocks = cells
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

        // Find matches, then the selection over them.
        let shown = Shown {
            origin,
            cell_width,
            line_height,
            columns: content.columns,
            lines: content.screen_lines,
            offset: content.display_offset,
        };
        let mut highlights = Vec::new();
        for found in &model.matches {
            for path in shown.highlight(found, false) {
                highlights.push((path, colours.search_match_background));
            }
        }
        if let Some(selection) = &content.selection {
            let range = selection.start..=selection.end;
            for path in shown.highlight(&range, selection.is_block) {
                highlights.push((path, theme.players().local().selection));
            }
        }

        // The cursor, where the input method's text also goes.
        let cursor_line = content.cursor.point.line.0 + content.display_offset as i32;
        let cursor_column = content.cursor.point.column.0;
        let cursor_origin = point(
            origin.x + (cell_width * cursor_column as f32).floor(),
            origin.y + (line_height * cursor_line as f32).floor(),
        );
        let on_screen = usize::try_from(cursor_line).is_ok_and(|line| line < content.screen_lines);
        let ime_cursor = on_screen.then_some(Bounds {
            origin: cursor_origin,
            size: size(cell_width, line_height),
        });
        let cursor_colour = theme.players().local().cursor;
        let kind = match content.cursor.shape {
            CursorShape::Hidden => None,
            _ if !focused => Some(CursorKind::Hollow),
            CursorShape::Block => Some(CursorKind::Block),
            CursorShape::Underline => Some(CursorKind::Underline),
            CursorShape::Beam => Some(CursorKind::Bar),
            CursorShape::HollowBlock => Some(CursorKind::Hollow),
        };
        let cursor = kind.filter(|_| on_screen && show_cursor).map(|kind| {
            let c = content.cursor_char;
            let text = (!c.is_whitespace()).then(|| {
                shape(
                    c.to_string(),
                    metrics.font.clone(),
                    colours.terminal_ansi_background,
                    None,
                )
            });
            let width = text
                .as_ref()
                .map_or(cell_width, |line| line.width().max(cell_width))
                .ceil();
            let full = Bounds {
                origin: cursor_origin,
                size: size(width, line_height),
            };
            let bounds = match kind {
                CursorKind::Bar => Bounds {
                    size: size(BAR_CURSOR_WIDTH, line_height),
                    ..full
                },
                CursorKind::Underline => Bounds {
                    origin: point(
                        cursor_origin.x,
                        cursor_origin.y + line_height - UNDERLINE_CURSOR_HEIGHT,
                    ),
                    size: size(width, UNDERLINE_CURSOR_HEIGHT),
                },
                CursorKind::Block | CursorKind::Hollow => full,
            };
            Cursor {
                bounds: window.pixel_snap_bounds(bounds),
                kind,
                color: cursor_colour,
                text: (kind == CursorKind::Block).then_some(text).flatten(),
            }
        });
        let marked = marked_text.map(|text| {
            let foreground = colours.terminal_foreground;
            let underline = UnderlineStyle {
                thickness: px(1.),
                color: Some(foreground),
                wavy: false,
            };
            let line = shape(text, metrics.font.clone(), foreground, Some(underline));
            let bounds = Bounds {
                origin: cursor_origin,
                size: size(line.width(), line_height),
            };
            (bounds, colours.terminal_background, line)
        });

        let thumb = scrollbar::thumb(
            track,
            content.total_lines,
            content.screen_lines,
            content.display_offset,
        );
        let scrollbar = thumb.filter(|_| bar_alpha > 0.).map(|thumb| {
            let state = if bar_dragged {
                ThumbState::Dragged
            } else if bar_hovered {
                ThumbState::Hovered
            } else {
                ThumbState::Idle
            };
            let track_colour = colours.editor_background;
            Scrollbar {
                track,
                thumb,
                track_colour: track_colour.opacity(bar_alpha),
                border_colour: colours.border_variant.opacity(BORDER_ALPHA * bar_alpha),
                thumb_colour: thumb_colour(state, ThumbColours::of(colours), track_colour, true)
                    .opacity(bar_alpha),
            }
        });
        let seen = (
            content.total_lines,
            content.screen_lines,
            content.display_offset,
        );
        let over_link = content.last_hovered_word.is_some() && window.modifiers().secondary();
        let background = colours.terminal_background;

        let laid = Laid {
            bounds: area,
            track,
            thumb,
            cell_width,
            cursor: ime_cursor,
        };
        self.view.update(cx, |view, cx| {
            view.laid = Some(laid);
            if view.bar.seen != seen {
                view.bar.seen = seen;
                view.show_bar(cx);
            }
        });
        if self.view.read(cx).bar_fading(cx) {
            window.request_animation_frame();
        }
        Frame {
            hitbox,
            track,
            background,
            rects,
            highlights,
            runs,
            blocks,
            marked,
            cursor,
            scrollbar,
            line_height,
            over_link,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        frame: &mut Frame,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            window.paint_quad(fill(bounds, frame.background));
            self.listen(&frame.hitbox, frame.track, window);
            let style = if frame.over_link {
                CursorStyle::PointingHand
            } else {
                CursorStyle::IBeam
            };
            window.set_cursor_style(style, &frame.hitbox);
            if self.focus.is_focused(window) {
                window.handle_input(&self.focus, TermInput::new(self.view.clone()), cx);
            }
            let (view, hitbox) = (self.view.clone(), frame.hitbox.clone());
            window.on_modifiers_changed(move |event: &ModifiersChangedEvent, window, cx| {
                let over = hitbox.is_hovered_at(window.mouse_position(), window);
                view.update(cx, |view, cx| {
                    view.modifiers_changed(event, over, window, cx)
                });
            });
            for (bounds, colour) in &frame.rects {
                window.paint_quad(fill(*bounds, *colour));
            }
            for (path, colour) in frame.highlights.drain(..) {
                window.paint_path(path, colour);
            }
            for (origin, line) in &frame.runs {
                line.paint(
                    *origin,
                    frame.line_height,
                    TextAlign::Left,
                    None,
                    window,
                    cx,
                )
                .ok();
            }
            for (bounds, colour) in &frame.blocks {
                window.paint_quad(fill(*bounds, *colour));
            }
            if let Some((bounds, background, line)) = &frame.marked {
                window.paint_quad(fill(*bounds, *background));
                line.paint(
                    bounds.origin,
                    frame.line_height,
                    TextAlign::Left,
                    None,
                    window,
                    cx,
                )
                .ok();
            }
            if let Some(cursor) = &frame.cursor {
                match cursor.kind {
                    CursorKind::Hollow => {
                        window.paint_quad(outline(cursor.bounds, cursor.color, BorderStyle::Solid))
                    }
                    _ => window.paint_quad(fill(cursor.bounds, cursor.color)),
                }
                if let Some(text) = &cursor.text {
                    text.paint(
                        cursor.bounds.origin,
                        frame.line_height,
                        TextAlign::Left,
                        None,
                        window,
                        cx,
                    )
                    .ok();
                }
            }
            if let Some(bar) = &frame.scrollbar {
                window.paint_quad(fill(bar.track, bar.track_colour));
                let border = Bounds {
                    size: size(BORDER, bar.track.size.height),
                    ..bar.track
                };
                window.paint_quad(fill(border, bar.border_colour));
                let radius = bar.thumb.size.width / 2.;
                window.paint_quad(
                    fill(bar.thumb, bar.thumb_colour).corner_radii(Corners::all(radius)),
                );
            }
        });
    }
}

impl TermElement {
    /// The mouse goes to the view: presses and the wheel over the element, moves and releases
    /// anywhere, so a drag can leave it. The scrollbar's track is not over the screen.
    fn listen(&self, hitbox: &Hitbox, track: Bounds<Pixels>, window: &mut Window) {
        let (view, area) = (self.view.clone(), hitbox.clone());
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble && area.is_hovered(window) {
                let over = !track.contains(&event.position);
                view.update(cx, |view, cx| view.mouse_down(event, over, window, cx));
            }
        });
        let (view, area) = (self.view.clone(), hitbox.clone());
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble {
                let over = area.is_hovered(window) && !track.contains(&event.position);
                view.update(cx, |view, cx| view.mouse_move(event, over, window, cx));
            }
        });
        let (view, area) = (self.view.clone(), hitbox.clone());
        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble {
                let over = area.is_hovered(window) && !track.contains(&event.position);
                view.update(cx, |view, cx| view.mouse_up(event, over, window, cx));
            }
        });
        let (view, area) = (self.view.clone(), hitbox.clone());
        window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
            // Not `is_hovered`: after a key that is false until the mouse moves.
            if phase == DispatchPhase::Bubble && area.should_handle_scroll(window) {
                view.update(cx, |view, cx| view.scroll_wheel(event, cx));
            }
        });
    }
}

/// Background rects of the same columns and colour on the lines below each other, joined into
/// one, with the lines each covers.
fn merged_rects(rects: &[BackgroundRect]) -> Vec<(BackgroundRect, usize)> {
    let mut merged: Vec<(BackgroundRect, usize)> = Vec::new();
    // (column, cells, the line after it) of each merged rect.
    let mut ends: HashMap<(usize, usize, usize), usize> = HashMap::new();
    for rect in rects {
        let key = (rect.column, rect.cells, rect.line);
        if let Some(&index) = ends.get(&key)
            && merged[index].0.color == rect.color
        {
            ends.remove(&key);
            merged[index].1 += 1;
            let (first, lines) = &merged[index];
            ends.insert((first.column, first.cells, first.line + lines), index);
            continue;
        }
        ends.insert((rect.column, rect.cells, rect.line + 1), merged.len());
        merged.push((rect.clone(), 1));
    }
    merged
}

/// The shown rows of the grid, to turn grid ranges into highlights.
struct Shown {
    origin: Point<Pixels>,
    cell_width: Pixels,
    line_height: Pixels,
    columns: usize,
    lines: usize,
    offset: usize,
}

impl Shown {
    /// The paths of a range of cells: one per group of rows whose spans touch, with round
    /// corners. A block range spans the same columns on every row.
    fn highlight(&self, range: &RangeInclusive<GridPoint>, block: bool) -> Vec<Path<Pixels>> {
        let (start, end) = (*range.start(), *range.end());
        let shown = |line: i32| line + self.offset as i32;
        let (first, last) = (shown(start.line.0).max(0), shown(end.line.0));
        let last = last.min(self.lines as i32 - 1);
        let mut spans = Vec::new();
        for line in first..=last {
            let from = if block || line == shown(start.line.0) {
                start.column.0
            } else {
                0
            };
            let to = if block || line == shown(end.line.0) {
                end.column.0
            } else {
                self.columns.saturating_sub(1)
            };
            if from > to {
                continue;
            }
            let left = self.origin.x + self.cell_width * from as f32;
            let right = self.origin.x + self.cell_width * (to + 1) as f32;
            spans.push((line as usize, left, right));
        }
        let radius = self.line_height * HIGHLIGHT_RADIUS;
        let mut paths = Vec::new();
        let mut group: Vec<(Pixels, Pixels)> = Vec::new();
        let mut group_top = 0;
        let mut previous: Option<(usize, Pixels, Pixels)> = None;
        for (line, left, right) in spans {
            let touches = previous.is_some_and(|(prev_line, prev_left, prev_right)| {
                prev_line + 1 == line && left <= prev_right && right >= prev_left
            });
            if !touches && !group.is_empty() {
                let top = self.origin.y + self.line_height * group_top as f32;
                paths.extend(rounded_path(&group, top, self.line_height, radius));
                group.clear();
            }
            if group.is_empty() {
                group_top = line;
            }
            group.push((left, right));
            previous = Some((line, left, right));
        }
        if !group.is_empty() {
            let top = self.origin.y + self.line_height * group_top as f32;
            paths.extend(rounded_path(&group, top, self.line_height, radius));
        }
        paths
    }
}

/// The outline of rows of spans (left, right) from `top`, each `line_height` high, with every
/// corner rounded by up to `radius` (at most half of each edge).
fn rounded_path(
    spans: &[(Pixels, Pixels)],
    top: Pixels,
    line_height: Pixels,
    radius: Pixels,
) -> Option<Path<Pixels>> {
    let y = |row: usize| top + line_height * row as f32;
    let last = spans.len().checked_sub(1)?;
    // Down the right side, then up the left side.
    let mut corners = vec![point(spans[0].0, y(0)), point(spans[0].1, y(0))];
    for row in 0..last {
        corners.push(point(spans[row].1, y(row + 1)));
        corners.push(point(spans[row + 1].1, y(row + 1)));
    }
    corners.push(point(spans[last].1, y(last + 1)));
    corners.push(point(spans[last].0, y(last + 1)));
    for row in (1..=last).rev() {
        corners.push(point(spans[row].0, y(row)));
        corners.push(point(spans[row - 1].0, y(row)));
    }
    let corners = without_straight_points(corners);
    if corners.len() < 3 {
        return None;
    }
    let count = corners.len();
    let toward = |from: Point<Pixels>, to: Point<Pixels>, by: Pixels| {
        let length = edge_length(from, to);
        if length <= px(0.) {
            return from;
        }
        let part = by / length;
        point(
            from.x + (to.x - from.x) * part,
            from.y + (to.y - from.y) * part,
        )
    };
    let corner_points = |index: usize| {
        let here = corners[index];
        let before = corners[(index + count - 1) % count];
        let after = corners[(index + 1) % count];
        let into = radius.min(edge_length(before, here) / 2.);
        let out = radius.min(edge_length(here, after) / 2.);
        (toward(here, before, into), here, toward(here, after, out))
    };
    let (_, _, start) = corner_points(0);
    let mut path = Path::new(start);
    for index in 1..=count {
        let (enter, corner, leave) = corner_points(index % count);
        path.line_to(enter);
        path.curve_to(leave, corner);
    }
    Some(path)
}

fn edge_length(from: Point<Pixels>, to: Point<Pixels>) -> Pixels {
    let (dx, dy) = (f32::from(to.x - from.x), f32::from(to.y - from.y));
    px((dx * dx + dy * dy).sqrt())
}

/// The corners of an outline, without repeated points and points in the middle of a straight
/// edge.
fn without_straight_points(points: Vec<Point<Pixels>>) -> Vec<Point<Pixels>> {
    let mut kept: Vec<Point<Pixels>> = Vec::new();
    for point in points {
        if kept.last() == Some(&point) {
            continue;
        }
        kept.push(point);
    }
    while kept.len() > 1 && kept.first() == kept.last() {
        kept.pop();
    }
    let mut changed = true;
    while changed && kept.len() >= 3 {
        changed = false;
        let count = kept.len();
        for index in 0..count {
            let before = kept[(index + count - 1) % count];
            let here = kept[index];
            let after = kept[(index + 1) % count];
            let straight = (before.x == here.x && here.x == after.x)
                || (before.y == here.y && here.y == after.y);
            if straight {
                kept.remove(index);
                changed = true;
                break;
            }
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rects_of_the_same_columns_on_lines_below_each_other_join() {
        let colour = gpui::black();
        let rect = |line, column, cells| BackgroundRect {
            line,
            column,
            cells,
            color: colour,
        };
        let merged = merged_rects(&[rect(0, 2, 3), rect(1, 2, 3), rect(1, 7, 1), rect(2, 2, 3)]);
        assert_eq!(merged, [(rect(0, 2, 3), 3), (rect(1, 7, 1), 1)]);
    }

    #[test]
    fn an_outline_keeps_only_its_corners() {
        let at = |x: f32, y: f32| point(px(x), px(y));
        // Two rows of one width: a plain rect.
        let kept = without_straight_points(vec![
            at(0., 0.),
            at(10., 0.),
            at(10., 5.),
            at(10., 5.),
            at(10., 10.),
            at(0., 10.),
            at(0., 5.),
            at(0., 5.),
        ]);
        assert_eq!(kept, [at(0., 0.), at(10., 0.), at(10., 10.), at(0., 10.)]);
    }
}
