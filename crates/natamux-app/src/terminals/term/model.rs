//! The model of an own terminal (docs/plan-mit-license.md 6.2.4): an alacritty screen behind a
//! fair lock, fed by a writer on the parser's thread, with the event loop that wakes the view.
//! What the screen would write to a PTY (query answers, focus reports, typed text) goes to the
//! input sink. Changes the view asks for wait in a queue and run in their order when the view
//! syncs before a paint; the paint then reads the copy of the screen that sync made.

use std::collections::VecDeque;
use std::ops::RangeInclusive;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use alacritty_terminal::Term;
use alacritty_terminal::event::{Event, WindowSize};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Direction, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionRange, SelectionType};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::color::COUNT;
use alacritty_terminal::term::search::{Match, RegexIter, RegexSearch};
use alacritty_terminal::term::{
    Config, Osc52, RenderableCursor, SEMANTIC_ESCAPE_CHARS, TermMode, viewport_to_point,
};
use alacritty_terminal::vte::ansi::{
    ClearMode, Color, CursorShape, CursorStyle, Handler, NamedColor, Processor, Rgb,
};
use futures::channel::mpsc::{UnboundedSender, unbounded};
use gpui::{
    App, Bounds, ClipboardItem, Context, EventEmitter, Hsla, Pixels, Rgba, Task, point, px, size,
};
use theme::ActiveTheme;

use super::event_loop::{self, Listener};
use super::grid_layout::whole;
use super::writer::TermWriter;
use super::{colors, links};

/// How often the screen wakes its view while output streams, until the lane sets its own.
pub const DEFAULT_PACE: Duration = Duration::from_millis(4);
/// The history of a terminal built with no width yet (the daemon's largest).
pub const MAX_HISTORY: usize = 100_000;

/// What the model tells its view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TermEvent {
    /// New output, or a change the view must draw.
    Wakeup,
    /// The program asked for another cursor blink.
    BlinkChanged,
    /// The whole screen was replaced.
    OutputReplaced,
}

/// The cell size and the bounds of the grid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TermBounds {
    pub cell_width: Pixels,
    pub line_height: Pixels,
    pub bounds: Bounds<Pixels>,
}

impl TermBounds {
    pub fn new(line_height: Pixels, cell_width: Pixels, bounds: Bounds<Pixels>) -> Self {
        TermBounds {
            cell_width,
            line_height,
            bounds,
        }
    }

    /// The rows that fit, counted one float step up, so 3.9999999 counts as 4.
    pub fn num_lines(&self) -> usize {
        whole(f32::from(self.bounds.size.height) / f32::from(self.line_height))
    }

    pub fn num_columns(&self) -> usize {
        whole(f32::from(self.bounds.size.width) / f32::from(self.cell_width)).max(2)
    }

    /// For the text area size answer: the cell size in whole pixels.
    fn window_size(&self) -> WindowSize {
        let whole_px = |value: Pixels| f32::from(value) as u16;
        WindowSize {
            num_lines: self.num_lines().min(usize::from(u16::MAX)) as u16,
            num_cols: self.num_columns().min(usize::from(u16::MAX)) as u16,
            cell_width: whole_px(self.cell_width),
            cell_height: whole_px(self.line_height),
        }
    }

    /// At least one cell each way, so the screen always has a row and a column.
    fn at_least_one_cell(mut self) -> Self {
        self.bounds.size.width = self.bounds.size.width.max(self.cell_width);
        self.bounds.size.height = self.bounds.size.height.max(self.line_height);
        self
    }

    /// The cell under `position` (window pixels) on the shown rows, clamped to a grid of
    /// `columns` x `lines`, and the side of the cell it is on.
    fn shown_cell(
        &self,
        position: gpui::Point<Pixels>,
        columns: usize,
        lines: usize,
    ) -> (Point<usize>, Side) {
        let from = position - self.bounds.origin;
        let x = f32::from(from.x).max(0.);
        let y = f32::from(from.y).max(0.);
        let (width, height) = (f32::from(self.cell_width), f32::from(self.line_height));
        let column = ((x / width) as usize).min(columns.saturating_sub(1));
        let line = ((y / height) as usize).min(lines.saturating_sub(1));
        let side = if x - column as f32 * width > width / 2. {
            Side::Right
        } else {
            Side::Left
        };
        (Point::new(line, Column(column)), side)
    }

