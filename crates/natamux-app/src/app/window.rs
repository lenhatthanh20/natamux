//! The window and the root view, the app shell of docs/ui-spec.md 3.1 (`features/app/AppShell.tsx`):
//! title bar, the sidebar (docked, or a drawer under 1024), main and the status bar, with the menu,
//! dialog and toast layers on top. Main shows All tasks, the open task's workspace, the Agents
//! grid, or a tool of phase 1 (bench, IME check). The Settings page fills the body over them.

use std::collections::BTreeSet;
use std::sync::Arc;

use editor::Editor;
use gpui::{
    App, Bounds, Entity, FocusHandle, Focusable, TitlebarOptions, WindowBounds, WindowOptions,
    point, px, size,
};
use natamux_proto::tasks::task::{Task, TaskState};
use ui::prelude::*;

use super::dashboard::{Dashboard, DashboardChange, SavedView, TerminalReader};
use super::dock_badge::ask_for_badges;
use super::layout_mode::{
    LayoutInput, LayoutMode, SidebarMode, decide_layout, effective_sidebar_width,
};
use super::notify::{self, Notifier};
use super::options::Options;
use super::settings_view::SettingsView;
use super::shell_rules::{AgentsToggle, agents_toggle, window_title};
use super::shortcuts::{
    FocusFilter, NewTask, NewWorkspace, ShowAgents, ShowAllTasks, ShowBoard, ShowTable,
    ToggleAgents, ToggleSidebar,
};
use super::sign_in::{SignIn, StartProblem};
use super::source::Source;
use super::view_state::Layout;
use crate::bench::bench_view::BenchView;
use crate::components::dialog::DialogLayer;
use crate::components::menu::MenuLayer;
use crate::components::toasts::Toaster;
use crate::ime::ime_check::ImeCheck;
use crate::palette::palette_layer::PaletteLayer;
use crate::sidebar::sidebar::Sidebar;
use crate::tasks::add_task_dialog;
use crate::tasks::details_panel::DetailsPanel;
use crate::tasks::details_rail::DetailsRail;
use crate::tasks::task_view::TaskView;
use crate::terminals::agent_panes::AgentPanes;
use crate::terminals::terminal_cache::{Owner, TerminalCache};
use crate::workspaces::new_workspace_dialog;
use crate::workspaces::workspace_view::WorkspaceView;
use crate::workspaces::wrap_up_rules::is_finished_state;

const DEFAULT_SIZE: (u32, u32) = (1280, 800);
const MIN_SIZE: (f32, f32) = (800., 560.);

enum Tool {
    None,
    Bench(Entity<BenchView>),
    Ime(Entity<ImeCheck>),
}

pub struct Root {
    source: Option<Arc<Source>>,
    dashboard: Entity<Dashboard>,
    sidebar: Entity<Sidebar>,
    tasks: Entity<TaskView>,
    workspace: Entity<WorkspaceView>,
    agents: Entity<AgentPanes>,
    details: Entity<DetailsPanel>,
    /// The details panel's views at the right edge (T-0183).
    rail: Entity<DetailsRail>,
    settings: Entity<SettingsView>,
    tool: Tool,
    /// `--gate2`: a bench whose shells keep printing while main shows All tasks.
    parked_bench: Option<Entity<BenchView>>,
    menus: Entity<MenuLayer>,
    dialogs: Entity<DialogLayer>,
    palette: Entity<PaletteLayer>,
    /// Fills the window while the app has no token the daemon takes.
    sign_in: Option<Entity<SignIn>>,
    toaster: Entity<Toaster>,
    focus: FocusHandle,
    resize_focus: FocusHandle,
    details_resize: FocusHandle,
    notifier: Notifier,
    /// The sidebar drawer under 1024. The docked sidebar's open state is in the view state.
    drawer_open: bool,
    /// The task open before the agents toggle showed the Agents grid; None: All tasks.
    agents_back: Option<String>,
    /// The toasts and the refused token Root took from the dashboard.
    followed: follow::Followed,
    title_bar: Entity<TitleBarView>,
    status_bar: Entity<StatusBarView>,
    /// Draws the perf overlay's numbers again, only while it is on.
    overlay_refresh: Option<gpui::Task<()>>,
}

