//! Find (Mod+F), Go to line (Ctrl+G) and Go to symbol (Mod+Shift+O) in the diff of the Changes
//! tab (T-0316). Go to line and the symbols use the working tree's side; find searches the side
//! that has the focus. The find bar sits under the toolbar.

use ui::prelude::*;

use super::ChangesView;
use crate::app::shortcuts::{FindInEditor, GoToLine, ShowOutline};
use crate::components::go_to_line::open_go_to_line;
use crate::files::outline_picker::open_outline;
use crate::git::editor_find::open_editor_find;

impl ChangesView {
    pub(super) fn with_editor_tools(&self, root: Div, cx: &mut Context<Self>) -> Div {
        root.on_action(cx.listener(|view, _: &FindInEditor, window, cx| view.find(window, cx)))
            .on_action(cx.listener(|view, _: &GoToLine, window, cx| view.go_to_line(window, cx)))
            .on_action(cx.listener(|view, _: &ShowOutline, window, cx| view.outline(window, cx)))
    }

    pub(super) fn find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self.diff.read(cx).focused_editor(window, cx) else {
            return;
        };
        open_editor_find(&mut self.find, target, window, cx, |view| &mut view.find);
    }

    fn go_to_line(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.diff.read(cx).ready() {
            return;
        }
        let diff = self.diff.downgrade();
        open_go_to_line(
            move |line, column, window, cx| {
                diff.update(cx, |diff, cx| diff.go_to_new_line(line, column, window, cx))
                    .ok();
            },
            window,
            cx,
        );
    }

    fn outline(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(buffer) = self.diff.read(cx).buffer.clone() else {
            return;
        };
        let diff = self.diff.downgrade();
        open_outline(
            &buffer,
            move |row, column, window, cx| {
                diff.update(cx, |diff, cx| {
                    diff.go_to_new_line(row + 1, Some(column + 1), window, cx)
                })
                .ok();
            },
            window,
            cx,
        );
    }

    /// The find bar, while its editor is still one of the diff's; a rebuilt diff drops it.
    pub(super) fn find_bar(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let find = self.find.clone()?;
        let target = find.read(cx).target().clone();
        if !self.diff.read(cx).shows_editor(&target) {
            find.update(cx, |find, cx| find.clear(cx));
            self.find = None;
            return None;
        }
        Some(find.into_any_element())
    }
}
