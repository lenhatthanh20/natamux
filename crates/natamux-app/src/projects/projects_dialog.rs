//! The Projects dialog (docs/ui-spec.md 4.14, `RepoPanel.tsx`): the projects the daemon manages
//! and a form to add one by its folder on the daemon's machine. The list follows the feed's
//! `repos.changed`, so the dialog stays open after an add.

use editor::{Editor, EditorEvent};
use gpui::{DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, FontWeight, Subscription};
use natamux_proto::projects::repo::{AddRepo, Repo};
use ui::{Tooltip, prelude::*};

use browse::Pick;

use super::project_rules::{
    RepoField, add_args, field_of_error, repo_path_problem, shown_worktree,
};
use crate::app::dashboard::Dashboard;
use crate::app::dashboard_data::{Part, Seen};
use crate::app::source::reason;
use crate::components::button_style::quiet_button;
use crate::components::dialog::{DialogFrame, DialogLayer, DialogView, primary_button};
use crate::components::form::{field, text_editor};
use crate::components::icons::icon_path;

/// What a project is, in the Add project tooltip.
const DESCRIPTION: &str = "Git repositories on the daemon's machine. The workspace of each task is a git worktree of one of them.";

pub fn open(dashboard: &Entity<Dashboard>, window: &mut Window, cx: &mut App) {
    let dashboard = dashboard.clone();
    let view = cx.new(|cx| ProjectsDialog::new(dashboard, window, cx));
    let first = view.read(cx).folder.focus_handle(cx);
    DialogLayer::open(view, window, cx);
    window.focus(&first, cx);
}

pub struct ProjectsDialog {
    dashboard: Entity<Dashboard>,
    folder: Entity<Editor>,
    name: Entity<Editor>,
    worktree: Entity<Editor>,
    /// None until the Worktree folder is edited, so the daemon's default shows even when it comes
    /// after the dialog opened.
    typed_worktree: Option<String>,
    folder_touched: bool,
    worktree_touched: bool,
    folder_error: Option<String>,
    worktree_error: Option<String>,
    busy: bool,
    /// A Browse… picker is open: a second click does not open another.
    picking: Option<Pick>,
    focus: FocusHandle,
    submit_focus: FocusHandle,
    folder_browse: FocusHandle,
    worktree_browse: FocusHandle,
    settings_stops: Vec<FocusHandle>,
    _subscriptions: Vec<Subscription>,
}

impl ProjectsDialog {
    fn new(dashboard: Entity<Dashboard>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let folder = text_editor("/path/to/the/git/repository", None, window, cx);
        let name = text_editor(
            "Optional. The folder name is used without one.",
            None,
            window,
            cx,
        );
        let worktree = text_editor(
            "Optional. The daemon's worktree folder is used without one.",
            None,
            window,
            cx,
        );
        // The projects and the daemon's worktree folder.
        let mut seen = Seen::default();
        let follow = cx.observe_in(&dashboard, window, move |this, dashboard, window, cx| {
            let gens = dashboard.read(cx).data.gens;
            if seen.changed(&gens, &[Part::Repos]) {
                this.show_default_worktree(window, cx);
            }
        });
        let mut subscriptions = vec![follow];
        subscriptions.push(cx.subscribe(&folder, |this, _, event: &EditorEvent, cx| {
            if matches!(event, EditorEvent::BufferEdited) {
                this.folder_error = None;
                cx.notify();
            }
        }));
        subscriptions.push(cx.subscribe(&name, |_, _, event: &EditorEvent, cx| {
            if matches!(event, EditorEvent::BufferEdited) {
                cx.notify();
            }
        }));
        subscriptions.push(
            cx.subscribe(&worktree, |this, editor, event: &EditorEvent, cx| {
                if !matches!(event, EditorEvent::BufferEdited) {
                    return;
                }
                // The default put into the field is not an edit.
                let text = editor.read(cx).text(cx);
                if text != this.shown_worktree(cx) {
                    this.typed_worktree = Some(text);
                    this.worktree_error = None;
                }
                cx.notify();
            }),
        );
        subscriptions.push(cx.on_blur(&folder.focus_handle(cx), window, |this, _, cx| {
            this.folder_touched = true;
            cx.notify();
        }));
        subscriptions.push(
            cx.on_blur(&worktree.focus_handle(cx), window, |this, _, cx| {
                this.worktree_touched = true;
                cx.notify();
            }),
        );
        let mut dialog = ProjectsDialog {
            dashboard,
            folder,
            name,
            worktree,
            typed_worktree: None,
            folder_touched: false,
            worktree_touched: false,
            folder_error: None,
            worktree_error: None,
            busy: false,
            picking: None,
            focus: cx.focus_handle(),
            submit_focus: cx.focus_handle(),
            folder_browse: cx.focus_handle(),
            worktree_browse: cx.focus_handle(),
            settings_stops: Vec::new(),
            _subscriptions: subscriptions,
        };
        dialog.show_default_worktree(window, cx);
        dialog
    }