impl Root {
    fn new(
        source: Result<Arc<Source>, StartProblem>,
        saved: SavedView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let wants_sign_in = matches!(source, Err(StartProblem::SignIn));
        let menus = MenuLayer::init(cx);
        let dialogs = DialogLayer::init(cx);
        let palette = PaletteLayer::init(cx);
        let toaster = Toaster::init(cx);
        let dashboard = cx.new(|cx| Dashboard::new(source.clone(), saved, cx));
        let sidebar = cx.new(|cx| Sidebar::new(dashboard.clone(), window, cx));
        let tasks = cx.new(|cx| TaskView::new(dashboard.clone(), window, cx));
        let source = source.ok();
        let workspace = cx.new(|cx| WorkspaceView::new(dashboard.clone(), cx));
        let agents = cx.new(|cx| AgentPanes::new(dashboard.clone(), cx));
        let details =
            cx.new(|cx| DetailsPanel::new(dashboard.clone(), workspace.clone(), window, cx));
        let rail = cx.new(|cx| DetailsRail::new(dashboard.clone(), details.clone(), cx));
        let settings = cx.new(|cx| SettingsView::new(dashboard.clone(), window, cx));
        cx.subscribe_in(&dashboard, window, Self::on_dashboard)
            .detach();
        cx.observe_in(&dashboard, window, Self::follow_dashboard)
            .detach();
        Self::watch_window(window, cx);
        notify::follow_clicks(&dashboard, window.window_handle(), cx);
        // Adds the badge for users who allowed notes before the app asked for it; no prompt then.
        if dashboard.read(cx).view().notify {
            ask_for_badges();
        }
        let title_bar = cx.new(|cx| TitleBarView::new(dashboard.clone(), cx));
        let weak_root = cx.weak_entity();
        let status_bar = cx.new(|cx| StatusBarView::new(weak_root, dashboard.clone(), cx));
        let mut root = Root {
            source,
            dashboard,
            sidebar,
            tasks,
            workspace,
            agents,
            details,
            rail,
            settings,
            tool: Tool::None,
            parked_bench: None,
            menus,
            dialogs,
            palette,
            sign_in: None,
            toaster,
            focus: cx.focus_handle(),
            resize_focus: cx.focus_handle().tab_stop(true),
            details_resize: cx.focus_handle().tab_stop(true),
            notifier: Notifier::default(),
            drawer_open: false,
            agents_back: None,
            followed: Default::default(),
            title_bar,
            status_bar,
            overlay_refresh: None,
        };
        root.follow_overlay(cx);
        if wants_sign_in {
            root.show_sign_in(false, window, cx);
        }
        root
    }

    fn on_dashboard(
        &mut self,
        _: &Entity<Dashboard>,
        change: &DashboardChange,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match change {
            DashboardChange::Tasks => {
                // A task that is gone, finished or has no workspace keeps no hidden terminal.
                let live: BTreeSet<String> = self
                    .dashboard
                    .read(cx)
                    .data
                    .tasks
                    .iter()
                    .filter(|task| task.workspace.is_some() && !is_finished_state(task.state))
                    .map(|task| task.id.clone())
                    .collect();
                TerminalCache::set_live(live, cx);
                self.notifier.after_tasks(&self.dashboard, window, cx);
            }
            DashboardChange::Opened(_) | DashboardChange::AgentsOpened => {
                self.close_tool(cx);
                self.drawer_open = false;
                self.sync_pane(cx);
            }
            DashboardChange::BackToList => {
                self.sync_pane(cx);
                let focus = self.tasks.focus_handle(cx);
                window.focus(&focus, cx);
            }
            DashboardChange::SettingsOpened(focus) => self.settings_opened(*focus, window, cx),
            DashboardChange::SettingsClosed => self.settings_closed(window, cx),
            DashboardChange::WorkspaceChanged(_)
            | DashboardChange::ShellsChanged(_)
            | DashboardChange::ReviewCommentsChanged(_)
            | DashboardChange::TaskLogged(_)
            | DashboardChange::UndoRollback(_) => {}
        }
    }

    /// The text box of the sidebar.
    pub fn filter(&self, cx: &App) -> Entity<Editor> {
        self.sidebar.read(cx).filter().clone()
    }

    /// The first text box on screen, which the smoke test types into: the token of Sign in, else
    /// the sidebar filter.
    pub fn text_box(&self, cx: &App) -> Entity<Editor> {
        match &self.sign_in {
            Some(sign_in) => sign_in.read(cx).field().clone(),
            None => self.filter(cx),
        }
    }

    pub(crate) fn dashboard(&self) -> &Entity<Dashboard> {
        &self.dashboard
    }

    /// All tasks in main, for `--shot` and `--gate2`.
    pub(super) fn task_view(&self) -> &Entity<TaskView> {
        &self.tasks
    }

    /// The workspace in main, for `--gate2`.
    pub(super) fn workspace_view(&self) -> &Entity<WorkspaceView> {
        &self.workspace
    }

