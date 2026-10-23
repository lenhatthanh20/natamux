//! The window and the root view, the app shell of docs/ui-spec.md 3.1 (`features/app/AppShell.tsx`):
//! title bar, the sidebar (docked, or a drawer under 1024), main and the status bar, with the menu,
//! dialog and toast layers on top. Main shows All tasks, the open task's workspace, the Agents
//! grid, or a tool of phase 1 (bench, IME check).

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use editor::Editor;
use gpui::{
    App, Bounds, Entity, FocusHandle, Focusable, TitlebarOptions, WindowBounds, WindowOptions,
    point, px, size,
};
use natamux_proto::tasks::task::{Task, TaskState};
use ui::prelude::*;

use super::dashboard::{Dashboard, DashboardChange, TerminalReader};
use super::layout_mode::{
    LayoutInput, LayoutMode, SidebarMode, decide_layout, effective_sidebar_width,
};
use super::notify::Notifier;
use super::options::Options;
use super::shell_rules::window_title;
use super::shortcuts::{
    FocusFilter, NewTask, NewWorkspace, ShowAgents, ShowAllTasks, ShowBoard, ShowTable,
    ToggleSidebar,
};
use super::source::Source;
use super::view_state::Layout;
use crate::bench::bench_view::BenchView;
use crate::components::dialog::DialogLayer;
use crate::components::menu::MenuLayer;
use crate::components::toasts::{Toaster, push_toast};
use crate::ime::ime_check::ImeCheck;
use crate::perf::overlay;
use crate::sidebar::sidebar::Sidebar;
use crate::tasks::details_panel::DetailsPanel;
use crate::tasks::quick_add::StartTask;
use crate::tasks::task_view::TaskView;
use crate::terminals::agent_panes::AgentPanes;
use crate::terminals::terminal_cache::{Owner, TerminalCache};
use crate::workspaces::new_workspace_dialog::{self, Created};
use crate::workspaces::workspace_view::WorkspaceView;
use crate::workspaces::wrap_up_rules::is_finished_state;

const DEFAULT_SIZE: (u32, u32) = (1280, 800);
const MIN_SIZE: (f32, f32) = (800., 560.);
/// How often the perf overlay shows new numbers.
const OVERLAY_REFRESH: Duration = Duration::from_millis(500);

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
    tool: Tool,
    /// `--gate2`: a bench whose shells keep printing while main shows All tasks.
    parked_bench: Option<Entity<BenchView>>,
    menus: Entity<MenuLayer>,
    dialogs: Entity<DialogLayer>,
    toaster: Entity<Toaster>,
    focus: FocusHandle,
    resize_focus: FocusHandle,
    details_resize: FocusHandle,
    notifier: Notifier,
    /// The sidebar drawer under 1024. The docked sidebar's open state is in the view state.
    drawer_open: bool,
    /// The id of the last dashboard toast shown, so each one shows once.
    toasts_shown: u64,
    title: String,
}

impl Root {
    fn new(
        source: Result<Arc<Source>, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let menus = MenuLayer::init(cx);
        let dialogs = DialogLayer::init(cx);
        let toaster = Toaster::init(cx);
        let dashboard = cx.new(|cx| Dashboard::new(source.clone(), cx));
        let sidebar = cx.new(|cx| Sidebar::new(dashboard.clone(), window, cx));
        let tasks = cx.new(|cx| TaskView::new(dashboard.clone(), window, cx));
        let source = source.ok();
        let workspace = cx.new(|cx| WorkspaceView::new(dashboard.clone(), cx));
        let agents = cx.new(|cx| AgentPanes::new(dashboard.clone(), cx));
        let details = cx.new(|cx| DetailsPanel::new(dashboard.clone(), window, cx));
        cx.subscribe_in(&dashboard, window, Self::on_dashboard)
            .detach();
        // Start a task of the quick add and of the "No tasks yet" state.
        cx.subscribe_in(&tasks, window, |root, _, event: &StartTask, window, cx| {
            root.start_task(event.0.clone(), window, cx)
        })
        .detach();
        cx.observe(&dashboard, |root, _, cx| {
            root.show_new_toasts(cx);
            cx.notify();
        })
        .detach();
        Self::watch_window(window, cx);
        Root {
            source,
            dashboard,
            sidebar,
            tasks,
            workspace,
            agents,
            details,
            tool: Tool::None,
            parked_bench: None,
            menus,
            dialogs,
            toaster,
            focus: cx.focus_handle(),
            resize_focus: cx.focus_handle().tab_stop(true),
            details_resize: cx.focus_handle().tab_stop(true),
            notifier: Notifier::default(),
            drawer_open: false,
            toasts_shown: 0,
            title: window_title(0),
        }
    }