    /// A change the shell would see: the grid or the cell size.
    fn differs(&self, other: &TermBounds) -> bool {
        self.num_lines() != other.num_lines()
            || self.num_columns() != other.num_columns()
            || self.cell_width != other.cell_width
            || self.line_height != other.line_height
    }
}

/// Before the first layout: 500 x 30 px of 5 x 5 px cells, 100 x 6.
impl Default for TermBounds {
    fn default() -> Self {
        TermBounds::new(
            px(5.),
            px(5.),
            Bounds::new(point(px(0.), px(0.)), size(px(500.), px(30.))),
        )
    }
}

impl Dimensions for TermBounds {
    fn total_lines(&self) -> usize {
        self.screen_lines()
    }

    fn screen_lines(&self) -> usize {
        self.num_lines()
    }

    fn columns(&self) -> usize {
        self.num_columns()
    }
}

/// A cell of the shown screen, where it is.
#[derive(Clone, Debug)]
pub struct IndexedCell {
    pub point: Point,
    pub cell: Cell,
}

/// A link under the pointer: its target and the cells it covers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HoveredWord {
    pub word: String,
    pub range: RangeInclusive<Point>,
}

/// The shown screen as the last sync copied it.
#[derive(Clone)]
pub struct Content {
    pub cells: Vec<IndexedCell>,
    pub mode: TermMode,
    pub total_lines: usize,
    pub display_offset: usize,
    pub columns: usize,
    pub screen_lines: usize,
    pub selection: Option<SelectionRange>,
    pub selection_text: Option<String>,
    pub cursor: RenderableCursor,
    pub cursor_char: char,
    pub terminal_bounds: TermBounds,
    pub last_hovered_word: Option<HoveredWord>,
    /// The cursor or some text is on the last row.
    pub bottom_row_used: bool,
    pub scrolled_to_top: bool,
    pub scrolled_to_bottom: bool,
}

/// A search of the screen and its history, in alacritty's regex syntax.
pub struct Search(RegexSearch);

impl Search {
    /// None when the pattern is no valid regex.
    pub fn new(pattern: &str) -> Option<Search> {
        RegexSearch::new(pattern).ok().map(Search)
    }
}

/// A change the view asked for; it runs at the next sync, in the order asked.
enum Change {
    Resize(TermBounds),
    Clear,
    Scroll(Scroll),
    ScrollToPoint(Point),
    SetSelection(Option<Selection>),
    /// The pixel positions are turned into cells when the change runs, with the display
    /// offset of that moment, so a scroll queued before them counts.
    UpdateSelection(gpui::Point<Pixels>),
    Copy,
    FindLink(gpui::Point<Pixels>),
    OpenLink(gpui::Point<Pixels>),
}

pub struct TermModel {
    term: Arc<FairMutex<Term<Listener>>>,
    config: Config,
    /// The screen being shown; events of an older one are dropped.
    generation: Arc<AtomicU64>,
    events: UnboundedSender<(u64, Event)>,
    /// The pace of the event loop, in nanoseconds.
    pace: Arc<AtomicU64>,
    sink: Option<UnboundedSender<Vec<u8>>>,
    queue: VecDeque<Change>,
    last_content: Content,
    /// The size asked for last; it reaches the screen at the next sync.
    bounds: TermBounds,
    /// Parses the output tests and shots write with no writer.
    processor: Processor,
    /// The last byte `write_output` got was a CR.
    after_cr: bool,
    /// Where the pointer looked for a link, in window pixels.
    hovered: Option<gpui::Point<Pixels>>,
    hovered_word: Option<HoveredWord>,
    pub matches: Vec<Match>,
    bell: bool,
    title: Option<String>,
    _event_loop: Task<()>,
}

impl EventEmitter<TermEvent> for TermModel {}

