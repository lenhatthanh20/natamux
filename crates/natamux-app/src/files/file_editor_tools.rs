//! Find (Mod+F), Go to line (Ctrl+G), Go to symbol (Mod+Shift+O) and Wrap long lines in the
//! editor tab (T-0316). The find bar sits under the header.

use gpui::{ClickEvent, Stateful};
use ui::{Tooltip, prelude::*};

use super::file_editor::FileEditor;
use super::outline_picker::open_outline;
use crate::app::shortcuts::{FindInEditor, GoToLine, ShowOutline};
use crate::components::go_to_line::open_go_to_line;
use crate::git::editor_find::open_editor_find;
use crate::terminals::pane_header::icon_button;

impl FileEditor {
    pub(super) fn with_editor_tools(
        &self,
        root: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        root.on_action(cx.listener(|this, _: &FindInEditor, window, cx| this.find(window, cx)))
            .on_action(cx.listener(|this, _: &GoToLine, window, cx| this.go_to_line(window, cx)))
            .on_action(cx.listener(|this, _: &ShowOutline, window, cx| this.outline(window, cx)))
    }

    /// Find with `term` typed: the `editor-find` shot.
    pub fn shot_find(&mut self, term: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.find(window, cx);
        if let Some(find) = &self.find {
            find.update(cx, |find, cx| find.set_term(term, window, cx));
        }
    }

    fn find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(body) = self.body.clone() else {
            return;
        };
        let target = body.read(cx).editor().clone();
        open_editor_find(&mut self.find, target, window, cx, |this| &mut this.find);
    }

    pub fn go_to_line(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(body) = self.body.as_ref().map(|body| body.downgrade()) else {
            return;
        };
        open_go_to_line(
            move |line, column, window, cx| {
                body.update(cx, |body, cx| body.go_to(line, column, window, cx))
                    .ok();
            },
            window,
            cx,
        );
    }

    pub fn outline(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(body) = self.body.clone() else {
            return;
        };
        let buffer = body.read(cx).buffer().clone();
        let body = body.downgrade();
        open_outline(
            &buffer,
            move |row, column, window, cx| {
                body.update(cx, |body, cx| {
                    body.go_to(row + 1, Some(column + 1), window, cx)
                })
                .ok();
            },
            window,
            cx,
        );
    }

    /// The header's Wrap long lines toggle, once the body is there. Not stored.
    pub(super) fn wrap_button(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let on = self.body.as_ref()?.read(cx).wraps();
        Some(
            icon_button("file-wrap", "text-wrap")
                .toggle_state(on)
                .tooltip(Tooltip::text("Wrap long lines"))
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    if let Some(body) = &this.body {
                        body.update(cx, |body, cx| body.toggle_wrap(cx));
                    }
                    cx.notify();
                }))
                .into_any_element(),
        )
    }

    pub(super) fn find_bar(&self) -> Option<AnyElement> {
        self.find.clone().map(|find| find.into_any_element())
    }
}
