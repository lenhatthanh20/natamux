//! The view of an own terminal (docs/plan-mit-license.md 6.2.6): the keys, the mouse, the wheel,
//! the cursor blink, the input method's text, the menu and the scrollbar of one screen. It holds
//! no screen state: the model does, and the element draws the copy the model's last sync made.
//! It tells the model what the user did and never calls the code that owns it (DT17).

use std::path::Path;
use std::time::{Duration, Instant};

use alacritty_terminal::index::{Column, Line, Point as GridPoint, Side};
use alacritty_terminal::selection::SelectionType;
use alacritty_terminal::term::TermMode;
use gpui::{
    Bounds, ClipboardEntry, DismissEvent, Entity, FocusHandle, Focusable, KeyDownEvent,
    ModifiersChangedEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    Point, ScrollWheelEvent, Subscription, TouchPhase, anchored, deferred,
};
use ui::{ContextMenu, prelude::*};

use super::element::TermElement;
use super::input_bytes::encode;
use super::links;
use super::model::{HoveredWord, TermEvent, TermModel};
use super::mouse_report::{MouseAction, report, wheel_report};
use super::scrollbar::{self, Show};
use crate::app::appearance::Inputs;
use crate::app::appearance_rules::shows_scrollbars;

/// One phase of a blinking cursor.
const BLINK: Duration = Duration::from_millis(500);
/// A press becomes a drag past this distance.
const DRAG_START: f32 = 2.;
/// The pointer looks for a link again after a move this far, or this long after the last look.
const HOVER_MOVE: f32 = 5.;
const HOVER_WAIT: Duration = Duration::from_millis(100);
/// A drag outside the screen scrolls at most this many lines per move.
const DRAG_SCROLL_MAX: usize = 3;

/// The cursor's blink: on only while the view has the focus and the program asked for it.
#[derive(Default)]
struct Blink {
    enabled: bool,
    /// The phase of the blink; true when the cursor is not blinking.
    visible: bool,
    /// A newer start or pause ends the timers of the older ones.
    epoch: usize,
}

/// A press of a mouse button in the view, until its release.
struct Press {
    position: Point<Pixels>,
    button: MouseButton,
    clicks: usize,
    shift: bool,
    /// A press that went to the program as a report.
    reported: bool,
    /// A press with the secondary key on a link: no selection and no reports while it lasts,
    /// and a release on the same link opens it.
    link: Option<HoveredWord>,
    dragged: bool,
}

struct OpenMenu {
    menu: Entity<ContextMenu>,
    position: Point<Pixels>,
    _dismiss: Subscription,
}

/// Where the element laid out the screen at the last frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Laid {
    pub bounds: Bounds<Pixels>,
    pub track: Bounds<Pixels>,
    pub thumb: Option<Bounds<Pixels>>,
    pub cell_width: Pixels,
    /// The cursor's rect, for the input method.
    pub cursor: Option<Bounds<Pixels>>,
}

/// The scrollbar's state.
#[derive(Default)]
pub struct Bar {
    /// When a scroll or a new size last showed it.
    pub shown_at: Option<Instant>,
    pub hovered: bool,
    /// The pointer's distance from the thumb's top while the thumb is dragged.
    pub grab: Option<Pixels>,
    /// The lines of the history, the screen and the offset at the last frame.
    pub seen: (usize, usize, usize),
}

pub struct TermView {
    model: Entity<TermModel>,
    focus: FocusHandle,
    /// The focus is on the view or inside it, such as on its menu.
    focused: bool,
    blink: Blink,
    /// The text an input method is composing, drawn at the cursor.
    pub(super) marked: Option<String>,
    menu: Option<OpenMenu>,
    press: Option<Press>,
    /// Wheel pixels not yet a whole line.
    wheel: Pixels,
    /// The cell and side of the last move: a move is reported only when one of them changed.
    reported: Option<(i32, usize, Side)>,
    /// Where and when the pointer last looked for a link.
    hover_look: Option<(Point<Pixels>, Instant)>,
    pub(super) bar: Bar,
    pub(crate) laid: Option<Laid>,
    _subscriptions: Vec<Subscription>,
}

