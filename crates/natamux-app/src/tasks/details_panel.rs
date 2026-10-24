//! The details panel (docs/ui-spec.md 3.2 and 4.27, `features/tasks/TaskDetail.tsx`): a 40 px
//! header with the id, Edit, the "..." menu and Close, the Progress, Files (with a workspace) and
//! Details tabs, and the edit form over them. It keeps what it read for the task it shows, and
//! runs the clicks of the tabs.

use std::collections::BTreeSet;
use std::time::Duration;

use editor::Editor;
use gpui::{Entity, FocusHandle, Focusable, KeyContext, ScrollHandle, Subscription};
use natamux_proto::events::task_event::StoredEvent;
use natamux_proto::projects::repo::{BranchList, LoadedSettings};
use natamux_proto::stages::actions::StageFile;
use natamux_proto::tasks::task::Task;
use ui::{Tab, TabBar, TabPosition, Tooltip, prelude::*};

use super::edit_form::EditForm;
use crate::app::dashboard::details::PanelRequest;
use crate::app::dashboard::{Dashboard, DashboardChange};
use crate::app::layout_mode::DetailsMode;
use crate::app::settings_reads::{SettingsReads, SettingsRefreshed};
use crate::app::tab_rules::{ShowWatch, details_tabs};
use crate::app::view_rules::DetailsView;
use crate::components::chips::tip_with_keys;
use crate::files::file_tree::FileTree;

/// `FALLBACK_POLL_MS`: the feed says when the log changed; this covers a lost message.
const POLL: Duration = Duration::from_secs(60);

/// A read: not yet, done, or the error it gave.
pub type Read<T> = Option<Result<T, String>>;

/// What the panel keeps of one task; another task starts from nothing.
#[derive(Default)]
pub struct Shown {
    pub task_id: String,
    pub events: Read<Vec<StoredEvent>>,
    pub settings: Read<LoadedSettings>,
    pub files: Read<Vec<StageFile>>,
    pub branches: Option<BranchList>,
    pub base: Option<String>,
    /// A stage or task action is on its way: every action button is off (`busy`).
    pub busy: bool,
    pub resuming: bool,
    /// The stage settings being saved, as "<kind>:<stage>".
    pub toggling: BTreeSet<String>,
    /// Finished cards and paths not taken the user opened.
    pub open_cards: BTreeSet<String>,
    pub open_lanes: BTreeSet<String>,
    /// The agent picked on a card; None: the task's agent.
    pub agent: Option<String>,
    /// The current card was brought into view once.
    pub scrolled: bool,
    /// The task's data was read since the panel showed it: reads wait until it shows.
    pub read: bool,
    /// Cards whose "Show evidence" is open.
    pub open_evidence: BTreeSet<String>,
    /// The focus of each folded card, so Enter and Space open it.
    pub card_focus: std::collections::BTreeMap<String, FocusHandle>,
}

