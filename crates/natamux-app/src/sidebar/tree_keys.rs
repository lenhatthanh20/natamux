//! Keys and focus of the tree that are not row moves: type-ahead, and the redraw on focus.

use gpui::KeyDownEvent;
use ui::prelude::*;

use crate::app::clock::now_ms;
use crate::sidebar::sidebar_text::{type_ahead_target, typed_word};
use crate::sidebar::workspace_tree::WorkspaceTree;

impl WorkspaceTree {
    /// The tree is a cached view: the focus ring needs its own notify when the focus comes or goes.
    pub(super) fn watch_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.focus_watch.is_some() {
            return;
        }
        let focus_in = cx.on_focus_in(&self.focus, window, |_, _, cx| cx.notify());
        let focus_out = cx.on_focus_out(&self.focus, window, |_, _, _, cx| cx.notify());
        self.focus_watch = Some([focus_in, focus_out]);
    }

    /// Letters jump to the next row whose title starts with what was typed.
    pub(super) fn type_ahead(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let stroke = &event.keystroke;
        let letter = stroke.key.chars().count() == 1 && stroke.key != " ";
        if self.rename.is_some() || !letter || stroke.modifiers.modified() {
            return;
        }
        let now = now_ms();
        let word = typed_word(&self.typed.0, self.typed.1, now, &stroke.key);
        self.typed = (word.clone(), now);
        let Some(model) = self.model.as_ref() else {
            return;
        };
        let labels: Vec<String> = model.entries.iter().map(|e| e.label.clone()).collect();
        let stop = self.current_stop(cx);
        let from = stop
            .and_then(|stop| model.entries.iter().position(|entry| entry.key == stop))
            .unwrap_or(0);
        if let Some(found) = type_ahead_target(&labels, &word, from) {
            let key = model.entries[found].key.clone();
            self.focus_key(key, window, cx);
            cx.stop_propagation();
        }
    }
}
