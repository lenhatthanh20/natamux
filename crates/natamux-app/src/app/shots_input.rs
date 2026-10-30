//! Input for the `--shot` states of the MIT move: keys, a pointer move, clicks, a drag and a wheel
//! turn, sent through the window's own dispatch as a user's would be. Root is not leased while
//! they go out, so a handler that updates it works.

use std::time::Duration;

use gpui::{
    AppContext as _, AsyncApp, Keystroke, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, PlatformInput, Point, ScrollDelta, ScrollWheelEvent, TouchPhase,
    WindowHandle, point,
};

use super::window::Root;

/// The end of the text in a box of many lines (`app/text_boxes.rs`).
pub const TEXT_END: &str = if cfg!(target_os = "macos") {
    "cmd-down"
} else {
    "ctrl-end"
};

pub async fn wait(time: Duration, cx: &mut AsyncApp) {
    cx.background_executor().timer(time).await;
}

/// Sends the events in order.
pub fn send(handle: WindowHandle<Root>, events: Vec<PlatformInput>, cx: &mut AsyncApp) {
    let sent = cx.update_window(handle.into(), |_, window, cx| {
        for event in events {
            window.dispatch_event(event, cx);
        }
    });
    if let Err(error) = sent {
        eprintln!("shot: the window is gone: {error}");
    }
}

/// Presses the keys in order, such as `shift-left`, at the focused element.
pub fn press(handle: WindowHandle<Root>, keys: &[&str], cx: &mut AsyncApp) {
    let pressed = cx.update_window(handle.into(), |_, window, cx| {
        for key in keys {
            match Keystroke::parse(key) {
                Ok(keystroke) => {
                    window.dispatch_keystroke(keystroke, cx);
                }
                Err(error) => eprintln!("shot: cannot make the key {key:?}: {error}"),
            }
        }
    });
    if let Err(error) = pressed {
        eprintln!("shot: the window is gone: {error}");
    }
}

pub fn pointer_to(position: Point<Pixels>) -> PlatformInput {
    PlatformInput::MouseMove(MouseMoveEvent {
        position,
        pressed_button: None,
        modifiers: Modifiers::default(),
    })
}

/// A move to `position`, then a press and a release of `button` there.
pub fn click(button: MouseButton, position: Point<Pixels>) -> Vec<PlatformInput> {
    let modifiers = Modifiers::default();
    vec![
        pointer_to(position),
        PlatformInput::MouseDown(MouseDownEvent {
            button,
            position,
            modifiers,
            click_count: 1,
            first_mouse: false,
        }),
        PlatformInput::MouseUp(MouseUpEvent {
            button,
            position,
            modifiers,
            click_count: 1,
        }),
    ]
}

/// A left press at `from`, a move in two steps with the button down, and the release at `to`.
pub fn drag(from: Point<Pixels>, to: Point<Pixels>) -> Vec<PlatformInput> {
    let modifiers = Modifiers::default();
    let button = MouseButton::Left;
    let middle = point((from.x + to.x) / 2., (from.y + to.y) / 2.);
    let held = |position| {
        PlatformInput::MouseMove(MouseMoveEvent {
            position,
            pressed_button: Some(button),
            modifiers,
        })
    };
    vec![
        pointer_to(from),
        PlatformInput::MouseDown(MouseDownEvent {
            button,
            position: from,
            modifiers,
            click_count: 1,
            first_mouse: false,
        }),
        held(middle),
        held(to),
        PlatformInput::MouseUp(MouseUpEvent {
            button,
            position: to,
            modifiers,
            click_count: 1,
        }),
    ]
}

/// A wheel turn of `lines` at `position`; a positive count goes up, into the history.
pub fn wheel(position: Point<Pixels>, lines: f32) -> Vec<PlatformInput> {
    vec![
        pointer_to(position),
        PlatformInput::ScrollWheel(ScrollWheelEvent {
            position,
            delta: ScrollDelta::Lines(point(0., lines)),
            modifiers: Modifiers::default(),
            touch_phase: TouchPhase::Moved,
        }),
    ]
}