pub struct DetailsPanel {
    pub(super) dashboard: Entity<Dashboard>,
    pub(super) shown: Option<Shown>,
    pub(super) edit: Option<Entity<EditForm>>,
    /// The Files tab of the task shown, made when it first shows.
    pub(super) files: Option<Entity<FileTree>>,
    /// The Files tab showed in the last draw: a hidden tree reads again when it shows.
    pub(super) tree_shown: ShowWatch,
    pub(super) subtask: Entity<Editor>,
    pub(super) mode: DetailsMode,
    /// A request that needs the window: done on the next draw.
    pending: Option<PanelRequest>,
    /// A part of the tab to put in the middle once it is drawn.
    pub(super) center: Option<usize>,
    focus: FocusHandle,
    tabs_focus: FocusHandle,
    pub(super) scroll: ScrollHandle,
    _poll: gpui::Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl DetailsPanel {
    pub fn new(dashboard: Entity<Dashboard>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let subtask = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("Add a subtask", window, cx);
            editor
        });
        let changes = cx.subscribe(&dashboard, |panel, _, change: &DashboardChange, cx| {
            panel.on_change(change, cx)
        });
        let observe = cx.observe(&dashboard, |panel, _, cx| panel.sync(cx));
        let mut subscriptions = vec![changes, observe];
        if let Some(reads) = SettingsReads::global(cx) {
            subscriptions.push(
                cx.subscribe(&reads, |panel, _, event: &SettingsRefreshed, cx| {
                    panel.settings_refreshed(&event.0, cx)
                }),
            );
        }
        let poll = cx.spawn(async move |panel, cx| {
            loop {
                cx.background_executor().timer(POLL).await;
                let poll = |panel: &mut Self, cx: &mut Context<Self>| {
                    if panel.is_open(cx) {
                        panel.read_log(cx);
                        panel.refresh_tree(cx);
                    }
                };
                if panel.update(cx, poll).is_err() {
                    break;
                }
            }
        });
        DetailsPanel {
            dashboard,
            shown: None,
            edit: None,
            files: None,
            tree_shown: ShowWatch::default(),
            subtask,
            mode: DetailsMode::Docked,
            pending: None,
            center: None,
            focus: cx.focus_handle(),
            tabs_focus: cx.focus_handle().tab_stop(true),
            scroll: ScrollHandle::new(),
            _poll: poll,
            _subscriptions: subscriptions,
        }
    }

    pub fn set_mode(&mut self, mode: DetailsMode) {
        self.mode = mode;
    }

    pub(super) fn task(&self, cx: &App) -> Option<Task> {
        let id = &self.shown.as_ref()?.task_id;
        self.dashboard.read(cx).data.task(id).cloned()
    }

    /// The panel is on screen, in the mode Root gave it.
    pub(super) fn is_open(&self, cx: &App) -> bool {
        self.dashboard
            .read(cx)
            .details_shown(self.mode == DetailsMode::Docked)
    }

    fn on_change(&mut self, change: &DashboardChange, cx: &mut Context<Self>) {
        if !self.is_open(cx) {
            // The next time it shows, the panel reads everything again, the tree too.
            if let Some(shown) = self.shown.as_mut() {
                shown.read = false;
            }
            self.tree_shown.hide();
            return;
        }
        let ours = |id: &str| self.shown.as_ref().is_some_and(|shown| shown.task_id == id);
        match change {
            DashboardChange::TaskLogged(id) if ours(id) => self.read_log(cx),
            DashboardChange::WorkspaceChanged(id) if ours(id) => {
                self.read_files(cx);
                self.refresh_tree(cx);
            }
            _ => {}
        }
    }

    /// Follows the task the dashboard shows the details of, and takes a request meant for it.
    fn sync(&mut self, cx: &mut Context<Self>) {
        // A closed panel is not drawn: its tree reads again when it opens.
        if !self.is_open(cx) {
            self.tree_shown.hide();
        }
        let id = self
            .dashboard
            .read(cx)
            .details_task()
            .map(|task| task.id.clone());
        let same = self.shown.as_ref().map(|shown| &shown.task_id) == id.as_ref();
        if !same {
            self.edit = None;
            self.shown = id.clone().map(|task_id| Shown {
                task_id,
                ..Shown::default()
            });
        }
        let Some(id) = id else {
            return;
        };
        let request = self
            .dashboard
            .update(cx, |dashboard, _| dashboard.take_panel_request(&id));
        if request.is_some() {
            self.pending = request;
            cx.notify();
        }
    }

    /// A request opens once the window is there; Send back waits until the stages are read.
    fn take_pending(&mut self, task: &Task, window: &mut Window, cx: &mut Context<Self>) {
        let ready = match self.pending {
            Some(PanelRequest::Edit) => true,
            Some(PanelRequest::SendBack) => self.progress(task).is_some(),
            None => false,
        };
        let Some(request) = self.pending.take_if(|_| ready) else {
            return;
        };
        cx.defer_in(window, move |panel, window, cx| match request {
            PanelRequest::Edit => panel.open_edit(window, cx),
            PanelRequest::SendBack => {
                panel.send_back_waiting(window, cx);
            }
        });
    }

    /// The pencil and `e`: the form opens on the next draw, which has the window.
    pub(super) fn start_edit(&mut self, cx: &mut Context<Self>) {
        self.pending = Some(PanelRequest::Edit);
        cx.notify();
    }

    pub(super) fn view(&self, cx: &App) -> DetailsView {
        self.dashboard.read(cx).details_view_now()
    }

    fn render_header(&self, task: &Task, cx: &mut Context<Self>) -> impl IntoElement {
        let editing = self.edit.is_some();
        let page = self.mode == DetailsMode::Page;
        let close = if page {
            IconButton::new("details-back", IconName::ArrowLeft)
                .tooltip(Tooltip::text("Back"))
                .on_click(cx.listener(|panel, _, _, cx| panel.close(cx)))
        } else {
            IconButton::new("details-close", IconName::Close)
                .tooltip(tip_with_keys("Close details", "Escape"))
                .on_click(cx.listener(|panel, _, _, cx| panel.close(cx)))
        };
        h_flex()
            .h(rems_from_px(40.0_f32))
            .flex_none()
            .px_3()
            .gap_1()
            .border_b_1()
            .border_color(cx.theme().colors().border_variant)
            .child(
                Label::new(task.id.clone())
                    .size(LabelSize::Small)
                    .color(Color::Muted)
                    .buffer_font(cx),
            )
            .child(div().flex_1())
            .when(!editing, |this| {
                this.child(
                    IconButton::new("details-edit", IconName::Pencil)
                        .icon_size(IconSize::Small)
                        .tooltip(tip_with_keys("Edit", "e"))
                        .on_click(cx.listener(|panel, _, _, cx| panel.start_edit(cx))),
                )
            })
            .child(self.more_button(task, cx))
            .child(close)
    }

    fn render_tabs(
        &self,
        view: DetailsView,
        has_workspace: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let tabs = details_tabs(has_workspace);
        let last = tabs.len() - 1;
        let picked_at = tabs.iter().position(|tab| *tab == view).unwrap_or(0);
        let tab = |index: usize, id: &'static str, label: &'static str, this: DetailsView| {
            let picked = view == this;
            let position = match index {
                0 => TabPosition::First,
                _ if index == last => TabPosition::Last,
                _ => TabPosition::Middle(index.cmp(&picked_at)),
            };
            Tab::new(id)
                .position(position)
                .toggle_state(picked)
                .child(Label::new(label).color(if picked { Color::Default } else { Color::Muted }))
                .on_click(cx.listener(move |panel, _, _, cx| panel.pick_tab(this, cx)))
        };
        div()
            .id("details-tabs")
            .flex_none()
            .track_focus(&self.tabs_focus)
            .on_key_down(
                cx.listener(move |panel, event: &gpui::KeyDownEvent, _, cx| {
                    // One tab stop; the arrows move between the tabs.
                    let tabs = details_tabs(has_workspace);
                    let at = tabs.iter().position(|tab| *tab == view).unwrap_or(0);
                    let last = tabs.len() - 1;
                    let to = match event.keystroke.key.as_str() {
                        "left" => at.checked_sub(1).unwrap_or(last),
                        "right" => (at + 1) % tabs.len(),
                        "home" => 0,
                        "end" => last,
                        _ => return,
                    };
                    let next = tabs[to];
                    cx.stop_propagation();
                    panel.pick_tab(next, cx);
                }),
            )
            .child(
                TabBar::new("details-tab-bar").children(tabs.into_iter().enumerate().map(
                    |(index, this)| match this {
                        DetailsView::Progress => {
                            tab(index, "details-tab-progress", "Progress", this)
                        }
                        DetailsView::Files => tab(index, "details-tab-files", "Files", this),
                        DetailsView::Details => tab(index, "details-tab-details", "Details", this),
                    },
                )),
            )
    }

    fn pick_tab(&mut self, view: DetailsView, cx: &mut Context<Self>) {
        // The edit form stays over the tabs while it is open.
        if self.edit.is_some() {
            return;
        }
        self.dashboard
            .update(cx, |dashboard, cx| dashboard.set_details_view(view, cx));
    }

    /// The panel's text boxes keep the single keys.
    fn editing(&self, window: &Window, cx: &App) -> bool {
        self.edit.is_some() || self.subtask.focus_handle(cx).is_focused(window)
    }
}