    fn default_worktree(&self, cx: &App) -> Option<String> {
        self.dashboard.read(cx).data.worktrees_dir.clone()
    }

    fn shown_worktree(&self, cx: &App) -> String {
        shown_worktree(
            self.typed_worktree.as_deref(),
            self.default_worktree(cx).as_deref(),
        )
    }

    /// The field shows the daemon's default until the user edits it.
    fn show_default_worktree(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let wanted = self.shown_worktree(cx);
        if self.worktree.read(cx).text(cx) != wanted {
            self.worktree
                .update(cx, |editor, cx| editor.set_text(wanted, window, cx));
        }
        cx.notify();
    }

    /// The text under a field: the full-path rule after the field was left, and the daemon's
    /// answer at once.
    fn problems(&self, cx: &App) -> (Option<String>, Option<String>) {
        let rule = |touched: bool, editor: &Entity<Editor>| {
            let text = editor.read(cx).text(cx);
            touched
                .then(|| repo_path_problem(&text))
                .flatten()
                .map(str::to_owned)
        };
        (
            rule(self.folder_touched, &self.folder).or_else(|| self.folder_error.clone()),
            rule(self.worktree_touched, &self.worktree).or_else(|| self.worktree_error.clone()),
        )
    }

    fn can_add(&self, cx: &App) -> bool {
        !self.folder.read(cx).text(cx).trim().is_empty() && !self.busy
    }

    fn add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (path, name, tree) = (
            self.folder.read(cx).text(cx),
            self.name.read(cx).text(cx),
            self.worktree.read(cx).text(cx),
        );
        if !self.can_add(cx) {
            return;
        }
        if repo_path_problem(&path).is_some() || repo_path_problem(&tree).is_some() {
            self.folder_touched = true;
            self.worktree_touched = true;
            cx.notify();
            return;
        }
        let Some(source) = self.dashboard.read(cx).source().cloned() else {
            return;
        };
        let default = self.default_worktree(cx);
        let (path, name, worktrees_dir) = add_args(&path, &name, &tree, default.as_deref());
        self.busy = true;
        self.folder_error = None;
        self.worktree_error = None;
        let answer = source.add_repo(&AddRepo {
            path,
            name,
            worktrees_dir,
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = answer.await;
            this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Ok(_) => this.reset(window, cx),
                    Err(error) => this.refused(reason(&error)),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// The fields start again; the worktree folder is back to the default.
    fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.folder
            .update(cx, |editor, cx| editor.clear(window, cx));
        self.name.update(cx, |editor, cx| editor.clear(window, cx));
        self.typed_worktree = None;
        self.folder_touched = false;
        self.worktree_touched = false;
        self.show_default_worktree(window, cx);
    }

    fn refused(&mut self, message: String) {
        match field_of_error(&message) {
            RepoField::Worktree => self.worktree_error = Some(message),
            RepoField::Folder => self.folder_error = Some(message),
        }
    }

    fn repos(&self, cx: &App) -> Vec<Repo> {
        self.dashboard.read(cx).data.repos.clone()
    }

    fn repo_row(&self, index: usize, repo: &Repo, cx: &mut Context<Self>) -> impl IntoElement {
        let id = repo.id.clone();
        let mut button = quiet_button(
            SharedString::from(format!("settings-{}", repo.id)),
            "Settings",
        )
        .label_size(LabelSize::Small)
        .tooltip(Tooltip::text(format!("Settings of {}", repo.name)))
        .on_click(cx.listener(move |this, _, _, cx| {
            // The dialog closes; Settings shows the project's page.
            cx.emit(DismissEvent);
            this.dashboard
                .update(cx, |dashboard, cx| dashboard.open_project_settings(&id, cx));
        }));
        if let Some(stop) = self.settings_stops.get(index) {
            button = button.track_focus(stop);
        }
        h_flex()
            .gap_3()
            .px_2()
            .py_1p5()
            .rounded_md()
            .child(
                Icon::from_path(icon_path("folder-git-2"))
                    .size(IconSize::Small)
                    .color(Color::Muted),
            )
            .child(Label::new(repo.name.clone()).weight(FontWeight::MEDIUM))
            .child(
                v_flex()
                    .min_w_0()
                    .flex_1()
                    .items_end()
                    .child(mono_line(repo.path.clone(), cx))
                    .children(
                        repo.worktrees_dir
                            .as_ref()
                            .map(|dir| mono_line(format!("Worktrees in {dir}"), cx)),
                    ),
            )
            .child(button)
    }
}

fn mono_line(text: impl Into<SharedString>, cx: &App) -> Label {
    Label::new(text)
        .size(LabelSize::XSmall)
        .color(Color::Muted)
        .buffer_font(cx)
        .truncate()
}

/// An error line: the alert icon and the text in the danger colour.
pub(super) fn alert_line(text: impl Into<SharedString>) -> impl IntoElement {
    h_flex()
        .items_start()
        .gap_1p5()
        .child(
            Icon::from_path(icon_path("circle-alert"))
                .size(IconSize::XSmall)
                .color(Color::Error),
        )
        .child(Label::new(text).size(LabelSize::Small).color(Color::Error))
}

/// An input box whose border turns danger while it has an error.
fn field_box(editor: &Entity<Editor>, invalid: bool, cx: &App) -> impl IntoElement {
    let colors = cx.theme().colors();
    let border = if invalid {
        cx.theme().status().error
    } else {
        colors.border
    };
    div()
        .px_2p5()
        .py_1p5()
        .rounded_md()
        .border_1()
        .border_color(border)
        .bg(colors.editor_background)
        .child(editor.clone())
}

/// A folder field: the box with Browse… after it, and the problem under them.
fn problem_field(
    label: &'static str,
    editor: &Entity<Editor>,
    problem: Option<String>,
    browse: Button,
    cx: &App,
) -> impl IntoElement {
    field(
        label,
        v_flex()
            .gap_1()
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .child(field_box(editor, problem.is_some(), cx)),
                    )
                    .child(browse),
            )
            .children(problem.map(alert_line)),
    )
}