impl TermModel {
    /// `history`: the lines of history it keeps (fixed); `bounds`: the grid it starts at.
    pub fn new(history: Option<usize>, bounds: Option<TermBounds>, cx: &mut Context<Self>) -> Self {
        let config = Config {
            scrolling_history: history.unwrap_or(MAX_HISTORY),
            default_cursor_style: CursorStyle {
                shape: CursorShape::Block,
                blinking: false,
            },
            semantic_escape_chars: format!("{SEMANTIC_ESCAPE_CHARS}─"),
            kitty_keyboard: false,
            osc52: Osc52::Disabled,
            ..Config::default()
        };
        let bounds = bounds.unwrap_or_default();
        let (events, queued) = unbounded();
        let generation = Arc::new(AtomicU64::new(0));
        let term = Term::new(config.clone(), &bounds, Listener::new(events.clone(), 0));
        let last_content = copy_content(&term, bounds, None);
        let pace = Arc::new(AtomicU64::new(nanos(DEFAULT_PACE)));
        let executor = cx.background_executor().clone();
        let loop_pace = pace.clone();
        let event_loop = cx.spawn(async move |this, cx| {
            event_loop::run(
                queued,
                || executor.timer(Duration::from_nanos(loop_pace.load(Ordering::Relaxed))),
                |batch| this.update(cx, |model, cx| model.handle(batch, cx)).is_ok(),
            )
            .await;
        });
        TermModel {
            term: Arc::new(FairMutex::new(term)),
            config,
            generation,
            events,
            pace,
            sink: None,
            queue: VecDeque::new(),
            last_content,
            bounds,
            processor: Processor::new(),
            after_cr: false,
            hovered: None,
            hovered_word: None,
            matches: Vec::new(),
            bell: false,
            title: None,
            _event_loop: event_loop,
        }
    }

    /// Where what the screen would write to a PTY goes.
    pub fn set_display_only_input(&mut self, sink: UnboundedSender<Vec<u8>>) -> anyhow::Result<()> {
        self.sink = Some(sink);
        Ok(())
    }

    /// A writer for the parser. It writes to the screen shown now, also after a replace.
    pub fn display_only_writer(&self) -> anyhow::Result<TermWriter> {
        let generation = self.generation.load(Ordering::Acquire);
        let listener = Listener::new(self.events.clone(), generation);
        Ok(TermWriter::new(self.term.clone(), listener))
    }

    pub fn set_wakeup_pace(&self, pace: Duration) {
        self.pace.store(nanos(pace), Ordering::Relaxed);
    }

    pub fn wakeup_pace(&self) -> Duration {
        Duration::from_nanos(self.pace.load(Ordering::Relaxed))
    }

    /// A new screen at the size asked for last, with `bytes` on it. Selection, matches, the link
    /// under the pointer, the title and the queued changes go with the old one; a queued resize
    /// is in the new size.
    pub fn replace_display_output(
        &mut self,
        bytes: &[u8],
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        let generation = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
        let listener = Listener::new(self.events.clone(), generation);
        let bounds = self.bounds;
        let term = Term::new(self.config.clone(), &bounds, listener);
        self.term = Arc::new(FairMutex::new(term));
        self.processor = Processor::new();
        self.after_cr = false;
        self.queue.clear();
        self.matches.clear();
        self.hovered = None;
        self.hovered_word = None;
        self.title = None;
        self.bell = false;
        let mut term = self.term.lock_unfair();
        self.processor.advance(&mut *term, bytes);
        self.last_content = copy_content(&term, bounds, None);
        drop(term);
        cx.emit(TermEvent::OutputReplaced);
        // The new screen may blink another way, and its view draws it.
        cx.emit(TermEvent::BlinkChanged);
        cx.emit(TermEvent::Wakeup);
        cx.notify();
        Ok(())
    }

    /// Output written straight to the screen, as tests and shots do. A bare LF becomes CR LF,
    /// also when the CR came at the end of the last call.
    pub fn write_output(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        let mut output = Vec::with_capacity(bytes.len());
        for &byte in bytes {
            if byte == b'\n' && !self.after_cr {
                output.push(b'\r');
            }
            output.push(byte);
            self.after_cr = byte == b'\r';
        }
        let mut term = self.term.lock_unfair();
        self.processor.advance(&mut *term, &output);
        drop(term);
        cx.emit(TermEvent::Wakeup);
        cx.notify();
    }

    pub fn last_content(&self) -> &Content {
        &self.last_content
    }

    /// The history it keeps, fixed when it was built.
    pub fn history(&self) -> usize {
        self.config.scrolling_history
    }

    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// The program rang the bell since the last key.
    pub fn bell(&self) -> bool {
        self.bell
    }

    pub fn clear_bell(&mut self) {
        self.bell = false;
    }

