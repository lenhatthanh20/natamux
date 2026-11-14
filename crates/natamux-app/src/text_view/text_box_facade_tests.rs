//! The `TextBox` facade (plan 6.3.1, S3.1): each kind of box builds, reads and writes through the
//! same methods, and tells its owner `Edited` and `Blurred` as effects.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gpui::{Focusable, Subscription, TestAppContext, VisualTestContext};
use ui::prelude::*;

use super::text_box::{TextBox, TextEvent};
use crate::app::test_driver::{is_focused, type_text};
use crate::app::test_views;
use crate::components::form::{FILL_LINES, text_editor};

/// One box of each kind, and what each told the host.
struct Host {
    boxes: Vec<TextBox>,
    events: Vec<(usize, TextEvent)>,
    _subscriptions: Vec<Subscription>,
}

impl Host {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let boxes = vec![
            text_editor("one", None, window, cx),
            text_editor("area", Some((2, 4)), window, cx),
            text_editor("fill", Some(FILL_LINES), window, cx),
            TextBox::plain(false, window, cx),
        ];
        let subscriptions = boxes
            .iter()
            .enumerate()
            .map(|(index, text_box)| {
                text_box.subscribe(cx, move |host, _, event, _| {
                    host.events.push((index, event));
                })
            })
            .collect();
        Host {
            boxes,
            events: Vec::new(),
            _subscriptions: subscriptions,
        }
    }
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        v_flex().children(
            self.boxes
                .iter()
                .map(|text_box| div().w(px(300.)).h(px(80.)).child(text_box.clone())),
        )
    }
}

fn host(cx: &mut TestAppContext) -> (gpui::Entity<Host>, &mut VisualTestContext) {
    let (host, _, cx) = test_views::init_app(cx, |_, window, cx| cx.new(|cx| Host::new(window, cx)));
    (host, cx)
}

fn boxes(host: &gpui::Entity<Host>, cx: &mut VisualTestContext) -> Vec<TextBox> {
    host.read_with(cx, |host, _| host.boxes.clone())
}

fn focus(text_box: &TextBox, cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.focus(&text_box.focus_handle(cx), cx));
    cx.run_until_parked();
}

#[gpui::test]
fn every_kind_of_box_reads_and_writes_through_the_facade(cx: &mut TestAppContext) {
    let (host, cx) = host(cx);
    for text_box in boxes(&host, cx) {
        assert_eq!(cx.update(|_, cx| text_box.text(cx)), "");
        cx.update(|window, cx| text_box.set_text("abc", window, cx));
        assert_eq!(cx.update(|_, cx| text_box.text(cx)), "abc");
        cx.update(|window, cx| {
            text_box.move_to_end(window, cx);
            text_box.insert_at_caret("d", window, cx);
        });
        assert_eq!(cx.update(|_, cx| text_box.text(cx)), "abcd");
        cx.update(|window, cx| {
            text_box.select_all(window, cx);
            text_box.insert_at_caret("x", window, cx);
        });
        assert_eq!(cx.update(|_, cx| text_box.text(cx)), "x");
        cx.update(|window, cx| text_box.clear(window, cx));
        assert_eq!(cx.update(|_, cx| text_box.text(cx)), "");
    }
}

#[gpui::test]
fn a_masked_box_keeps_its_real_text(cx: &mut TestAppContext) {
    let (host, cx) = host(cx);
    let text_box = boxes(&host, cx).remove(0);
    cx.update(|window, cx| {
        text_box.set_masked(true, cx);
        text_box.set_text("secret", window, cx);
    });
    assert_eq!(cx.update(|_, cx| text_box.text(cx)), "secret");
    cx.update(|_, cx| text_box.set_masked(false, cx));
    assert_eq!(cx.update(|_, cx| text_box.text(cx)), "secret");
}

#[gpui::test]
fn the_focus_handle_is_the_one_that_takes_the_keys(cx: &mut TestAppContext) {
    let (host, cx) = host(cx);
    let all = boxes(&host, cx);
    for text_box in &all {
        assert!(!is_focused(text_box, cx));
        focus(text_box, cx);
        assert!(is_focused(text_box, cx));
        assert!(cx.update(|window, cx| text_box.is_focused(window, cx)));
        type_text(cx, "k");
        assert!(cx.update(|_, cx| text_box.text(cx)).ends_with('k'));
        cx.update(|window, cx| text_box.clear(window, cx));
    }
}

#[gpui::test]
fn edits_and_blur_come_as_events_of_the_box_that_had_them(cx: &mut TestAppContext) {
    let (host, cx) = host(cx);
    let all = boxes(&host, cx);
    focus(&all[0], cx);
    type_text(cx, "a");
    cx.update(|window, cx| all[1].set_text("area text", window, cx));
    cx.run_until_parked();
    focus(&all[1], cx);
    let events = host.read_with(cx, |host, _| host.events.clone());
    assert!(events.contains(&(0, TextEvent::Edited)), "{events:?}");
    assert!(events.contains(&(1, TextEvent::Edited)), "{events:?}");
    assert!(events.contains(&(0, TextEvent::Blurred)), "{events:?}");
    assert!(
        events.iter().all(|(index, event)| *index <= 1
            || *event != TextEvent::Edited),
        "{events:?}"
    );
}
