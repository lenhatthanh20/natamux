//! How Root draws the shell: the bars, the docked sidebar or the drawer, main, the details panel
//! and its rail at the right edge.

use gpui::{KeyContext, MouseButton, MouseDownEvent, StyleRefinement};
use ui::prelude::*;

use super::{Root, Tool};
use crate::app::commands_stages::ShowTasksProgress;
use crate::app::layout_mode::{DetailsMode, SIDEBAR_WIDTH, SidebarMode, effective_sidebar_width};
use crate::app::shortcuts::{
    Approve, BlockTask, MarkReady, RequestChanges, ResumeAgent, RetryTask, UnblockTask,
};
use crate::app::view_state::{Layout, SIDEBAR_MAX, SIDEBAR_MIN};
use crate::components::resize_handle::{Edge, Limits, Resize, ResizeHandle};
use crate::perf::{draws, overlay};

/// The drawer of 768-1023: 300 wide, at most 85% of the window.
const DRAWER_WIDTH: f32 = 300.;
/// Dragging the sidebar narrower than this collapses it.
const SIDEBAR_COLLAPSE_BELOW: f32 = 180.;

/// Under 1024: the sidebar slides over main, above a scrim that closes it.
fn drawer_layer(
    panel: impl IntoElement,
    viewport: f32,
    close: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Div {
    let colors = cx.theme().colors();
    // The dialog layer's scrim (components/dialog.rs).
    let mut scrim = colors.elevated_surface_background;
    scrim.fade_out(0.2);
    div()
        .absolute()
        .inset_0()
        .occlude()
        .child(
            div()
                .id("drawer-scrim")
                .absolute()
                .inset_0()
                .bg(scrim)
                .on_mouse_down(MouseButton::Left, close),
        )
        .child(
            div()
                .relative()
                .h_full()
                .w(px(DRAWER_WIDTH.min(viewport * 0.85)))
                .bg(colors.panel_background)
                .border_r_1()
                .border_color(colors.border)
                .elevation_3(cx)
                // Else a mouse down in the panel also reaches the scrim, which closes the drawer
                // before the row under it gets its click (T-0198). The dialog panel does the same.
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(panel),
        )
}

impl Root {
    fn render_sidebar(&self, width: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let dashboard = self.dashboard.clone();
        let limits = Limits {
            min: SIDEBAR_MIN,
            max: SIDEBAR_MAX,
            default: SIDEBAR_WIDTH,
            collapse_below: Some(SIDEBAR_COLLAPSE_BELOW),
        };
        let resize = ResizeHandle::new(
            "sidebar-resize",
            &self.resize_focus,
            Edge::Right,
            limits,
            width,
            move |resize, _, cx| {
                dashboard.update(cx, |dashboard, cx| match resize {
                    Resize::Width(width) => dashboard.set_sidebar_width(width, cx),
                    Resize::Collapse => dashboard.set_sidebar_open(false, cx),
                })
            },
        );
        h_flex()
            .h_full()
            .flex_none()
            .child(
                div()
                    .w(px(width))
                    .h_full()
                    .bg(cx.theme().colors().panel_background)
                    // Drawn again only when the sidebar changes, as the phase 1 list was.
                    .child(
                        self.sidebar
                            .clone()
                            .cached(StyleRefinement::default().size_full()),
                    ),
            )
            .child(resize)
    }

    fn render_drawer(&self, viewport: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let close =
            cx.listener(|root, _: &MouseDownEvent, window, cx| root.set_drawer(false, window, cx));
        drawer_layer(self.sidebar.clone(), viewport, close, cx)
    }

    /// Main is drawn again only when what it shows changed: output of one terminal, a blink in
    /// the bars or a dashboard event draws Root, not the other views.
    fn render_main(&self, cx: &App) -> AnyElement {
        match &self.tool {
            Tool::Bench(bench) => bench.clone().into_any_element(),
            Tool::Ime(check) => check.clone().into_any_element(),
            Tool::None if self.dashboard.read(cx).open_task.is_some() => self
                .workspace
                .clone()
                .cached(StyleRefinement::default().size_full())
                .into_any_element(),
            Tool::None if self.dashboard.read(cx).agents_open => self
                .agents
                .clone()
                .cached(StyleRefinement::default().size_full())
                .into_any_element(),
            Tool::None => self
                .tasks
                .clone()
                .cached(StyleRefinement::default().size_full())
                .into_any_element(),
        }
    }

    fn render_body(&mut self, width: f32, window: &Window, cx: &mut Context<Self>) -> Div {
        if self.dashboard.read(cx).settings.is_some() {
            return h_flex()
                .flex_1()
                .min_h_0()
                .w_full()
                .child(self.settings.clone());
        }
        let layout = self.layout(window, cx);
        let docked = layout.sidebar == SidebarMode::Docked;
        self.sync_details_mode(window, cx);
        let details = self.details_shown(window, cx).then_some(layout.details);
        let rail = self.rail_shown(cx);
        let view = self.dashboard.read(cx).view();
        let sidebar_open = view.sidebar_open;
        let sidebar_width = effective_sidebar_width(view.sidebar_width, Some(width));
        h_flex()
            .relative()
            .flex_1()
            .min_h_0()
            .w_full()
            .when(docked && sidebar_open, |this| {
                this.child(self.render_sidebar(sidebar_width, cx))
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .bg(cx.theme().colors().editor_background)
                    .child(self.render_main(cx)),
            )
            .when(details == Some(DetailsMode::Docked), |this| {
                this.child(self.render_docked_details(sidebar_width, rail, cx))
            })
            .when(rail, |this| this.child(self.rail.clone()))
            .when(
                details.is_some_and(|mode| mode != DetailsMode::Docked),
                |this| this.child(self.render_sheet(width, layout.compact, rail, cx)),
            )
            .when(!docked && self.drawer_open, |this| {
                this.child(self.render_drawer(width, cx))
            })
    }
}

impl Root {
    /// The keys of the details panel, its stages, and the open workspace's tabs and files.
    fn workspace_actions(shell: Div, cx: &mut Context<Self>) -> Div {
        shell
            .on_action(cx.listener(Self::toggle_details))
            .on_action(cx.listener(Self::toggle_selected_details))
            .on_action(cx.listener(Self::edit_task))
            .on_action(cx.listener(Self::show_progress))
            .on_action(cx.listener(Self::approve_stage))
            .on_action(cx.listener(Self::report_stage))
            .on_action(cx.listener(Self::skip_stage))
            .on_action(cx.listener(Self::send_back_stage))
            .on_action(cx.listener(Self::show_terminal_tab))
            .on_action(cx.listener(Self::show_changes_tab))
            .on_action(cx.listener(Self::show_history_tab))
            .on_action(cx.listener(Self::commit_changes))
            .on_action(cx.listener(Self::show_docs_tab))
            .on_action(cx.listener(Self::show_open_file))
            .on_action(cx.listener(Self::close_tab))
            .on_action(cx.listener(Self::show_worktree_files))
    }

    /// The state action keys of the selected task.
    fn task_actions(shell: Div, cx: &mut Context<Self>) -> Div {
        shell
            .on_action(
                cx.listener(|root, _: &MarkReady, _, cx| root.task_action("task.readied", cx)),
            )
            .on_action(
                cx.listener(|root, _: &BlockTask, _, cx| root.task_action("task.blocked", cx)),
            )
            .on_action(
                cx.listener(|root, _: &ResumeAgent, _, cx| root.task_action("agent.resumed", cx)),
            )
            .on_action(
                cx.listener(|root, _: &Approve, _, cx| root.task_action("review.approved", cx)),
            )
            .on_action(cx.listener(|root, _: &RequestChanges, _, cx| {
                root.task_action("review.changes-requested", cx)
            }))
            .on_action(
                cx.listener(|root, _: &UnblockTask, _, cx| root.task_action("task.unblocked", cx)),
            )
            .on_action(
                cx.listener(|root, _: &RetryTask, _, cx| root.task_action("task.retried", cx)),
            )
    }
}

impl Render for Root {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        draws::count();
        let font = theme_settings::setup_ui_font(window, cx);
        if let Some(sign_in) = &self.sign_in {
            return div()
                .size_full()
                .font(font)
                .text_color(cx.theme().colors().text)
                .child(sign_in.clone())
                .child(self.toaster.clone())
                .into_any_element();
        }
        let width = f32::from(window.viewport_size().width);
        let docked = self.layout(window, cx).sidebar == SidebarMode::Docked;
        if docked {
            self.drawer_open = false;
        }
        let sidebar_shown = if docked {
            self.dashboard.read(cx).view().sidebar_open
        } else {
            self.drawer_open
        };
        let (title_bar, status_bar) = self.render_bars(sidebar_shown, window, cx);
        let body = self.render_body(width, window, cx);
        let mut context = KeyContext::new_with_defaults();
        context.add("Natamux");
        if self.focus.is_focused(window) {
            context.add("body");
        }
        let shell = Self::settings_actions(Self::task_actions(v_flex(), cx), cx);
        let shell = Self::workspace_actions(shell, cx);
        let colors = cx.theme().colors();
        shell
            .size_full()
            .font(font)
            .bg(colors.background)
            .text_color(colors.text)
            .key_context(context)
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::back))
            .on_action(cx.listener(Self::show_all))
            .on_action(cx.listener(Self::show_agents))
            .on_action(cx.listener(Self::toggle_agents))
            .on_action(cx.listener(Self::show_table))
            .on_action(cx.listener(Self::show_board))
            .on_action(cx.listener(|this, _: &ShowTasksProgress, _, cx| {
                this.show_list(Some(Layout::Progress), cx)
            }))
            .on_action(cx.listener(Self::toggle_sidebar))
            .on_action(cx.listener(Self::focus_filter))
            .on_action(cx.listener(Self::new_workspace))
            .on_action(cx.listener(Self::new_task))
            .on_action(cx.listener(Self::toggle_perf))
            .on_action(cx.listener(Self::save_perf))
            .on_action(cx.listener(Self::start_bench))
            .on_action(cx.listener(Self::stop_bench))
            .on_action(cx.listener(Self::check_ime))
            .on_action(cx.listener(Self::quit_app))
            .on_action(cx.listener(Self::close))
            .child(title_bar)
            .child(body)
            .child(status_bar)
            .children(overlay::render(cx))
            .child(self.menus.clone())
            .child(self.dialogs.clone())
            .child(self.palette.clone())
            .child(self.toaster.clone())
            .into_any_element()
    }
}

#[cfg(test)]
#[path = "window_render_tests.rs"]
mod drawer_tests;