    /// The program asked for a blinking cursor (DECSCUSR or mode 12).
    pub fn cursor_blinks(&self) -> bool {
        self.term.lock_unfair().cursor_style().blinking
    }

    /// All the text of the screen and its history.
    pub fn get_content(&self) -> String {
        let term = self.term.lock_unfair();
        let start = Point::new(term.topmost_line(), Column(0));
        let end = Point::new(term.bottommost_line(), term.last_column());
        term.bounds_to_string(start, end)
    }

    /// Typed text: the view goes to the bottom and the selection goes.
    pub fn type_bytes(&mut self, bytes: impl Into<Vec<u8>>) {
        self.queue.push_back(Change::Scroll(Scroll::Bottom));
        self.queue.push_back(Change::SetSelection(None));
        self.write_to_pty(bytes);
    }

    /// Bytes for the shell with no scroll and no selection change: query answers, focus,
    /// mouse and wheel reports.
    pub fn write_to_pty(&self, bytes: impl Into<Vec<u8>>) {
        if let Some(sink) = &self.sink {
            sink.unbounded_send(bytes.into()).ok();
        }
    }

    /// Wrapped in the bracketed paste marks, with no ESC inside, when the program asked for
    /// them; else every line break becomes a CR.
    pub fn paste(&mut self, text: &str) {
        let text = if self.last_content.mode.contains(TermMode::BRACKETED_PASTE) {
            format!("\x1b[200~{}\x1b[201~", text.replace('\x1b', ""))
        } else {
            text.replace("\r\n", "\r").replace('\n', "\r")
        };
        self.type_bytes(text);
    }

    /// The view got the focus: a focus report when the program asked for them, and the cursor
    /// is a block again.
    pub fn focus_in(&mut self) {
        self.set_focused(true);
    }

    /// The view lost the focus: a focus report when asked for, and a hollow cursor.
    pub fn focus_out(&mut self) {
        self.set_focused(false);
    }

    fn set_focused(&mut self, focused: bool) {
        if self.last_content.mode.contains(TermMode::FOCUS_IN_OUT) {
            self.write_to_pty(if focused { "\x1b[I" } else { "\x1b[O" });
        }
        self.config.default_cursor_style.shape = if focused {
            CursorShape::Block
        } else {
            CursorShape::HollowBlock
        };
        let mut term = self.term.lock();
        term.is_focused = focused;
        term.set_options(self.config.clone());
    }

    /// The new bounds are kept at once (points are mapped from their origin); the screen is
    /// resized, at the next sync, only when the grid or the cell size changes.
    pub fn set_size(&mut self, bounds: TermBounds) {
        let bounds = bounds.at_least_one_cell();
        let resize = bounds.differs(&self.bounds);
        self.bounds = bounds;
        self.last_content.terminal_bounds = bounds;
        if resize {
            self.queue.push_back(Change::Resize(bounds));
        }
    }

    /// Lines into the history.
    pub fn scroll_up_by(&mut self, lines: usize) {
        let lines = i32::try_from(lines).unwrap_or(i32::MAX);
        self.queue.push_back(Change::Scroll(Scroll::Delta(lines)));
    }

    pub fn scroll_down_by(&mut self, lines: usize) {
        let lines = i32::try_from(lines).unwrap_or(i32::MAX);
        self.queue.push_back(Change::Scroll(Scroll::Delta(-lines)));
    }

    pub fn scroll_to_top(&mut self) {
        self.queue.push_back(Change::Scroll(Scroll::Top));
    }

    pub fn scroll_to_bottom(&mut self) {
        self.queue.push_back(Change::Scroll(Scroll::Bottom));
    }

    /// Clears the local screen and its history; the shell is not told.
    pub fn clear(&mut self) {
        self.queue.push_back(Change::Clear);
    }

    pub fn set_selection(&mut self, selection: Option<Selection>) {
        self.queue.push_back(Change::SetSelection(selection));
    }

    /// Moves the end of the selection to the cell under `position` (window pixels).
    pub fn update_selection(&mut self, position: gpui::Point<Pixels>) {
        self.queue.push_back(Change::UpdateSelection(position));
    }

    /// The whole history and screen, as they are now: a resize queued before it moves the
    /// lines, not the selection.
    pub fn select_all(&mut self) {
        let term = self.term.lock_unfair();
        let start = Point::new(term.topmost_line(), Column(0));
        let end = Point::new(term.bottommost_line(), term.last_column());
        drop(term);
        self.set_selection(Some(simple(start..=end)));
    }

