#![allow(clippy::unwrap_used, clippy::expect_used)]

use gpui::{
    Entity, Modifiers, MouseButton, MouseDownEvent, TestAppContext, VisualTestContext, point,
};
use ui::prelude::*;

use super::super::{DocState, MarkdownDoc, Pick, Unit};
use crate::app::test_views;
use crate::components::menu::MenuLayer;

struct Host {
    doc: MarkdownDoc,
}

impl Render for Host {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.doc.view(window, cx))
    }
}

/// A doc of one paragraph, drawn, with `Hello` picked by the mouse.
fn open(cx: &mut TestAppContext) -> (Entity<DocState>, &mut VisualTestContext) {
    let (host, _, cx) = test_views::init_app(cx, |_, _, cx| {
        let doc = MarkdownDoc::new(cx);
        doc.set_text("Hello world", cx);
        cx.new(|_| Host { doc })
    });
    let state = host.read_with(cx, |host, _| host.doc.state.clone());
    state.update_in(cx, |state, window, cx| {
        state.pick = Pick::new(0..5, Unit::Char);
        state.pick.held = false;
        window.focus(&state.focus, cx);
        cx.notify();
    });
    cx.run_until_parked();
    (state, cx)
}

/// T-0350: the selection was look only (D13), no key copied it.
#[gpui::test]
fn mod_c_copies_the_mouse_selection(cx: &mut TestAppContext) {
    let (_, cx) = open(cx);
    let mod_c = if cfg!(target_os = "macos") {
        "cmd-c"
    } else {
        "ctrl-c"
    };
    cx.simulate_keystrokes(mod_c);
    let copied = cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
    assert_eq!(copied.as_deref(), Some("Hello"));
}

#[gpui::test]
fn a_right_press_opens_the_copy_menu_and_keeps_the_selection(cx: &mut TestAppContext) {
    let (state, cx) = open(cx);
    // Inside the first line, past the view's 16 px padding.
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Right,
        position: point(px(24.), px(24.)),
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    cx.run_until_parked();
    assert!(cx.update(|_, cx| MenuLayer::open_menu(cx)).is_some());
    let picked = state.read_with(cx, |state, _| state.pick.selected());
    assert_eq!(picked, Some(0..5));
}
