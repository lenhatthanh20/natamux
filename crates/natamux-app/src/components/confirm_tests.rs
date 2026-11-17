//! `resolve` swaps the text, the label and the action of an open confirm (T-0307, T-0337).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::cell::Cell;
use std::rc::Rc;

use gpui::TestAppContext;

use super::{Confirm, ConfirmDialog};
use crate::app::test_views;
use crate::components::dialog::DialogView;

#[gpui::test]
fn resolve_swaps_the_text_and_the_action(cx: &mut TestAppContext) {
    test_views::init(cx);
    let runs = Rc::new(Cell::new(0));
    let first = runs.clone();
    let (dialog, cx) = cx.add_window_view(move |_, cx| ConfirmDialog {
        confirm: Confirm::new("Delete?", "Clean", "Delete task", move |_, _| {
            first.set(first.get() + 1)
        }),
        focus: cx.focus_handle(),
        cancel: cx.focus_handle(),
        submit: cx.focus_handle(),
    });

    // The main button works before any resolve.
    dialog.update_in(cx, |dialog, window, cx| dialog.submit(window, cx));
    assert_eq!(runs.get(), 1);

    let resolved = runs.clone();
    dialog.update(cx, |dialog, cx| {
        dialog.resolve("Dirty", "Delete anyway", move |_, _| resolved.set(10), cx)
    });
    dialog.read_with(cx, |dialog, _| {
        assert_eq!(dialog.confirm.description.as_ref(), "Dirty");
        assert_eq!(dialog.confirm.confirm_label.as_ref(), "Delete anyway");
    });
    dialog.update_in(cx, |dialog, window, cx| dialog.submit(window, cx));
    assert_eq!(runs.get(), 10);
}
