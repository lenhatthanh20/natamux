//! The views of the open workspace that the details panel draws (T-0183): the Changes list with
//! the commit box and the git buttons, History, and the Docs list. The workspace has no tab bar
//! any more: main shows the terminals, or the picked file of Changes or Docs over them. New
//! terminal and Back to wrap-up moved into each pane header.

use gpui::Entity;
use ui::prelude::*;

use super::WorkspaceView;
use crate::workspaces::docs_tab::DocsTab;
use crate::workspaces::finished_banner::OnClick;

impl WorkspaceView {
    /// The panel's Changes view: the files and the commit box, then the git buttons under it.
    pub fn changes_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(task) = self.task(cx).filter(|task| task.workspace.is_some()) else {
            return div().into_any_element();
        };
        let Some(shown) = self.shown.as_ref() else {
            return div().into_any_element();
        };
        let uncommitted = shown.changes.as_ref().map(|changes| changes.len() as u64);
        let buttons = self.git_buttons(&task, shown, uncommitted, cx);
        let list = self
            .changes
            .update(cx, |changes, cx| changes.render_panel(window, cx));
        let row = (!buttons.is_empty()).then(|| {
            h_flex()
                .flex_none()
                .flex_wrap()
                .gap_1()
                .px_2()
                .pb_2()
                .children(buttons)
        });
        v_flex()
            .size_full()
            .child(div().flex_1().min_h_0().child(list))
            .children(row)
            .into_any_element()
    }

    /// The panel's History view.
    pub fn history_panel(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(task) = self.task(cx).filter(|task| task.workspace.is_some()) else {
            return div().into_any_element();
        };
        let Some(shown) = self.shown.as_ref() else {
            return div().into_any_element();
        };
        self.history(&task, shown, false, cx)
    }

    /// The panel's Docs view: the stage files and attachments; the picked one covers main.
    pub fn docs_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(task) = self.task(cx).filter(|task| task.workspace.is_some()) else {
            return div().into_any_element();
        };
        let docs = self.docs_of(&task.id, window, cx);
        docs.update(cx, |docs, cx| docs.render_panel(window, cx))
    }

    /// The Docs view of the task, made the first time the panel or main shows it.
    pub(super) fn docs_of(
        &mut self,
        task_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<DocsTab> {
        if let Some(docs) = self
            .docs
            .as_ref()
            .filter(|docs| docs.read(cx).task_id() == task_id)
        {
            return docs.clone();
        }
        let (dashboard, id) = (self.dashboard.clone(), task_id.to_owned());
        let docs = cx.new(|cx| DocsTab::new(dashboard, id, window, cx));
        self.docs = Some(docs.clone());
        docs
    }

    /// The pane headers' Back to wrap-up, while the terminals show in its place.
    pub(super) fn back_to_wrap_up(&self, cx: &mut Context<Self>) -> OnClick {
        let this = cx.weak_entity();
        std::rc::Rc::new(move |_: &mut Window, cx: &mut App| {
            this.update(cx, |view, cx| {
                if let Some(shown) = view.shown.as_mut() {
                    shown.shells_wanted = false;
                }
                cx.notify();
            })
            .ok();
        })
    }
}