    /// Copies the selection; it stays.
    pub fn copy(&mut self) {
        self.queue.push_back(Change::Copy);
    }

    /// Looks for a link under `position` (window pixels) for the hover.
    pub fn find_link(&mut self, position: gpui::Point<Pixels>) {
        self.queue.push_back(Change::FindLink(position));
    }

    /// Opens the link under `position` (window pixels) when it is a web or mail link (D5).
    pub fn open_link(&mut self, position: gpui::Point<Pixels>) {
        self.queue.push_back(Change::OpenLink(position));
    }

    pub fn clear_hovered_word(&mut self) {
        self.hovered = None;
        self.hovered_word = None;
    }

    /// The link at `point` on the screen as it is now.
    pub fn link_at(&self, point: Point) -> Option<HoveredWord> {
        link_at(&self.term.lock_unfair(), point)
    }

    /// Every match of `search` in the history and the screen, found on the background pool.
    pub fn find_matches(&self, search: Search, cx: &App) -> Task<Vec<Match>> {
        let term = self.term.clone();
        let mut regex = search.0;
        cx.background_executor().spawn(async move {
            let term = term.lock();
            let start = Point::new(term.topmost_line(), Column(0));
            let end = Point::new(term.bottommost_line(), term.last_column());
            RegexIter::new(start, end, Direction::Right, &term, &mut regex).collect()
        })
    }

    /// Selects match `index` and scrolls it into view.
    pub fn activate_match(&mut self, index: usize) {
        if let Some(found) = self.matches.get(index).cloned() {
            let start = *found.start();
            self.set_selection(Some(simple(found)));
            self.queue.push_back(Change::ScrollToPoint(start));
        }
    }

    /// Runs the queued changes in their order, then copies the shown screen. Called before a
    /// paint, under the lock.
    pub fn sync(&mut self, cx: &mut Context<Self>) {
        let term = self.term.clone();
        let mut term = term.lock_unfair();
        while let Some(change) = self.queue.pop_front() {
            self.apply_change(change, &mut term, cx);
        }
        let before = &self.last_content;
        let grid = (before.columns, before.screen_lines);
        let (lines, offset) = (before.total_lines as i64, before.display_offset as i64);
        let mut content = copy_content(&term, self.bounds, None);
        drop(term);
        let added = content.total_lines as i64 - lines;
        let scrolled = content.display_offset as i64 - offset;
        if grid != (content.columns, content.screen_lines) || added != scrolled {
            // Other lines are under the pointer now: look for the link again.
            self.hovered_word = None;
            if let Some(position) = self.hovered {
                self.queue.push_back(Change::FindLink(position));
                cx.emit(TermEvent::Wakeup);
            }
        } else if added != 0
            && let Some(word) = self.hovered_word.as_mut()
        {
            // The view kept its lines while output came: the link moved into the history with
            // them.
            let shift = |point: Point| Point::new(point.line - added as i32, point.column);
            word.range = shift(*word.range.start())..=shift(*word.range.end());
        }
        content.last_hovered_word = self.hovered_word.clone();
        self.last_content = content;
    }