    pub(super) fn sidebar(&self) -> &Entity<Sidebar> {
        &self.sidebar
    }

    /// A task of the daemon's list, once the list came.
    pub(super) fn task(&self, id: &str, cx: &App) -> Option<Task> {
        self.dashboard.read(cx).data.task(id).cloned()
    }

    /// The open task, else the selected one: what the bench and the IME check work on.
    fn current_task(&self, cx: &App) -> Option<Task> {
        let dashboard = self.dashboard.read(cx);
        let id = dashboard
            .open_task
            .as_ref()
            .or(dashboard.selected.as_ref())?;
        dashboard.data.task(id).cloned()
    }

    fn layout(&self, window: &Window, cx: &App) -> LayoutMode {
        let width = f32::from(window.viewport_size().width);
        let view = self.dashboard.read(cx).view();
        let sidebar_width = if view.sidebar_open {
            effective_sidebar_width(view.sidebar_width, Some(width))
        } else {
            0.
        };
        // The rail's 40 px are not counted: at 1024 wide the panel still docks beside the
        // sidebar (main 384) instead of covering the diff as a sheet.
        decide_layout(LayoutInput {
            width: Some(width),
            sidebar_width,
            details_width: view.details_width,
            coarse: false,
        })
    }

    /// The terminals give up their size while something else fills main (a tool, Settings). The
    /// workspace and the Agents grid claim theirs again when they draw.
    fn sync_pane(&mut self, cx: &mut Context<Self>) {
        let dashboard = self.dashboard.read(cx);
        let tool = !matches!(self.tool, Tool::None) || dashboard.settings.is_some();
        let (workspace, agents) = (dashboard.open_task.is_some(), dashboard.agents_open);
        if tool || !workspace {
            TerminalCache::claim(Owner::Workspace, BTreeSet::new(), cx);
        }
        if tool || !agents {
            TerminalCache::claim(Owner::Agents, BTreeSet::new(), cx);
        }
        if !agents {
            self.dashboard
                .read(cx)
                .show_terminals(TerminalReader::Agents, Vec::new());
        }
        cx.notify();
    }

    // The shell's commands

    fn toggle_sidebar(&mut self, _: &ToggleSidebar, window: &mut Window, cx: &mut Context<Self>) {
        if self.close_settings(cx) {
            return;
        }
        match self.layout(window, cx).sidebar {
            SidebarMode::Drawer => self.set_drawer(!self.drawer_open, window, cx),
            SidebarMode::Docked => self
                .dashboard
                .update(cx, |dashboard, cx| dashboard.toggle_sidebar(cx)),
        }
    }

    /// Opening moves focus into the drawer; closing gives it to the list.
    pub(super) fn set_drawer(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.drawer_open = open;
        let focus = if open {
            self.sidebar.focus_handle(cx)
        } else {
            self.tasks.focus_handle(cx)
        };
        window.focus(&focus, cx);
        cx.notify();
    }

    fn focus_filter(&mut self, _: &FocusFilter, window: &mut Window, cx: &mut Context<Self>) {
        match self.layout(window, cx).sidebar {
            SidebarMode::Drawer => self.drawer_open = true,
            SidebarMode::Docked => self
                .dashboard
                .update(cx, |dashboard, cx| dashboard.set_sidebar_open(true, cx)),
        }
        let focus = self.filter(cx).focus_handle(cx);
        window.focus(&focus, cx);
        cx.notify();
    }

    fn show_list(&mut self, layout: Option<Layout>, cx: &mut Context<Self>) {
        self.close_tool(cx);
        self.drawer_open = false;
        self.dashboard.update(cx, |dashboard, cx| {
            dashboard.close_settings(cx);
            if let Some(layout) = layout {
                dashboard.set_layout(layout, cx);
            }
            dashboard.back(cx);
        });
    }

    fn show_all(&mut self, _: &ShowAllTasks, _: &mut Window, cx: &mut Context<Self>) {
        self.show_list(None, cx);
    }

    fn show_agents(&mut self, _: &ShowAgents, _: &mut Window, cx: &mut Context<Self>) {
        self.close_tool(cx);
        self.drawer_open = false;
        self.dashboard
            .update(cx, |dashboard, cx| dashboard.open_agents(cx));
    }

