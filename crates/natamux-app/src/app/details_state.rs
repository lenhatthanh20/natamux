//! What the details panel shows (`useDashboardLayout.ts`, the details part of `view-store`): the
//! open workspace's task, else the selected one; open or closed as one flag for every task (a
//! Zed dock), and the picked tab. Both are kept in `view.json`.

use natamux_proto::tasks::task::Task;
use ui::prelude::*;

use super::Dashboard;
use crate::app::notify_rules::turned_on;
use crate::app::tab_rules::{RailClick, main_after_view, rail_click, shown_details_view};
use crate::app::view_rules::{DetailsView, details_view_for};

/// What a key or a command asks the panel to do once it shows the task.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelRequest {
    /// `e` and Edit task of the "..." menu: the edit form.
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

    /// The dock shows when the flag is on, except over the Agents grid. With no task it shows
    /// an empty state.
    pub fn details_shown(&self) -> bool {
        self.view().details_open && !self.agents_open
    }

    /// The view shown. A remembered view the task cannot show shows Progress
    /// (`shown_details_view`).
    pub fn details_view_now(&self) -> DetailsView {
        let beside = self.details_beside_workspace();
        let view = details_view_for(self.view().details_view, beside);
        let has_workspace = self
            .details_task()
            .is_some_and(|task| task.workspace.is_some());
        shown_details_view(view, has_workspace, beside)
    }

    /// The open panel shows `view` beside the open workspace: Changes and Docs then draw their
    /// list there, and only the picked file over main (T-0183).
    pub fn panel_shows(&self, view: DetailsView) -> bool {
        self.details_shown() && self.details_beside_workspace() && self.details_view_now() == view
    }

    pub fn set_details_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.update_view(cx, |state| state.details_open = open);
    }

    /// Mod+Alt+B and the status bar toggle.
    pub fn toggle_details(&mut self, cx: &mut Context<Self>) {
        let open = self.view().details_open;
        self.set_details_open(!open, cx);
    }

    /// `i`: selects the row; the dock closes when it is open and already about that task.
    pub fn toggle_selected_details(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.selected.clone() else {
            return;
        };
        let about_it = self.details_task().is_some_and(|task| task.id == id);
        let close = about_it && self.details_shown();
        self.select(Some(id), cx);
        self.set_details_open(!close, cx);
    }

    /// A click on a row or a card, and Show details of a row menu (`chooseTask`).
    pub fn choose(&mut self, id: &str, cx: &mut Context<Self>) {
        self.select(Some(id.to_owned()), cx);
        self.set_details_open(true, cx);
    }

    pub fn set_details_view(&mut self, view: DetailsView, cx: &mut Context<Self>) {
        self.update_view(cx, |state| state.details_view = Some(view));
    }

    /// Keys 2 to 4 and the commands of a view: the panel opens on `view`; Changes and Docs also
    /// cover the terminals in main (`main_after_view`).
    pub fn show_view(&mut self, view: DetailsView, cx: &mut Context<Self>) {
        self.set_details_view(view, cx);
        self.set_details_open(true, cx);
        let main = main_after_view(view, self.workspace_tab);
        if main != self.workspace_tab {
            self.pick_tab(main, cx);
        }
    }

    /// A click on a rail icon (`rail_click`): the picked view of an open panel closes it.
    pub fn rail_pick(&mut self, view: DetailsView, cx: &mut Context<Self>) {
        match rail_click(view, self.details_view_now(), self.details_shown()) {
            RailClick::Close => self.set_details_open(false, cx),
            RailClick::Show(view) => self.show_view(view, cx),
        }
    }

    /// Show in progress, Show progress and `progress.show` (`showProgress`). The panel is about
    /// `details_task`, so `_id` only keeps the callers' shape.
    pub fn show_progress(&mut self, _id: &str, cx: &mut Context<Self>) {
        self.update_view(cx, |state| state.show_progress());
    }

    /// `files.show` and `showFiles`: the dock open on its Files tab.
    pub fn show_files(&mut self, _id: &str, cx: &mut Context<Self>) {
        self.update_view(cx, |state| state.show_files());
    }

    /// Opens the panel on the task and asks it to do `request` once it shows it.
    pub fn request_panel(&mut self, id: &str, request: PanelRequest, cx: &mut Context<Self>) {
        if request == PanelRequest::SendBack {
            self.set_details_view(DetailsView::Progress, cx);
        }
        self.set_details_open(true, cx);
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
        if self.view().notify {
            cx.show_system_notification(turned_on());
        }
    }
}
