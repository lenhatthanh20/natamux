//! The details panel in the shell (`AppShell.tsx`, `useDashboardCommands.ts`): a dock at the right
//! of main from 768 while main keeps 400, else a sheet over main (with a scrim under 1024). Esc
//! closes the top layer first; the panel's keys and the Task menu's stage commands.

use gpui::{MouseButton, StyleRefinement};
use natamux_proto::stages::actions::{StageAction, StageActionInput};
use ui::prelude::*;

use super::Root;
use crate::app::command_when::{CommandState, When};
use crate::app::dashboard::details::PanelRequest;
use crate::app::layout_mode::{DetailsMode, PANEL_WIDTH};
use crate::app::shell_rules::details_toggle;
use crate::app::shortcuts::{
    ApproveStage, Back, EditTask, ReportStage, SendBackStage, ShowProgress, SkipStage,
    ToggleDetails, ToggleSelectedDetails,
};
use crate::app::view_state::{DETAILS_MAX, DETAILS_MIN};
use crate::components::resize_handle::{Edge, Limits, Resize, ResizeHandle};

/// `min(360px, 90vw)`.
const SHEET_WIDTH: f32 = 360.;

impl Root {
    /// The details panel, for `--shot`.
    pub(crate) fn details_panel(&self) -> &gpui::Entity<crate::tasks::details_panel::DetailsPanel> {
        &self.details
    }

    fn details_mode(&self, window: &Window, cx: &App) -> DetailsMode {
        self.layout(window, cx).details
    }

    /// Whether the panel shows now. Without a task the dock shows its empty state; a sheet or
    /// page needs a task, so a fresh start does not cover main.
    pub(super) fn details_shown(&self, window: &Window, cx: &App) -> bool {
        let dashboard = self.dashboard.read(cx);
        let has_task = dashboard.details_task().is_some();
        let fits = has_task || self.details_mode(window, cx) == DetailsMode::Docked;
        dashboard.details_shown() && fits && matches!(self.tool, super::Tool::None)
    }