    /// The bench's quit handling, the system look and the perf overlay's refresh.
    fn watch_window(window: &mut Window, cx: &mut Context<Self>) {
        // Closing the window drops Root before an app quit runs its handlers, so with a bench
        // running the window quits the app instead: the quit handler below then still finds Root.
        let weak_root = cx.weak_entity();
        window.on_window_should_close(cx, move |_, cx| {
            let bench_runs = weak_root
                .read_with(cx, |root, _| matches!(root.tool, Tool::Bench(_)))
                .unwrap_or(false);
            if bench_runs {
                cx.quit();
            }
            !bench_runs
        });
        // A bench left running would flood the daemon after the app is gone.
        cx.on_app_quit(|root, cx| {
            let stop = root.take_bench_stop(cx);
            async move {
                if let Some(stop) = stop {
                    stop.await;
                }
            }
        })
        .detach();
        // Follow the system's light or dark look, as Zed's workspace does.
        cx.observe_window_appearance(window, |_, window, cx| {
            *theme::SystemAppearance::global_mut(cx) =
                theme::SystemAppearance(window.appearance().into());
            theme_settings::reload_theme(cx);
        })
        .detach();
        cx.spawn(async move |root, cx| {
            loop {
                cx.background_executor().timer(OVERLAY_REFRESH).await;
                let refreshed = root.update(cx, |_, cx| {
                    if overlay::enabled(cx) {
                        cx.notify();
                    }
                });
                if refreshed.is_err() {
                    break;
                }
            }
        })
        .detach();
        if overlay::enabled(cx) {
            overlay::watch_frames(window);
        }
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
                let (main, active) = (window.window_handle(), window.is_window_active());
                self.notifier.after_tasks(&self.dashboard, main, active, cx);
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
            DashboardChange::WorkspaceChanged(_)
            | DashboardChange::ShellsChanged(_)
            | DashboardChange::TaskLogged(_) => {}
        }
    }

    /// The dashboard keeps its own toast queue; each new one shows once in the window's stack.
    fn show_new_toasts(&mut self, cx: &mut Context<Self>) {
        let fresh: Vec<_> = self
            .dashboard
            .read(cx)
            .toasts
            .toasts
            .iter()
            .filter(|toast| toast.id > self.toasts_shown)
            .cloned()
            .collect();
        for shown in fresh {
            self.toasts_shown = shown.id;
            push_toast(&shown.text, shown.tone, shown.detail, Vec::new(), cx);
        }
    }

    /// The text box of the sidebar (the smoke test types into it).
    pub fn filter(&self, cx: &App) -> Entity<Editor> {
        self.sidebar.read(cx).filter().clone()
    }

    pub(super) fn dashboard(&self) -> &Entity<Dashboard> {
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
        decide_layout(LayoutInput {
            width: Some(width),
            sidebar_width,
            details_width: view.details_width,
            coarse: false,
        })
    }

    /// The terminals give up their size while something else fills main. The workspace and the
    /// Agents grid claim theirs again when they draw.
    fn sync_pane(&mut self, cx: &mut Context<Self>) {
        let dashboard = self.dashboard.read(cx);
        let tool = !matches!(self.tool, Tool::None);
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

    /// The needs-you pill and "N need you" filter All tasks to every task that needs the user.
    fn show_needs_you(&mut self, cx: &mut Context<Self>) {
        self.show_list(None, cx);
        self.dashboard
            .update(cx, |dashboard, cx| dashboard.set_needs_you(true, cx));
    }

    fn new_workspace(&mut self, _: &NewWorkspace, window: &mut Window, cx: &mut Context<Self>) {
        new_workspace_dialog::open(&self.dashboard, None, window, cx);
    }

    /// The dialog starts with the typed title; once the task is made, the quick add gives the
    /// title up unless something else was typed since.
    fn start_task(&mut self, title: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let dialog =
            new_workspace_dialog::open_with(&self.dashboard, None, title.clone(), window, cx);
        let (Some(dialog), Some(title)) = (dialog, title) else {
            return;
        };
        let quick_add = self.tasks.read(cx).quick_add().clone();
        cx.subscribe_in(&dialog, window, move |_, _, _: &Created, window, cx| {
            if quick_add.read(cx).typed_title(cx).as_deref() == Some(title.as_str()) {
                quick_add.update(cx, |quick_add, cx| quick_add.clear(window, cx));
            }
        })
        .detach();
    }

    /// `n` from anywhere outside a text field: All tasks, then the backlog field once drawn.
    fn new_task(&mut self, _: &NewTask, window: &mut Window, cx: &mut Context<Self>) {
        self.show_list(None, cx);
        let quick_add = self.tasks.read(cx).quick_add().clone();
        window.on_next_frame(move |window, cx| {
            let field = quick_add.read(cx).focus_handle(cx);
            window.focus(&field, cx);
        });
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

#[path = "window_details.rs"]
mod details;

pub fn open(
    options: &Options,
    source: Result<Arc<Source>, String>,
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
                traffic_light_position: mac.then(|| point(px(12.), px(11.))),
            }),
            app_owns_titlebar_drag: mac,
            ..Default::default()
        },
        |window, cx| {
            let root = cx.new(|cx| Root::new(source, window, cx));
            // The smoke test types into the sidebar filter; a user starts on the list.
            let focus = if smoke {
                root.read(cx).filter(cx).focus_handle(cx)
            } else {
                root.read(cx).tasks.focus_handle(cx)
            };
            window.focus(&focus, cx);
            root
        },
    )?;
    Ok(window)
}
