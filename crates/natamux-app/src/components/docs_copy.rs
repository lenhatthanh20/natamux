//! Copy from the rendered Markdown (`docs_markdown.rs`, T-0350): Mod+C copies the mouse
//! selection, and a right press opens Copy and Select All. Before, the selection was look only
//! (docs/plan-mit-license.md D13).

use std::any::TypeId;

use gpui::{ClipboardItem, KeyBinding, KeyContext, Pixels, Point};
use ui::{ContextMenu, ContextMenuEntry, prelude::*};

use super::{DocArea, DocState, Hits, Pick, Unit};
use crate::components::menu::MenuLayer;

gpui::actions!(markdown_doc, [CopySelection]);

/// The key context of the view, so Mod+C copies only while it has focus.
const CONTEXT: &str = "MarkdownDoc";

pub fn bindings() -> [KeyBinding; 1] {
    [KeyBinding::new("secondary-c", CopySelection, Some(CONTEXT))]
}

impl DocState {
    /// Puts the selected text, as drawn, on the clipboard; no selection copies nothing.
    pub(super) fn copy(&self, hits: &Hits, cx: &mut App) {
        if let Some(range) = self.pick.selected() {
            cx.write_to_clipboard(ClipboardItem::new_string(hits.text_of(range)));
        }
    }

    pub(super) fn select_all(&mut self, hits: &Hits, cx: &mut Context<Self>) {
        self.pick = Pick::new(0..hits.end(), Unit::All);
        self.pick.held = false;
        cx.notify();
    }

    /// The right-click menu at `position`. The view takes focus first, so the menu gives it back.
    pub(super) fn open_menu(
        &mut self,
        position: Point<Pixels>,
        hits: &Hits,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus, cx);
        // Taken now: the press on the menu item drops the selection before the item runs.
        let text = self.pick.selected().map(|range| hits.text_of(range));
        let focus = self.focus.clone();
        let (state, all) = (cx.weak_entity(), hits.clone());
        let menu = ContextMenu::build(window, cx, move |menu, _, _| {
            let copy = ContextMenuEntry::new("Copy")
                .action(Box::new(CopySelection))
                .disabled(text.is_none())
                .handler(move |_, cx| {
                    if let Some(text) = &text {
                        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                    }
                });
            menu.context(focus)
                .item(copy)
                .separator()
                .entry("Select All", None, move |_, cx| {
                    state
                        .update(cx, |state, cx| state.select_all(&all, cx))
                        .ok();
                })
        });
        MenuLayer::open(menu, position, window, cx);
    }
}

impl DocArea {
    /// The key context and the copy action of this frame; paint only.
    pub(super) fn listen_keys(&self, hits: &Hits, window: &mut Window) {
        let mut context = KeyContext::default();
        context.add(CONTEXT);
        window.set_key_context(context);
        let (state, hits) = (self.state.downgrade(), hits.clone());
        window.on_action(TypeId::of::<CopySelection>(), move |_, phase, _, cx| {
            if phase.bubble() {
                state.update(cx, |state, cx| state.copy(&hits, cx)).ok();
            }
        });
    }
}

#[cfg(test)]
#[path = "docs_copy_tests.rs"]
mod tests;