    /// The status bar toggle and Mod+Alt+B: whether the details show, or why they cannot.
    pub(super) fn details_toggle(&self, window: &Window, cx: &App) -> Result<bool, &'static str> {
        let dashboard = self.dashboard.read(cx);
        details_toggle(
            dashboard.agents_open,
            dashboard.details_task().is_some(),
            self.details_mode(window, cx) == DetailsMode::Docked,
            self.details_shown(window, cx),
        )
    }

    /// The docked column: the resize line at its left edge, then the panel.
    pub(super) fn render_docked_details(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let width = self.dashboard.read(cx).view().details_width;
        let dashboard = self.dashboard.clone();
        let limits = Limits {
            min: DETAILS_MIN,
            max: DETAILS_MAX,
            default: PANEL_WIDTH,
            collapse_below: None,
        };
        let resize = ResizeHandle::new(
            "details-resize",
            &self.details_resize,
            Edge::Left,
            limits,
            width,
            move |resize, _, cx| {
                if let Resize::Width(width) = resize {
                    dashboard.update(cx, |dashboard, cx| dashboard.set_details_width(width, cx));
                }
            },
        );
        h_flex().h_full().flex_none().child(resize).child(
            div()
                .w(px(width))
                .h_full()
                .border_l_1()
                .border_color(cx.theme().colors().border)
                .child(
                    self.details
                        .clone()
                        .cached(StyleRefinement::default().size_full()),
                ),
        )
    }

    /// The sheet: over main at the right; under 1024 above a scrim that closes it.
    pub(super) fn render_sheet(
        &self,
        viewport: f32,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let colors = cx.theme().colors();
        let mut scrim = colors.elevated_surface_background;
        scrim.fade_out(0.2);
        div()
            .absolute()
            .inset_0()
            .flex()
            .justify_end()
            .when(compact, |this| {
                this.occlude().child(
                    div()
                        .id("details-scrim")
                        .absolute()
                        .inset_0()
                        .bg(scrim)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|root, _, _, cx| root.close_details(cx)),
                        ),
                )
            })
            .child(
                div()
                    .relative()
                    .h_full()
                    .w(px(SHEET_WIDTH.min(viewport * 0.9)))
                    .occlude()
                    .border_l_1()
                    .border_color(colors.border)
                    .elevation_3(cx)
                    .child(
                        self.details
                            .clone()
                            .cached(StyleRefinement::default().size_full()),
                    ),
            )
    }

    /// Tells the panel how it is shown (the Back of a page, the sheet's Answer in terminal).
    pub(super) fn sync_details_mode(&self, window: &Window, cx: &mut Context<Self>) {
        let mode = self.details_mode(window, cx);
        self.details
            .update(cx, |panel, cx| panel.set_mode(mode, cx));
    }

    fn close_details(&mut self, cx: &mut Context<Self>) {
        self.dashboard
            .update(cx, |dashboard, cx| dashboard.set_details_open(false, cx));
    }

    /// Esc closes the top layer first: Settings (when focus is outside it), the drawer, a sheet,
    /// a tool, the open task, then the details dock.
    pub(super) fn back(&mut self, _: &Back, window: &mut Window, cx: &mut Context<Self>) {
        if self.close_settings(cx) {
            return;
        }
        let sheet =
            self.details_mode(window, cx) != DetailsMode::Docked && self.details_shown(window, cx);
        let dashboard = self.dashboard.read(cx);
        let nothing_open = dashboard.open_task.is_none() && !dashboard.agents_open;
        if self.drawer_open {
            self.set_drawer(false, window, cx);
        } else if sheet {
            self.close_details(cx);
        } else if !matches!(self.tool, super::Tool::None) {
            self.close_tool(cx);
        } else if nothing_open {
            self.close_details(cx);
        } else {
            self.dashboard
                .update(cx, |dashboard, cx| dashboard.back(cx));
        }
    }

    /// Does nothing where the toggle is off (`details_toggle`): flipping the hidden flag there
    /// would show nothing.
    pub(super) fn toggle_details(
        &mut self,
        _: &ToggleDetails,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.close_settings(cx) {
            return;
        }
        if self.details_toggle(window, cx).is_ok() {
            self.dashboard
                .update(cx, |dashboard, cx| dashboard.toggle_details(cx));
        }
    }

    pub(super) fn toggle_selected_details(
        &mut self,
        _: &ToggleSelectedDetails,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.close_settings(cx) {
            return;
        }
        self.dashboard
            .update(cx, |dashboard, cx| dashboard.toggle_selected_details(cx));
    }

    pub(super) fn edit_task(&mut self, _: &EditTask, _: &mut Window, cx: &mut Context<Self>) {
        self.request(PanelRequest::Edit, cx);
    }

    pub(super) fn send_back_stage(
        &mut self,
        _: &SendBackStage,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.request(PanelRequest::SendBack, cx);
    }

    fn request(&mut self, request: PanelRequest, cx: &mut Context<Self>) {
        let id = self
            .dashboard
            .read(cx)
            .details_task()
            .map(|task| task.id.clone());
        if let Some(id) = id {
            self.dashboard.update(cx, |dashboard, cx| {
                dashboard.request_panel(&id, request, cx)
            });
        }
    }

    pub(super) fn show_progress(
        &mut self,
        _: &ShowProgress,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = self
            .dashboard
            .read(cx)
            .details_task()
            .map(|task| task.id.clone());
        if let Some(id) = id {
            self.dashboard
                .update(cx, |dashboard, cx| dashboard.show_progress(&id, cx));
        }
    }

    /// `stage.approve`, `stage.report`, `stage.skip`: the open task's stage, with no stage id.
    fn stage_command(
        &mut self,
        action: StageAction,
        failure: &'static str,
        cx: &mut Context<Self>,
    ) {
        let dashboard = self.dashboard.read(cx);
        let Some(task) = dashboard
            .open_task
            .as_deref()
            .and_then(|id| dashboard.data.task(id))
        else {
            return;
        };
        let state = CommandState {
            task_open: true,
            agents_open: false,
            has_selection: true,
            selected_state: Some(task.state),
            workspace_open: task.workspace.is_some(),
        };
        if !When::StageOpen.holds(&state) {
            return;
        }
        let id = task.id.clone();
        let input = StageActionInput::new(action);
        self.dashboard.update(cx, |dashboard, cx| {
            dashboard.stage_action(&id, input, failure, cx).detach()
        });
    }

    pub(super) fn approve_stage(
        &mut self,
        _: &ApproveStage,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.stage_command(StageAction::Approve, "Could not approve the stage.", cx);
    }

    pub(super) fn report_stage(&mut self, _: &ReportStage, _: &mut Window, cx: &mut Context<Self>) {
        self.stage_command(StageAction::Report, "Could not mark the stage done.", cx);
    }

    pub(super) fn skip_stage(&mut self, _: &SkipStage, _: &mut Window, cx: &mut Context<Self>) {
        self.stage_command(StageAction::Skip, "Could not skip the stage.", cx);
    }
}
