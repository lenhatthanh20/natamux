//! How Root draws the shell: the bars, the docked sidebar or the drawer, and main.

use gpui::{ClickEvent, KeyContext, MouseButton, StyleRefinement};
use natamux_proto::tasks::task::TaskState;
use ui::prelude::*;

use super::{Root, Tool};
use crate::app::commands_stages::ShowTasksProgress;
use crate::app::layout_mode::{DetailsMode, SIDEBAR_WIDTH, SidebarMode, effective_sidebar_width};
use crate::app::shell_rules::{Crumb, crumb, live_item, running_count, window_title};
use crate::app::shortcuts::{
    Approve, BlockTask, MarkReady, RequestChanges, ResumeAgent, RetryTask, ToggleAgents,
    ToggleDetails, ToggleSidebar, UnblockTask,
};
use crate::app::status_bar::StatusBar;
use crate::app::status_bar::now_ms;
use crate::app::title_bar::TitleBar;
use crate::app::view_state::{Layout, SIDEBAR_MAX, SIDEBAR_MIN};
use crate::components::resize_handle::{Edge, Limits, Resize, ResizeHandle};
use crate::perf::{draws, overlay};
use crate::tasks::task_attention::needs_me_tasks;

/// The drawer of 768-1023: 300 wide, at most 85% of the window.
const DRAWER_WIDTH: f32 = 300.;
/// Dragging the sidebar narrower than this collapses it.
const SIDEBAR_COLLAPSE_BELOW: f32 = 180.;

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

    /// Under 1024: the sidebar slides over main, above a scrim that closes it.
    fn render_drawer(&self, viewport: f32, cx: &mut Context<Self>) -> impl IntoElement {
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
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|root, _, window, cx| root.set_drawer(false, window, cx)),
                    ),
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
                    .child(self.sidebar.clone()),
            )
    }

    fn render_main(&self, cx: &App) -> AnyElement {
        match &self.tool {
            Tool::Bench(bench) => bench.clone().into_any_element(),
            Tool::Ime(check) => check.clone().into_any_element(),
            Tool::None if self.dashboard.read(cx).open_task.is_some() => {
                self.workspace.clone().into_any_element()
            }
            Tool::None if self.dashboard.read(cx).agents_open => {
                self.agents.clone().into_any_element()
            }
            Tool::None => self
                .tasks
                .clone()
                .cached(StyleRefinement::default().size_full())
                .into_any_element(),
        }
    }

    /// The title bar and the status bar, and the window title "(N) Natamux".
    fn render_bars(
        &mut self,
        width: f32,
        sidebar_shown: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (TitleBar, StatusBar) {
        let dashboard = self.dashboard.read(cx);
        let tasks = &dashboard.data.tasks;
        let now = now_ms();
        let needs_you = needs_me_tasks(tasks, now).len();
        let running = running_count(tasks);
        let open = dashboard
            .open_task
            .as_deref()
            .and_then(|id| dashboard.data.task(id));
        let settings = dashboard.settings.is_some();
        let crumb = if settings {
            Crumb::View("Settings")
        } else {
            crumb(open, dashboard.agents_open, &dashboard.data.repos)
        };
        let live = live_item(&dashboard.data.link);
        let jobs = dashboard.data.jobs.clone();
        let title = window_title(needs_you);
        if title != self.title {
            window.set_window_title(&title);
            self.title = title;
        }
        let show_waiting =
            |root: &mut Root, _: &ClickEvent, window: &mut Window, cx: &mut Context<Root>| {
                root.show_needs_you(window, cx)
            };
        let agents_open = dashboard.agents_open;
        // Over Settings both toggles show off; a click closes Settings (`close_settings`).
        let details = if settings {
            Ok(false)
        } else {
            self.details_toggle(window, cx)
        };
        let sidebar_shown = sidebar_shown && !settings;
        let title_bar = TitleBar::new(crumb, self.dashboard.clone());
        let status_bar = StatusBar::new(live, running, needs_you, jobs, width)
            .toggles(sidebar_shown, details, agents_open)
            .on_toggles(
                cx.listener(|root, _: &ClickEvent, window, cx| {
                    root.toggle_sidebar(&ToggleSidebar, window, cx)
                }),
                cx.listener(|root, _: &ClickEvent, window, cx| {
                    root.toggle_details(&ToggleDetails, window, cx)
                }),
                cx.listener(|root, _: &ClickEvent, window, cx| {
                    root.toggle_agents(&ToggleAgents, window, cx)
                }),
            )
            .on_running(cx.listener(|root, _: &ClickEvent, _, cx| {
                root.show_state(Some(TaskState::Running), cx)
            }))
            .on_needs_you(cx.listener(show_waiting))
            .on_open_task(cx.listener(|root, id: &str, _, cx| {
                root.dashboard
                    .update(cx, |dashboard, cx| dashboard.open(id, cx))
            }));
        (title_bar, status_bar)
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
                this.child(self.render_docked_details(cx))
            })
            .when(
                details.is_some_and(|mode| mode != DetailsMode::Docked),
                |this| this.child(self.render_sheet(width, layout.compact, cx)),
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
            .on_action(cx.listener(Self::reopen_tabs))
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
        let (title_bar, status_bar) = self.render_bars(width, sidebar_shown, window, cx);
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
