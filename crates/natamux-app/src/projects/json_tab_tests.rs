//! The JSON tab's Copy button and its editor box (T-0294).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gpui::{Entity, Modifiers, Pixels, TestAppContext, VisualTestContext};
use ui::prelude::*;

use super::JsonTab;
use crate::app::test_views;
use crate::stages::pipeline_editor::PipelineEditor;

/// Invalid JSON on purpose: the button copies the draft as shown.
const DRAFT: &str = "{\"stages\": [ broken";

fn open_tab(cx: &mut TestAppContext) -> (Entity<JsonTab>, &mut VisualTestContext) {
    test_views::init(cx);
    let (tab, cx) = cx.add_window_view(|window, cx| {
        let pipeline = cx.new(|_| PipelineEditor::new(DRAFT));
        JsonTab::new(pipeline, window, cx)
    });
    cx.run_until_parked();
    (tab, cx)
}

fn near(a: Pixels, b: Pixels) -> bool {
    (f32::from(a) - f32::from(b)).abs() < 0.5
}

#[gpui::test]
fn the_copy_button_copies_the_draft_also_when_it_cannot_be_read(cx: &mut TestAppContext) {
    let (_, cx) = open_tab(cx);
    let button = cx
        .debug_bounds("json-copy")
        .expect("the Copy button was not drawn");

    cx.simulate_click(button.center(), Modifiers::none());

    let copied = cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
    assert_eq!(copied.as_deref(), Some(DRAFT));
}

/// The editor draws its own scrollbar at its right edge: the editor must fill the box inside
/// its 1 px border on the right, the top and the bottom, so the bar runs along the border.
#[gpui::test]
fn the_editor_fills_the_box_but_its_left_inset(cx: &mut TestAppContext) {
    let (tab, cx) = open_tab(cx);
    let outer = cx.debug_bounds("json-box").expect("the box was not drawn");
    let inner = tab.read_with(cx, |tab, cx| tab.editor.read(cx).last_bounds().cloned());
    let inner = inner.expect("the editor was not drawn");
    let border = px(1.);
    assert!(
        near(inner.top(), outer.top() + border),
        "{inner:?} in {outer:?}"
    );
    assert!(
        near(inner.bottom(), outer.bottom() - border),
        "{inner:?} in {outer:?}"
    );
    assert!(
        near(inner.right(), outer.right() - border),
        "{inner:?} in {outer:?}"
    );
    assert!(
        inner.left() > outer.left() + border,
        "no left inset: {inner:?} in {outer:?}"
    );
}
