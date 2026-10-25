//! The focus stops a view draws, in the order it draws them: the Tab ring of a dialog whose
//! fields change with what is picked (Project settings). A view calls `begin` before it builds
//! its elements and `end` after; `input_box` and the rows push their handles in between.

use std::cell::RefCell;

use gpui::FocusHandle;

thread_local! {
    static DRAWN: RefCell<Option<Vec<FocusHandle>>> = const { RefCell::new(None) };
}

pub fn begin() {
    DRAWN.with(|drawn| *drawn.borrow_mut() = Some(Vec::new()));
}

/// A focus stop being drawn; nothing happens outside `begin` and `end`.
pub fn push(handle: &FocusHandle) {
    DRAWN.with(|drawn| {
        if let Some(stops) = drawn.borrow_mut().as_mut() {
            stops.push(handle.clone());
        }
    });
}

pub fn end() -> Vec<FocusHandle> {
    DRAWN.with(|drawn| drawn.borrow_mut().take().unwrap_or_default())
}
