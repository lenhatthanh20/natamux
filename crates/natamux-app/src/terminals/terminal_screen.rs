//! The screen of one terminal, as the panes and the tools show it: Zed's view, drawn again only
//! when it changed, and a look at its grid size after each layout (no timer per terminal).

use gpui::{Entity, StyleRefinement, canvas};
use ui::prelude::*;

use super::remote_terminal::RemoteTerminal;

/// `ended` dims the screen. The cache of a view does not know the opacity around it, so an ended
/// screen draws uncached.
pub fn terminal_screen(terminal: &Entity<RemoteTerminal>, ended: bool, cx: &App) -> Div {
    let view = terminal.read(cx).view.clone();
    let style = StyleRefinement::default().size_full();
    let view = if ended {
        view.into_any_element()
    } else {
        view.cached(style).into_any_element()
    };
    let watched = terminal.clone();
    // After the view, so it runs once the view's layout told the terminal its grid size.
    let watch = canvas(
        move |_, window, cx| {
            if watched.read(cx).layout_changed(cx) {
                window.defer(cx, move |window, cx| {
                    watched.update(cx, |terminal, cx| terminal.laid_out(window, cx));
                });
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_full();
    div()
        .relative()
        .size_full()
        .when(ended, |screen| screen.opacity(0.6))
        .child(view)
        .child(watch)
}

#[cfg(test)]
#[path = "terminal_screen_tests.rs"]
mod draw_tests;