    fn apply_change(&mut self, change: Change, term: &mut Term<Listener>, cx: &mut Context<Self>) {
        match change {
            Change::Resize(bounds) => {
                term.resize(bounds);
                // Matches are worked out again on the new grid.
                if !self.matches.is_empty() {
                    cx.emit(TermEvent::Wakeup);
                }
            }
            Change::Clear => clear_keeping_the_cursor_line(term),
            Change::Scroll(scroll) => term.scroll_display(scroll),
            Change::ScrollToPoint(point) => term.scroll_to_point(point),
            Change::SetSelection(selection) => {
                term.selection = selection;
                write_primary(term, cx);
            }
            Change::UpdateSelection(position) => {
                let (point, side) = self.cell_at(term, position);
                if let Some(selection) = term.selection.as_mut() {
                    selection.update(point, side);
                }
                write_primary(term, cx);
            }
            Change::Copy => {
                if let Some(text) = term.selection_to_string() {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }
            Change::FindLink(position) => {
                self.hovered = Some(position);
                self.hovered_word = link_at(term, self.cell_at(term, position).0);
            }
            Change::OpenLink(position) => {
                if let Some(link) = link_at(term, self.cell_at(term, position).0)
                    && links::may_open(&link.word)
                {
                    crate::app::links::open_link(&link.word, cx);
                }
            }
        }
    }

    /// The grid cell under `position` (window pixels) and its side, with the display offset the
    /// screen has now.
    fn cell_at(&self, term: &Term<Listener>, position: gpui::Point<Pixels>) -> (Point, Side) {
        let (columns, lines) = (term.columns(), term.screen_lines());
        let (shown, side) = self.bounds.shown_cell(position, columns, lines);
        (viewport_to_point(term.grid().display_offset(), shown), side)
    }

    /// The events of one batch of the event loop, in order. Events of a screen that was
    /// replaced since they were sent are dropped here, when they would be applied.
    fn handle(&mut self, batch: Vec<(u64, Event)>, cx: &mut Context<Self>) {
        let current = self.generation.load(Ordering::Acquire);
        for (generation, event) in batch {
            if generation != current {
                continue;
            }
            match event {
                Event::Wakeup => cx.emit(TermEvent::Wakeup),
                Event::PtyWrite(text) => self.write_to_pty(text),
                Event::ColorRequest(index, answer) => {
                    let rgb = self.colour_at(index, cx);
                    self.write_to_pty(answer(rgb));
                }
                Event::TextAreaSizeRequest(answer) => {
                    let size = self.last_content.terminal_bounds.window_size();
                    self.write_to_pty(answer(size));
                }
                Event::CursorBlinkingChange => cx.emit(TermEvent::BlinkChanged),
                // No sound and nothing shown.
                Event::Bell => self.bell = true,
                Event::Title(title) => self.title = Some(title),
                Event::ResetTitle => self.title = None,
                // OSC 52 is off; the rest has no use here.
                Event::ClipboardStore(..)
                | Event::ClipboardLoad(..)
                | Event::MouseCursorDirty
                | Event::Exit
                | Event::ChildExit(_) => {}
            }
        }
    }

    /// The colour a program asks for: its own when it set one, else the theme's. The
    /// background is the terminal's own background, not the cells' default (T-Q1).
    fn colour_at(&self, index: usize, cx: &App) -> Rgb {
        if index < COUNT
            && let Some(rgb) = self.term.lock_unfair().colors()[index]
        {
            return rgb;
        }
        let theme = cx.theme();
        let colour = match u8::try_from(index) {
            Ok(index) => colors::convert(&Color::Indexed(index), theme),
            Err(_) => match NAMED.get(index - 256) {
                Some(NamedColor::Background) => theme.colors().terminal_background,
                Some(named) => colors::convert(&Color::Named(*named), theme),
                None => gpui::black(),
            },
        };
        premultiplied(colour)
    }
}

/// The colours after the 256 table, by index from 256.
const NAMED: [NamedColor; 13] = [
    NamedColor::Foreground,
    NamedColor::Background,
    NamedColor::Cursor,
    NamedColor::DimBlack,
    NamedColor::DimRed,
    NamedColor::DimGreen,
    NamedColor::DimYellow,
    NamedColor::DimBlue,
    NamedColor::DimMagenta,
    NamedColor::DimCyan,
    NamedColor::DimWhite,
    NamedColor::BrightForeground,
    NamedColor::DimForeground,
];

/// A colour with its alpha folded in, in 8 bit channels, cut down (T-Q1 shows `#e17079` as
/// `e1e1/6f6f/7979`).
fn premultiplied(colour: Hsla) -> Rgb {
    let rgba = Rgba::from(colour);
    let byte = |channel: f32| (channel * rgba.a * 255.) as u8;
    Rgb {
        r: byte(rgba.r),
        g: byte(rgba.g),
        b: byte(rgba.b),
    }
}

fn nanos(pace: Duration) -> u64 {
    u64::try_from(pace.as_nanos()).unwrap_or(u64::MAX)
}

/// Clears the history and the screen but keeps the cursor's line: it moves to the top row, and
/// the cursor stays in its column. The shell is not told, so its prompt stays in sight.
fn clear_keeping_the_cursor_line(term: &mut Term<Listener>) {
    term.clear_screen(ClearMode::Saved);
    let line = term.grid().cursor.point.line;
    let row = term.grid()[line].clone();
    let lines = term.screen_lines();
    let grid = term.grid_mut();
    grid[Line(0)] = row;
    grid.cursor.point.line = Line(0);
    if lines > 1 {
        grid.reset_region(Line(1)..);
    }
}

/// On Linux and FreeBSD a selection is also the primary selection.
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn write_primary(term: &Term<Listener>, cx: &App) {
    if let Some(text) = term.selection_to_string() {
        cx.write_to_primary(ClipboardItem::new_string(text));
    }
}

#[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
fn write_primary(_: &Term<Listener>, _: &App) {}

/// A plain selection over `range`, both ends included.
fn simple(range: RangeInclusive<Point>) -> Selection {
    let mut selection = Selection::new(SelectionType::Simple, *range.start(), Side::Left);
    selection.update(*range.end(), Side::Right);
    selection
}

fn copy_content(
    term: &Term<Listener>,
    bounds: TermBounds,
    hovered: Option<HoveredWord>,
) -> Content {
    let content = term.renderable_content();
    let cells = content
        .display_iter
        .map(|indexed| IndexedCell {
            point: indexed.point,
            cell: indexed.cell.clone(),
        })
        .collect();
    let screen_lines = term.screen_lines();
    let last = Line(i32::try_from(screen_lines).unwrap_or(i32::MAX) - 1);
    let grid = term.grid();
    let bottom_row_used = grid.cursor.point.line == last
        || (0..term.columns()).any(|column| grid[last][Column(column)].c != ' ');
    Content {
        cells,
        mode: content.mode,
        total_lines: term.total_lines(),
        display_offset: content.display_offset,
        columns: term.columns(),
        screen_lines,
        selection: content.selection,
        selection_text: term.selection_to_string(),
        cursor: content.cursor,
        cursor_char: grid[content.cursor.point].c,
        terminal_bounds: bounds,
        last_hovered_word: hovered,
        bottom_row_used,
        scrolled_to_top: content.display_offset == term.history_size(),
        scrolled_to_bottom: content.display_offset == 0,
    }
}

/// The link at `point`: an OSC 8 link of its cells (its whole run of cells on the line), else a
/// URL of its logical line (wrapped rows joined).
fn link_at(term: &Term<Listener>, point: Point) -> Option<HoveredWord> {
    let columns = term.columns();
    if point.line < term.topmost_line()
        || point.line > term.bottommost_line()
        || point.column.0 >= columns
    {
        return None;
    }
    let grid = term.grid();
    let row = &grid[point.line];
    if let Some(link) = row[point.column].hyperlink() {
        let same = |column: usize| row[Column(column)].hyperlink().as_ref() == Some(&link);
        let mut start = point.column.0;
        while start > 0 && same(start - 1) {
            start -= 1;
        }
        let mut end = point.column.0;
        while end + 1 < columns && same(end + 1) {
            end += 1;
        }
        return Some(HoveredWord {
            word: link.uri().to_owned(),
            range: Point::new(point.line, Column(start))..=Point::new(point.line, Column(end)),
        });
    }
    let last = Column(columns - 1);
    let wraps = |line: Line| grid[line][last].flags.contains(Flags::WRAPLINE);
    let mut top = point.line;
    while top > term.topmost_line() && wraps(Line(top.0 - 1)) {
        top = Line(top.0 - 1);
    }
    let mut bottom = point.line;
    while bottom < term.bottommost_line() && wraps(bottom) {
        bottom = Line(bottom.0 + 1);
    }
    let mut text = String::new();
    let mut cells = Vec::new();
    let mut at = None;
    for line in top.0..=bottom.0 {
        for column in 0..columns {
            let here = Point::new(Line(line), Column(column));
            let cell = &grid[here];
            if cell
                .flags
                .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
            {
                if here == point {
                    at = cells.len().checked_sub(1);
                }
                continue;
            }
            if here == point {
                at = Some(cells.len());
            }
            text.push(cell.c);
            cells.push(here);
        }
    }
    let (range, url) = links::url_at(&text, at?)?;
    let start = *cells.get(range.start)?;
    let end = *cells.get(range.end.checked_sub(1)?)?;
    Some(HoveredWord {
        word: url,
        range: start..=end,
    })
}

#[cfg(test)]
#[path = "model_tests.rs"]
pub(crate) mod tests;