impl TermView {
    pub fn new(model: Entity<TermModel>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        let subscriptions = vec![
            cx.subscribe_in(&model, window, Self::model_event),
            cx.observe(&model, |_, _, cx| cx.notify()),
            cx.on_focus_in(&focus, window, Self::focus_in),
            // The focus back on the view itself closes its menu.
            cx.on_focus(&focus, window, |view, _, cx| {
                if view.menu.take().is_some() {
                    cx.notify();
                }
            }),
            cx.on_focus_out(&focus, window, |view, _, _, cx| view.focus_out(cx)),
            // A new font measures the cells again, so the shell gets the new grid size.
            cx.observe_global::<crate::app::appearance::Appearance>(|_, cx| cx.notify()),
        ];
        TermView {
            model,
            focus,
            focused: false,
            blink: Blink {
                visible: true,
                ..Blink::default()
            },
            marked: None,
            menu: None,
            press: None,
            wheel: px(0.),
            reported: None,
            hover_look: None,
            bar: Bar::default(),
            laid: None,
            _subscriptions: subscriptions,
        }
    }

    /// When the scrollbar shows: as the settings of a shot say, else for a moment after a scroll.
    pub(super) fn show_scrollbar(cx: &App) -> Show {
        match cx.try_global::<Inputs>().and_then(|inputs| inputs.shot) {
            Some(state) if shows_scrollbars(state) => Show::Always,
            Some(_) => Show::Never,
            None => Show::Auto,
        }
    }