impl Focusable for DetailsPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for DetailsPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(task) = self.task(cx) else {
            return div().into_any_element();
        };
        // Drawn means shown: the first draw of a task reads its data.
        if let Some(shown) = self.shown.as_mut()
            && !shown.read
        {
            shown.read = true;
            self.read_all(cx);
        }
        self.take_pending(&task, window, cx);
        let view = self.view(cx);
        let mut context = KeyContext::new_with_defaults();
        context.add("NatamuxDetails");
        if !self.editing(window, cx) {
            context.add("not_editing");
        }
        let has_workspace = task.workspace.is_some();
        // The tree scrolls by itself, under its own header.
        let tree = (self.edit.is_none() && view == DetailsView::Files)
            .then(|| self.files_body(&task.id, window, cx));
        if tree.is_none() {
            self.tree_shown.hide();
        }
        let body = match (&self.edit, view) {
            (Some(edit), _) => vec![edit.clone().into_any_element()],
            (None, DetailsView::Details) => self.render_details(&task, window, cx),
            (None, DetailsView::Files) => Vec::new(),
            (None, _) => self.render_progress(&task, cx),
        };
        if let Some(index) = self.center.take() {
            cx.on_next_frame(window, move |panel, _, cx| panel.center_on(index, cx));
        }
        v_flex()
            .id("details")
            .size_full()
            .key_context(context)
            .track_focus(&self.focus)
            .bg(cx.theme().colors().panel_background)
            .child(self.render_header(&task, cx))
            .child(self.render_tabs(view, has_workspace, cx))
            .map(|this| match tree {
                Some(tree) => this.child(v_flex().flex_1().min_h_0().child(tree)),
                None => this.child(
                    v_flex()
                        .id("details-body")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .track_scroll(&self.scroll)
                        .children(body),
                ),
            })
            .into_any_element()
    }
}

#[path = "details_reads.rs"]
mod reads;

#[path = "details_run.rs"]
mod run;

#[path = "details_calls.rs"]
mod calls;

#[path = "details_render.rs"]
mod render;

#[path = "details_tab.rs"]
mod tab;

#[path = "details_lists.rs"]
mod lists;

#[path = "details_files.rs"]
mod files;
