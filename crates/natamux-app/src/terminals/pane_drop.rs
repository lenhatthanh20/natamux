//! A path dragged from the Files tab onto a terminal pane (`TerminalView.tsx` drop): a pane of
//! the same task that has not ended takes it and types its quoted path. While the drag is over
//! it, a 2 px outline in the focus colour shows inside the pane.

use std::any::Any;

use gpui::{Entity, Focusable, Global, Stateful, transparent_black};
use natamux_client::TerminalStatus;
use ui::prelude::*;

use super::remote_terminal::RemoteTerminal;
use crate::files::explorer_tree::{DraggedPath, drop_text};

/// The pane that shows the outline without a drag: the shots cannot drag. Task id and terminal
/// name, as in `set_drop_target`.
#[derive(Default)]
struct ForcedTarget(Option<(String, String)>);

impl Global for ForcedTarget {}

/// For the `terminal-drop-target` shot: `set_drop_target(Some((task_id, terminal_name)), cx)`
/// draws the outline on that pane; None clears it.
pub fn set_drop_target(target: Option<(String, String)>, cx: &mut App) {
    cx.set_global(ForcedTarget(target));
}

/// The pane with the drop area laid over it. An ended terminal takes nothing.
pub fn with_drop(
    pane: Stateful<Div>,
    terminal: &Entity<RemoteTerminal>,
    cx: &App,
) -> Stateful<Div> {
    let remote = terminal.read(cx);
    if matches!(remote.status(), TerminalStatus::Ended { .. }) {
        return pane;
    }
    let task_id = remote.task_id().to_owned();
    let forced = cx
        .try_global::<ForcedTarget>()
        .and_then(|target| target.0.as_ref())
        .is_some_and(|(task, name)| task == remote.task_id() && name == remote.name());
    let focus_color = cx.theme().colors().border_focused;
    let base = if forced {
        focus_color
    } else {
        transparent_black()
    };
    let taker = terminal.clone();
    let accepted = task_id.clone();
    let area = div()
        .absolute()
        .inset_0()
        .rounded(px(8.))
        .border_2()
        .border_color(base)
        .can_drop(move |value: &dyn Any, _, _| {
            value
                .downcast_ref::<DraggedPath>()
                .is_some_and(|dragged| dragged.accepts(&accepted))
        })
        .drag_over::<DraggedPath>(move |style, dragged, _, _| {
            if dragged.accepts(&task_id) {
                style.border_color(focus_color)
            } else {
                style
            }
        })
        .on_drop(move |dragged: &DraggedPath, window, cx| {
            let text = drop_text(&dragged.path, dragged.kind);
            taker.update(cx, |terminal, cx| terminal.paste_text(&text, cx));
            let focus = taker.read(cx).view.focus_handle(cx);
            window.focus(&focus, cx);
        });
    pane.relative().child(area)
}