    fn model_event(
        &mut self,
        _: &Entity<TermModel>,
        event: &TermEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            TermEvent::Wakeup | TermEvent::OutputReplaced => {
                if self.focus.is_focused(window) {
                    window.invalidate_character_coordinates();
                }
            }
            TermEvent::BlinkChanged => self.follow_blink(cx),
        }
        cx.notify();
    }

    fn focus_in(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.focused = true;
        self.model.update(cx, |model, _| model.focus_in());
        self.follow_blink(cx);
        window.invalidate_character_coordinates();
        cx.notify();
    }

    fn focus_out(&mut self, cx: &mut Context<Self>) {
        self.focused = false;
        self.model.update(cx, |model, _| model.focus_out());
        self.blink_off();
        cx.notify();
    }

    /// The blink runs while the view has the focus and the program asked for a blinking cursor.
    /// A blink that runs goes on in its phases.
    fn follow_blink(&mut self, cx: &mut Context<Self>) {
        if self.focused && self.model.read(cx).cursor_blinks() {
            if !self.blink.enabled {
                self.blink.enabled = true;
                self.blink.visible = true;
                self.blink.epoch += 1;
                self.next_blink(cx);
            }
        } else {
            self.blink_off();
        }
    }

    fn blink_off(&mut self) {
        self.blink.enabled = false;
        self.blink.visible = true;
        self.blink.epoch += 1;
    }

    /// A key shows the cursor; the blink starts again one phase later.
    fn pause_blink(&mut self, cx: &mut Context<Self>) {
        self.blink.visible = true;
        self.blink.epoch += 1;
        if self.blink.enabled {
            self.next_blink(cx);
        }
    }

    fn next_blink(&mut self, cx: &mut Context<Self>) {
        let epoch = self.blink.epoch;
        cx.spawn(async move |view, cx| {
            cx.background_executor().timer(BLINK).await;
            view.update(cx, |view, cx| {
                if view.blink.epoch == epoch && view.blink.enabled {
                    view.blink.visible = !view.blink.visible;
                    cx.notify();
                    view.next_blink(cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// The cursor shows when the view is not `focused`, on the alternate screen, when the program
    /// asked for no blink, and in the shown half of a blink.
    pub fn should_show_cursor(&self, focused: bool, cx: &App) -> bool {
        let model = self.model.read(cx);
        !focused
            || model.last_content().mode.contains(TermMode::ALT_SCREEN)
            || !model.cursor_blinks()
            || self.blink.visible
    }

    fn key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.pause_blink(cx);
        self.model.update(cx, |model, _| model.clear_bell());
        cx.notify();
        // The input method types this key's text.
        if event.prefer_character_input && event.keystroke.key_char.is_some() {
            return;
        }
        // The mode of the last frame (6.2.5): output not drawn yet does not count.
        let mode = self.model.read(cx).last_content().mode;
        if let Some(bytes) = encode(&event.keystroke, mode) {
            self.model.update(cx, |model, _| model.type_bytes(bytes));
            cx.stop_propagation();
        }
    }

    /// Text the input method committed: it goes to the shell as typed.
    pub(super) fn commit_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        self.pause_blink(cx);
        if !text.is_empty() {
            let bytes = text.as_bytes().to_vec();
            self.model.update(cx, |model, _| model.type_bytes(bytes));
        }
        window.invalidate_character_coordinates();
        cx.notify();
    }

    /// Text the input method is composing; it reaches the shell only once committed.
    pub(super) fn set_marked(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.marked = (!text.is_empty()).then(|| text.to_owned());
        window.invalidate_character_coordinates();
        cx.notify();
    }

    fn copy(&mut self, cx: &mut Context<Self>) {
        self.model.update(cx, |model, _| model.copy());
        cx.notify();
    }

    /// The clipboard as typed text. With `any`, an image sends Ctrl+V for the program to read it
    /// itself, and files send their quoted paths; else only text pastes.
    fn paste_clipboard(&mut self, any: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = cx.read_from_clipboard() else {
            return;
        };
        if any {
            match item.entries().first() {
                Some(ClipboardEntry::Image(image)) if !image.bytes.is_empty() => {
                    self.model
                        .update(cx, |model, _| model.type_bytes(vec![0x16]));
                    cx.notify();
                    return;
                }
                Some(ClipboardEntry::ExternalPaths(paths)) => {
                    let mut text: String = paths
                        .paths()
                        .iter()
                        .map(|path| format!(" {}", quoted(path)))
                        .collect();
                    text.push(' ');
                    self.model.update(cx, |model, _| model.paste(&text));
                    window.focus(&self.focus, cx);
                    cx.notify();
                    return;
                }
                _ => {}
            }
        }
        if let Some(text) = item.text().filter(|text| !text.is_empty()) {
            self.model.update(cx, |model, _| model.paste(&text));
            cx.notify();
        }
    }

    fn select_all(&mut self, cx: &mut Context<Self>) {
        self.model.update(cx, |model, _| model.select_all());
        cx.notify();
    }

    fn clear(&mut self, cx: &mut Context<Self>) {
        self.model.update(cx, |model, _| model.clear());
        cx.notify();
    }

    /// The menu of a right click, at `position` (window pixels). Copy and Paste send the bound
    /// actions, so their rows show the keys bound in the terminal (6.2.6).
    pub fn deploy_context_menu(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let focus = self.focus.clone();
        let view = cx.entity().downgrade();
        let menu = ContextMenu::build(window, cx, move |menu, _, _| {
            let run = |act: fn(&mut TermView, &mut Window, &mut Context<TermView>)| {
                let view = view.clone();
                move |window: &mut Window, cx: &mut App| {
                    view.update(cx, |view, cx| act(view, window, cx)).ok();
                }
            };
            menu.context(focus)
                .action("Copy", Box::new(terminal::Copy))
                .action("Paste", Box::new(terminal::Paste))
                .entry(
                    "Paste Text",
                    None,
                    run(|view, window, cx| view.paste_clipboard(false, window, cx)),
                )
                .entry("Select All", None, run(|view, _, cx| view.select_all(cx)))
                .entry("Clear", None, run(|view, _, cx| view.clear(cx)))
        });
        let dismiss = cx.subscribe_in(&menu, window, |view, menu, _: &DismissEvent, window, cx| {
            if menu.focus_handle(cx).contains_focused(window, cx) {
                window.focus(&view.focus, cx);
            }
            view.menu = None;
            cx.notify();
        });
        window.focus(&menu.focus_handle(cx), cx);
        self.menu = Some(OpenMenu {
            menu,
            position,
            _dismiss: dismiss,
        });
        cx.notify();
    }

    /// The grid cell (line, column) under `position` and its side, from the grid of the last
    /// frame; lines of the history above the screen are negative.
    fn cell_at(&self, position: Point<Pixels>, cx: &App) -> (i32, usize, Side) {
        let content = self.model.read(cx).last_content();
        let (line, column, side) =
            content
                .terminal_bounds
                .shown_cell(position, content.columns, content.screen_lines);
        (line - content.display_offset as i32, column.0, side)
    }

    /// The grid point of the cell corner nearest `position`, as the old view found an OSC 8 cell
    /// for a plain click.
    fn nearest_cell(&self, position: Point<Pixels>, cx: &App) -> GridPoint {
        let content = self.model.read(cx).last_content();
        let grid = content.terminal_bounds;
        let from = position - grid.bounds.origin;
        let column = (f32::from(from.x) / f32::from(grid.cell_width))
            .round()
            .max(0.) as usize;
        let line = (f32::from(from.y) / f32::from(grid.line_height))
            .round()
            .max(0.) as usize;
        let column = column.min(content.columns.saturating_sub(1));
        let line = line.min(content.screen_lines.saturating_sub(1)) as i32;
        GridPoint::new(Line(line - content.display_offset as i32), Column(column))
    }

    /// The link of the cell under `position`: an OSC 8 link, else a URL of its line.
    fn link_under(&self, position: Point<Pixels>, cx: &App) -> Option<HoveredWord> {
        let (line, column, _) = self.cell_at(position, cx);
        self.model
            .read(cx)
            .link_at(GridPoint::new(Line(line), Column(column)))
    }

    fn mouse_mode(&self, shift: bool, cx: &App) -> Option<TermMode> {
        let mode = self.model.read(cx).last_content().mode;
        (mode.intersects(TermMode::MOUSE_MODE) && !shift).then_some(mode)
    }

    fn send_report(
        &mut self,
        action: MouseAction,
        event_at: Point<Pixels>,
        mods: &gpui::Modifiers,
        mode: TermMode,
        cx: &mut Context<Self>,
    ) {
        let (line, column, _) = self.cell_at(event_at, cx);
        if let Some(bytes) = report(action, line, column, mods, mode) {
            self.model.update(cx, |model, _| model.write_to_pty(bytes));
        }
    }

    pub(super) fn mouse_down(
        &mut self,
        event: &MouseDownEvent,
        over: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button == MouseButton::Left && self.bar_press(event.position, cx) {
            cx.stop_propagation();
            cx.notify();
            return;
        }
        // The rest of the scrollbar's track takes the press and does nothing, not even focus.
        if !over {
            cx.stop_propagation();
            return;
        }
        let mouse_mode = self.mouse_mode(event.modifiers.shift, cx);
        if matches!(event.button, MouseButton::Left | MouseButton::Middle) || mouse_mode.is_some() {
            window.focus(&self.focus, cx);
        }
        // The secondary key on a link: the link wins over selection and mouse reports.
        let link = if event.button == MouseButton::Left && event.modifiers.secondary() {
            self.link_under(event.position, cx)
        } else {
            None
        };
        let mut press = Press {
            position: event.position,
            button: event.button,
            clicks: event.click_count,
            shift: event.modifiers.shift,
            reported: false,
            link: None,
            dragged: false,
        };
        if link.is_some() {
            press.link = link;
            self.press = Some(press);
            cx.notify();
            return;
        }
        if let Some(mode) = mouse_mode {
            let action = MouseAction::Press(event.button);
            self.send_report(action, event.position, &event.modifiers, mode, cx);
            press.reported = true;
            self.press = Some(press);
            return;
        }
        match event.button {
            MouseButton::Left => self.select_from(event, cx),
            MouseButton::Right => {
                if self.model.read(cx).last_content().selection.is_none() {
                    let at = event.position;
                    self.model.update(cx, |model, _| {
                        model.start_selection(SelectionType::Semantic, at)
                    });
                }
                self.deploy_context_menu(event.position, window, cx);
                // The view's own focus on a press would take the focus back from the menu.
                cx.stop_propagation();
            }
            MouseButton::Middle => self.paste_primary(cx),
            MouseButton::Navigate(_) => {}
        }
        self.press = Some(press);
        cx.notify();
    }

    /// A left press: one click clears the selection (Shift extends it, or starts one), two
    /// select a word, three a line; more do nothing.
    fn select_from(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        let at = event.position;
        let selected = self.model.read(cx).last_content().selection.is_some();
        self.model.update(cx, |model, _| match event.click_count {
            1 if event.modifiers.shift && selected => model.update_selection(at),
            1 if event.modifiers.shift => model.start_selection(SelectionType::Simple, at),
            1 => model.set_selection(None),
            2 => model.start_selection(SelectionType::Semantic, at),
            3 => model.start_selection(SelectionType::Lines, at),
            _ => {}
        });
    }

    /// On Linux and FreeBSD a middle click that is not a report pastes the primary selection.
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    fn paste_primary(&mut self, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_primary().and_then(|item| item.text()) {
            self.model.update(cx, |model, _| model.paste(&text));
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
    fn paste_primary(&mut self, _: &mut Context<Self>) {}

    pub(super) fn mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        over: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.bar_move(event.position, cx) {
            return;
        }
        self.look_for_link(event.position, &event.modifiers, over, cx);
        // A press on a link takes the moves until its release.
        if self
            .press
            .as_ref()
            .is_some_and(|press| press.link.is_some())
        {
            return;
        }
        if let Some(mode) = self.mouse_mode(event.modifiers.shift, cx) {
            if over {
                let cell = self.cell_at(event.position, cx);
                if self.reported != Some(cell) {
                    self.reported = Some(cell);
                    let action = MouseAction::Move(event.pressed_button);
                    self.send_report(action, event.position, &event.modifiers, mode, cx);
                }
            }
            return;
        }
        self.drag(event, over, window, cx);
    }

    /// With the secondary key held, the link under the pointer shows; not while selecting.
    fn look_for_link(
        &mut self,
        position: Point<Pixels>,
        mods: &gpui::Modifiers,
        over: bool,
        cx: &mut Context<Self>,
    ) {
        let selecting = self.press.as_ref().is_some_and(|press| press.dragged);
        if !mods.secondary() || selecting || !over {
            let shown = self
                .model
                .read(cx)
                .last_content()
                .last_hovered_word
                .is_some();
            if shown || self.hover_look.is_some() {
                self.hover_look = None;
                self.model.update(cx, |model, _| model.clear_hovered_word());
                cx.notify();
            }
            return;
        }
        // The executor's clock, which tests move by hand.
        let now = cx.background_executor().now();
        let due = self.hover_look.is_none_or(|(at, when)| {
            distance(at, position) > HOVER_MOVE || now.saturating_duration_since(when) >= HOVER_WAIT
        });
        if due {
            self.hover_look = Some((position, now));
            self.model.update(cx, |model, _| model.find_link(position));
            cx.notify();
        }
    }

    pub(super) fn modifiers_changed(
        &mut self,
        event: &ModifiersChangedEvent,
        over: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.hover_look = None;
        let position = window.mouse_position();
        self.look_for_link(position, &event.modifiers, over, cx);
    }

    /// A left drag selects once it moved past a few pixels over the screen; then outside the
    /// screen it scrolls.
    fn drag(
        &mut self,
        event: &MouseMoveEvent,
        over: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.pressed_button != Some(MouseButton::Left)
            || !self.focus.is_focused(window)
            || cx.has_active_drag()
        {
            return;
        }
        let Some(press) = self.press.as_mut() else {
            return;
        };
        if press.button != MouseButton::Left || press.link.is_some() || press.reported {
            return;
        }
        if !press.dragged {
            if !over || distance(press.position, event.position) <= DRAG_START {
                return;
            }
            press.dragged = true;
            if press.clicks == 1 && !press.shift {
                let from = press.position;
                self.model.update(cx, |model, _| {
                    model.start_selection(SelectionType::Simple, from)
                });
            }
        }
        let content = self.model.read(cx).last_content();
        let line_height = content.terminal_bounds.line_height;
        let alt_screen = content.mode.contains(TermMode::ALT_SCREEN);
        let at = event.position;
        let scroll = self.laid.filter(|_| !alt_screen).and_then(|laid| {
            let bounds = laid.bounds;
            if at.y < bounds.top() {
                Some(drag_lines(bounds.top() - at.y, line_height) as i32)
            } else if at.y > bounds.bottom() {
                Some(-(drag_lines(at.y - bounds.bottom(), line_height) as i32))
            } else {
                None
            }
        });
        // The selection moves first, then the screen scrolls, so the end is the cell under the
        // pointer before the scroll.
        self.model.update(cx, |model, _| {
            model.update_selection(at);
            match scroll {
                Some(lines) if lines > 0 => model.scroll_up_by(lines as usize),
                Some(lines) if lines < 0 => model.scroll_down_by(lines.unsigned_abs() as usize),
                _ => {}
            }
        });
        cx.notify();
    }

    /// A release: `over` when the pointer is over the screen (the element less the track).
    pub(super) fn mouse_up(
        &mut self,
        event: &MouseUpEvent,
        over: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.bar.grab.take().is_some() {
            cx.notify();
            return;
        }
        // The next move is reported, even in the same cell.
        self.reported = None;
        let press = self.press.take();
        // Off the screen a release opens no link and sends no report.
        if !over {
            return;
        }
        if let Some(link) = press.as_ref().and_then(|press| press.link.as_ref()) {
            if self.link_under(event.position, cx).as_ref() == Some(link)
                && links::may_open(&link.word)
            {
                crate::app::links::open_link(&link.word, cx);
            }
            return;
        }
        if let Some(mode) = self.mouse_mode(event.modifiers.shift, cx) {
            if self.focus.is_focused(window) {
                let action = MouseAction::Release(event.button);
                self.send_report(action, event.position, &event.modifiers, mode, cx);
            }
            return;
        }
        let Some(press) = press.filter(|press| !press.reported) else {
            return;
        };
        if press.button == MouseButton::Left && !press.dragged && !event.modifiers.secondary() {
            // D5: a left release with no drag on an OSC 8 cell opens it, only a web or mail
            // link.
            let point = self.nearest_cell(event.position, cx);
            if let Some(url) = self.model.read(cx).hyperlink_at(point)
                && links::may_open(&url)
            {
                crate::app::links::open_link(&url, cx);
            }
        }
    }

    /// The wheel and the trackpad: whole lines of the pixels the gesture added up. A program
    /// that takes the mouse gets wheel reports; the alternate screen gets arrow keys; else the
    /// screen scrolls.
    pub(super) fn scroll_wheel(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        match event.touch_phase {
            TouchPhase::Started => {
                self.wheel = px(0.);
                return;
            }
            TouchPhase::Moved => {}
            _ => return,
        }
        let content = self.model.read(cx).last_content();
        let line_height = content.terminal_bounds.line_height;
        let mode = content.mode;
        let screen = content.terminal_bounds.bounds.size.height;
        let old = self.wheel;
        let new = old + event.delta.pixel_delta(line_height).y;
        let lines = (new / line_height) as i32 - (old / line_height) as i32;
        self.wheel = if screen > px(0.) {
            px(f32::from(new) % f32::from(screen))
        } else {
            new
        };
        if lines == 0 {
            return;
        }
        let shift = event.modifiers.shift;
        let count = lines.unsigned_abs() as usize;
        if mode.intersects(TermMode::MOUSE_MODE) && !shift {
            let (line, column, _) = self.cell_at(event.position, cx);
            if let Some(bytes) = wheel_report(lines > 0, line, column, &event.modifiers, mode) {
                let bytes = bytes.repeat(count);
                self.model.update(cx, |model, _| model.write_to_pty(bytes));
            }
        } else if mode.contains(TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL) && !shift {
            let arrow: &[u8] = if lines > 0 { b"\x1bOA" } else { b"\x1bOB" };
            let bytes = arrow.repeat(count);
            self.model.update(cx, |model, _| model.write_to_pty(bytes));
        } else {
            self.model.update(cx, |model, _| {
                if lines > 0 {
                    model.scroll_up_by(count)
                } else {
                    model.scroll_down_by(count)
                }
            });
            self.show_bar(cx);
        }
        cx.notify();
    }

    /// The scrollbar shows for a moment (when it shows only then), and draws again as it fades.
    pub(super) fn show_bar(&mut self, cx: &mut Context<Self>) {
        if Self::show_scrollbar(cx) != Show::Auto {
            return;
        }
        let at = cx.background_executor().now();
        self.bar.shown_at = Some(at);
        cx.spawn(async move |view, cx| {
            cx.background_executor().timer(scrollbar::SHOWN_FOR).await;
            view.update(cx, |view, cx| {
                if view.bar.shown_at == Some(at) {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// A left press on the thumb starts its drag; one elsewhere in the track moves the thumb's
    /// middle to the pointer, with no drag.
    fn bar_press(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) -> bool {
        let Some(laid) = self.laid else {
            return false;
        };
        let Some(thumb) = laid.thumb else {
            return false;
        };
        if Self::show_scrollbar(cx) == Show::Never || !laid.track.contains(&position) {
            return false;
        }
        if thumb.contains(&position) {
            self.bar.grab = Some(position.y - thumb.top());
            return true;
        }
        self.scroll_bar_to(laid, thumb, position.y - thumb.size.height / 2., cx);
        true
    }

    /// The thumb follows a drag; else the pointer may hover it.
    fn bar_move(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) -> bool {
        let Some(laid) = self.laid else {
            return false;
        };
        // A hidden thumb is hovered too, and that shows it.
        let hovered = Self::show_scrollbar(cx) != Show::Never
            && laid.thumb.is_some_and(|thumb| thumb.contains(&position));
        if hovered != self.bar.hovered {
            self.bar.hovered = hovered;
            cx.notify();
        }
        let (Some(grab), Some(thumb)) = (self.bar.grab, laid.thumb) else {
            return false;
        };
        self.scroll_bar_to(laid, thumb, position.y - grab, cx);
        true
    }

    /// Scrolls so the thumb's top is at `top`.
    fn scroll_bar_to(
        &mut self,
        laid: Laid,
        thumb: Bounds<Pixels>,
        top: Pixels,
        cx: &mut Context<Self>,
    ) {
        let content = self.model.read(cx).last_content();
        let (total, screen, now) = (
            content.total_lines,
            content.screen_lines,
            content.display_offset,
        );
        let wanted = scrollbar::offset_at(laid.track, thumb.size.height, top, total, screen);
        self.model.update(cx, |model, _| {
            if wanted > now {
                model.scroll_up_by(wanted - now);
            } else if wanted < now {
                model.scroll_down_by(now - wanted);
            }
        });
        self.show_bar(cx);
        cx.notify();
    }

    /// How much of the scrollbar shows now.
    pub(super) fn bar_alpha(&self, cx: &App) -> f32 {
        match Self::show_scrollbar(cx) {
            Show::Never => 0.,
            Show::Always => 1.,
            Show::Auto if self.bar.grab.is_some() || self.bar.hovered => 1.,
            Show::Auto => self.bar.shown_at.map_or(0., |at| {
                let now = cx.background_executor().now();
                scrollbar::auto_alpha(now.saturating_duration_since(at))
            }),
        }
    }

    /// Whether the scrollbar shows in or fades out now, so frames must keep coming.
    pub(super) fn bar_fading(&self, cx: &App) -> bool {
        Self::show_scrollbar(cx) == Show::Auto
            && self.bar.grab.is_none()
            && !self.bar.hovered
            && self.bar.shown_at.is_some_and(|at| {
                let now = cx.background_executor().now();
                scrollbar::auto_fading(now.saturating_duration_since(at))
            })
    }
}

/// The lines a drag this far outside the screen scrolls per move.
fn drag_lines(distance: Pixels, line_height: Pixels) -> usize {
    let lines = (f32::from(distance).powf(1.1) / f32::from(line_height)).ceil();
    (lines.max(0.) as usize).min(DRAG_SCROLL_MAX)
}

fn distance(a: Point<Pixels>, b: Point<Pixels>) -> f32 {
    let (dx, dy) = (f32::from(a.x - b.x), f32::from(a.y - b.y));
    (dx * dx + dy * dy).sqrt()
}

/// A path as the shell reads it: quoted when it has spaces or quotes.
fn quoted(path: &Path) -> String {
    let text = path.to_string_lossy();
    shlex::try_quote(&text).map_or_else(|_| text.to_string(), |quoted| quoted.into_owned())
}

impl Focusable for TermView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for TermView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let menu = self.menu.as_ref().map(|open| {
            deferred(
                anchored()
                    .position(open.position)
                    .anchor(gpui::Anchor::TopLeft)
                    .child(open.menu.clone()),
            )
            .with_priority(1)
        });
        div()
            .key_context("Terminal")
            .track_focus(&self.focus)
            .size_full()
            .on_key_down(cx.listener(Self::key_down))
            .on_action(cx.listener(|view, _: &terminal::Copy, _, cx| view.copy(cx)))
            .on_action(cx.listener(|view, _: &terminal::Paste, window, cx| {
                view.paste_clipboard(true, window, cx)
            }))
            .child(TermElement::new(
                cx.entity(),
                self.model.clone(),
                self.focus.clone(),
            ))
            .children(menu)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drag_far_outside_scrolls_at_most_three_lines() {
        assert_eq!(drag_lines(px(1.), px(16.9)), 1);
        assert_eq!(drag_lines(px(40.), px(16.9)), 3);
        assert_eq!(drag_lines(px(400.), px(16.9)), 3);
    }

    #[test]
    fn a_path_is_quoted_as_the_shell_reads_it() {
        assert_eq!(quoted(Path::new("/tmp/a b.txt")), "'/tmp/a b.txt'");
        assert_eq!(quoted(Path::new("/tmp/it's.md")), "\"/tmp/it's.md\"");
        assert_eq!(quoted(Path::new("/tmp/plain")), "/tmp/plain");
    }
}