    /// The status bar's agents toggle: the grid, or back to the task or list shown before it.
    fn toggle_agents(&mut self, _: &ToggleAgents, _: &mut Window, cx: &mut Context<Self>) {
        if self.agents_over_settings(cx) {
            return;
        }
        let dashboard = self.dashboard.read(cx);
        let step = agents_toggle(
            dashboard.agents_open,
            dashboard.open_task.as_deref(),
            self.agents_back.as_deref(),
        );
        self.close_tool(cx);
        self.drawer_open = false;
        match step {
            AgentsToggle::Open { back } => {
                self.agents_back = back;
                self.dashboard
                    .update(cx, |dashboard, cx| dashboard.open_agents(cx));
            }
            AgentsToggle::Back(back) => {
                self.agents_back = None;
                self.dashboard.update(cx, |dashboard, cx| match back {
                    Some(id) if dashboard.data.task(&id).is_some() => dashboard.open(&id, cx),
                    _ => dashboard.back(cx),
                });
            }
        }
    }

    fn show_table(&mut self, _: &ShowTable, _: &mut Window, cx: &mut Context<Self>) {
        self.show_list(Some(Layout::Table), cx);
    }

    fn show_board(&mut self, _: &ShowBoard, _: &mut Window, cx: &mut Context<Self>) {
        self.show_list(Some(Layout::Board), cx);
    }

    /// The running item filters All tasks to a state.
    fn show_state(&mut self, state: Option<TaskState>, cx: &mut Context<Self>) {
        self.show_list(None, cx);
        self.dashboard
            .update(cx, |dashboard, cx| dashboard.set_state_filter(state, cx));
    }

    fn new_workspace(&mut self, _: &NewWorkspace, window: &mut Window, cx: &mut Context<Self>) {
        new_workspace_dialog::open(&self.dashboard, None, window, cx);
    }

    /// `n` from anywhere outside a text field, the title bar's + menu and the palette: the Add
    /// task dialog over what main shows.
    fn new_task(&mut self, _: &NewTask, window: &mut Window, cx: &mut Context<Self>) {
        add_task_dialog::open(&self.dashboard, None, window, cx);
    }

    /// The state action keys work from the list, the tree and an open task.
    fn task_action(&mut self, event: &'static str, cx: &mut Context<Self>) {
        self.dashboard
            .update(cx, |dashboard, cx| dashboard.task_action(event, cx));
    }
}

#[path = "window_tools.rs"]
mod tools;

impl Focusable for Root {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

#[path = "window_render.rs"]
mod render;

#[path = "window_bars.rs"]
mod bars;
use bars::{StatusBarView, TitleBarView};

#[path = "window_details.rs"]
mod details;

#[path = "window_git.rs"]
mod git_commands;

#[path = "window_files.rs"]
mod file_commands;

#[path = "window_settings.rs"]
mod settings_commands;

#[path = "window_watch.rs"]
mod watch;

#[path = "window_follow.rs"]
mod follow;

pub fn open(
    options: &Options,
    source: Result<Arc<Source>, StartProblem>,
    saved: SavedView,
    cx: &mut App,
) -> anyhow::Result<gpui::WindowHandle<Root>> {
    let (width, height) = options.size.unwrap_or(DEFAULT_SIZE);
    let window_size = size(px(width as f32), px(height as f32));
    // A fixed place at the top left when a size is given, so CI can capture that part of the screen.
    let bounds = if options.size.is_some() {
        Bounds::new(point(px(0.), px(0.)), window_size)
    } else {
        Bounds::centered(None, window_size, cx)
    };
    let mac = cfg!(target_os = "macos");
    let smoke = options.smoke;
    let window = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(MIN_SIZE.0), px(MIN_SIZE.1))),
            // macOS: the app draws the title bar under the traffic lights and moves the window
            // itself (title_bar.rs). Elsewhere the system title bar stays.
            titlebar: Some(TitlebarOptions {
                title: Some(window_title(0).into()),
                appears_transparent: mac,
                // gpui centers the 16 px buttons at y + 8; 9 matches the 34 px bar, as in Zed.
                // x 9 as in Zed: ui's TRAFFIC_LIGHT_PADDING assumes it.
                traffic_light_position: mac.then(|| point(px(9.), px(9.))),
            }),
            app_owns_titlebar_drag: mac,
            ..Default::default()
        },
        |window, cx| {
            let root = cx.new(|cx| Root::new(source, saved, window, cx));
            // The smoke test types into the first text box; a user starts on the list, or on
            // Sign in when the app has no token.
            let signing_in = root.read(cx).sign_in.is_some();
            let focus = if smoke || signing_in {
                root.read(cx).text_box(cx).focus_handle(cx)
            } else {
                root.read(cx).tasks.focus_handle(cx)
            };
            window.focus(&focus, cx);
            root
        },
    )?;
    Ok(window)
}
