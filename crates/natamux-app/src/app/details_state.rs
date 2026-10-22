//! What the details panel shows (`useDashboardLayout.ts`, the details part of `view-store`): the
//! open workspace's task, else the selected one; open or closed per task, which the app does not
//! keep across runs (as the web: 'does not store the details flags'); the picked tab, which it
//! keeps.

use natamux_proto::tasks::task::Task;
use ui::prelude::*;

use super::Dashboard;
use crate::app::view_rules::{DetailsView, details_open_for, details_view_for, with_detail_open};

/// What a key or a command asks the panel to do once it shows the task.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelRequest {
    /// `e` and the pencil: the edit form.
    Edit,
    /// `stage.send-back`: the Send back dialog of the stage that waits.
    SendBack,
}

impl Dashboard {
    /// The task the panel is about. None while the Agents grid is open.
    pub fn details_task(&self) -> Option<&Task> {
        if self.agents_open {
            return None;
        }
        let id = self.open_task.as_ref().or(self.selected.as_ref())?;
        self.data.task(id)
    }

    /// The panel sits beside the task's workspace (not over the list).
    pub fn details_beside_workspace(&self) -> bool {
        self.open_task.is_some()
            && self
                .details_task()
                .is_some_and(|task| task.workspace.is_some())
    }

    /// `docked`: the window docks the panel. Until the user chooses, it is open where it docks
    /// beside a workspace.
    pub fn details_shown(&self, docked: bool) -> bool {
        let Some(task) = self.details_task() else {
            return false;
        };
        let choice = self
            .details_open
            .iter()
            .find(|(id, _)| *id == task.id)
            .map(|(_, open)| *open);
        details_open_for(choice, docked && self.details_beside_workspace())
    }

    /// The tab shown. The Files tab comes with screen 6, so a remembered Files shows Progress.
    pub fn details_view_now(&self) -> DetailsView {
        match details_view_for(self.view().details_view, self.details_beside_workspace()) {
            DetailsView::Files => DetailsView::Progress,
            view => view,
        }
    }

    pub fn set_details_open(&mut self, id: &str, open: bool, cx: &mut Context<Self>) {
        self.details_open = with_detail_open(&self.details_open, id, open);
        cx.notify();
    }

    /// Mod+Alt+B and the title bar toggle: the panel of the task it is about.
    pub fn toggle_details(&mut self, docked: bool, cx: &mut Context<Self>) {
        let shown = self.details_shown(docked);
        if let Some(id) = self.details_task().map(|task| task.id.clone()) {
            self.set_details_open(&id, !shown, cx);
        }
    }

    /// `i`: the selected task's panel, open or closed.
    pub fn toggle_selected_details(&mut self, docked: bool, cx: &mut Context<Self>) {
        let Some(id) = self.selected.clone() else {
            return;
        };
        let about_it = self.details_task().is_some_and(|task| task.id == id);
        let shown = about_it && self.details_shown(docked);
        self.set_details_open(&id, !shown, cx);
    }

    /// A click on a row or a card, and Show details of a row menu (`chooseTask`).
    pub fn choose(&mut self, id: &str, cx: &mut Context<Self>) {
        self.select(Some(id.to_owned()), cx);
        self.set_details_open(id, true, cx);
    }

    pub fn set_details_view(&mut self, view: DetailsView, cx: &mut Context<Self>) {
        self.update_view(cx, |state| state.details_view = Some(view));
    }

    /// Show in progress, Show progress and `progress.show` (`showProgress`).
    pub fn show_progress(&mut self, id: &str, cx: &mut Context<Self>) {
        let open = self.details_open.clone();
        let mut next = Vec::new();
        self.update_view(cx, |state| next = state.show_progress(&open, id));
        self.details_open = next;
    }

    /// Opens the panel on the task and asks it to do `request` once it shows it.
    pub fn request_panel(&mut self, id: &str, request: PanelRequest, cx: &mut Context<Self>) {
        if request == PanelRequest::SendBack {
            self.set_details_view(DetailsView::Progress, cx);
        }
        self.set_details_open(id, true, cx);
        self.panel_request = Some((id.to_owned(), request));
        cx.notify();
    }

    /// The panel takes the request meant for its task, once.
    pub fn take_panel_request(&mut self, id: &str) -> Option<PanelRequest> {
        let ours = self
            .panel_request
            .as_ref()
            .is_some_and(|(task, _)| task == id);
        ours.then(|| self.panel_request.take().map(|(_, request)| request))
            .flatten()
    }

    pub fn set_details_width(&mut self, width: f32, cx: &mut Context<Self>) {
        self.update_view(cx, |state| state.set_details_width(width));
    }

    pub fn set_section(&mut self, id: &str, open: bool, cx: &mut Context<Self>) {
        self.update_view(cx, |state| state.set_section(id, open));
    }

    pub fn toggle_notify(&mut self, cx: &mut Context<Self>) {
        self.update_view(cx, |state| state.notify = !state.notify);
    }
}