impl EventEmitter<DismissEvent> for ProjectsDialog {}

impl Focusable for ProjectsDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl DialogView for ProjectsDialog {
    fn tab_ring(&self, cx: &App) -> Vec<FocusHandle> {
        let repos = self.repos(cx).len().min(self.settings_stops.len());
        let mut ring: Vec<FocusHandle> = self.settings_stops[..repos].to_vec();
        ring.extend([
            self.folder.focus_handle(cx),
            self.folder_browse.clone(),
            self.name.focus_handle(cx),
            self.worktree.focus_handle(cx),
            self.worktree_browse.clone(),
            self.submit_focus.clone(),
        ]);
        ring
    }

    fn busy(&self, _: &App) -> bool {
        self.busy
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.add(window, cx);
    }
}

impl Render for ProjectsDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let repos = self.repos(cx);
        while self.settings_stops.len() < repos.len() {
            self.settings_stops.push(cx.focus_handle());
        }
        let (folder_problem, worktree_problem) = self.problems(cx);
        let empty = !self.can_add(cx);
        let add = primary_button(
            "Add project",
            &self.submit_focus,
            empty || self.busy,
            cx.listener(|this, _, window, cx| this.add(window, cx)),
        )
        .tooltip(Tooltip::text(
            if self.folder.read(cx).text(cx).trim().is_empty() {
                "Enter the folder first"
            } else {
                DESCRIPTION
            },
        ));
        let mut rows = Vec::new();
        for (index, repo) in repos.iter().enumerate() {
            rows.push(self.repo_row(index, repo, cx).into_any_element());
        }
        let list = v_flex().children(rows);
        let line = cx.theme().colors().border_variant;
        let browse_folder = self.browse_button(Pick::Folder, cx);
        let browse_worktree = self.browse_button(Pick::Worktree, cx);
        let form = v_flex()
            .gap_2()
            .pt_4()
            .border_t_1()
            .border_color(line)
            .child(problem_field(
                "Folder",
                &self.folder,
                folder_problem,
                browse_folder,
                cx,
            ))
            .child(field("Name", field_box(&self.name, false, cx)))
            .child(problem_field(
                "Worktree folder",
                &self.worktree,
                worktree_problem,
                browse_worktree,
                cx,
            ));
        div().track_focus(&self.focus).child(
            DialogFrame::new("Projects", cx.listener(|_, _, _, cx| cx.emit(DismissEvent)))
                .child(list)
                .child(form)
                .footer(add),
        )
    }
}

#[path = "projects_browse.rs"]
mod browse;

#[cfg(test)]
#[path = "projects_dialog_tests.rs"]
mod tests;
