//! The Start a task dialog (`NewWorkspaceDialog.tsx`, docs/ui-spec.md 4.14 and K-06): a task, its
//! git worktree on a new branch, and an agent in it. It closes once the daemon made the task; the
//! dashboard toasts "being prepared", then ready or failed (ui-spec 5.4).

use editor::{Editor, EditorEvent};
use gpui::{
    DismissEvent, Entity, EventEmitter, ExternalPaths, FocusHandle, Focusable, Subscription,
};
use natamux_client::Answer;
use natamux_proto::projects::repo::{BranchList, LoadedSettings, RemoteBranchList, SkillList};
use ui::prelude::*;

use super::new_workspace_rules::{
    PickedFile, SourceChoice, StartForm, blocked_reason, branch_text, initial_repo,
    new_workspace_input, pick_source, prefill_branch, read_uploads, shown_repo, source_choices,
    start_label,
};
use crate::agents::agent_choices::DEFAULT_AGENT;
use crate::app::dashboard::Dashboard;
use crate::app::dashboard_data::{Part, Seen};
use crate::app::source::reason;
use crate::components::dialog::{
    DialogFrame, DialogLayer, DialogView, cancel_button, primary_button,
};
use crate::components::form::text_editor;
use crate::components::toast_rules::ToastTone;

/// What Start a task does, in its tooltip.
const START_HINT: &str = "A new task with its own workspace: a git worktree on a new branch.";

/// The Branch field's placeholder when the project sets no `git.branchName`.
const DEFAULT_BRANCH_HINT: &str = "natamux/{id}-{slug}";

#[path = "new_workspace_fields.rs"]
mod fields;
#[path = "new_workspace_menus.rs"]
mod menus;
#[path = "new_workspace_trust.rs"]
mod trust;

use menus::Pick;

/// Opens the dialog, in `repo` when given, with the title focused. With no project yet it only
/// says so: the projects panel comes with a later screen.
pub fn open(
    dashboard: &Entity<Dashboard>,
    repo: Option<String>,
    window: &mut Window,
    cx: &mut App,
) {
    open_with(dashboard, repo, None, window, cx);
}

/// `open` with `title` in the title field: what was typed in the quick add. Gives the dialog, or
/// None when it did not open.
pub fn open_with(
    dashboard: &Entity<Dashboard>,
    repo: Option<String>,
    title: Option<String>,
    window: &mut Window,
    cx: &mut App,
) -> Option<Entity<NewWorkspaceDialog>> {
    let board = dashboard.read(cx);
    if board.data.loaded && board.data.repos.is_empty() {
        dashboard.update(cx, |dashboard, cx| {
            dashboard.toast("Add a project first.", ToastTone::Info, None, cx)
        });
        return None;
    }
    let dashboard = dashboard.clone();
    let view = cx.new(|cx| NewWorkspaceDialog::new(dashboard, repo, window, cx));
    let field = view.read(cx).title.clone();
    if let Some(title) = title {
        field.update(cx, |editor, cx| editor.set_text(title, window, cx));
    }
    DialogLayer::open(view.clone(), window, cx);
    window.focus(&field.focus_handle(cx), cx);
    Some(view)
}

/// The dialog made its task (`onCreated`).
pub struct Created;

pub struct NewWorkspaceDialog {
    dashboard: Entity<Dashboard>,
    title: Entity<Editor>,
    notes: Entity<Editor>,
    branch_name: Entity<Editor>,
    /// What the field holds from a refill: a text equal to it is not an edit by the user.
    filled: String,
    /// The user typed a branch: the title and project no longer refill it.
    branch_edited: bool,
    branch_hint: String,
    reference: Entity<Editor>,
    picked_repo: String,
    base: String,
    agent: String,
    picked_source: String,
    skill: String,
    files: Vec<PickedFile>,
    refused: Vec<String>,
    next_file: u64,
    branches: Option<BranchList>,
    /// The branches of origin; None while origin is asked.
    remote_branches: Option<Vec<String>>,
    settings: Option<LoadedSettings>,
    skills: Option<SkillList>,
    read_for: Option<String>,
    busy: bool,
    error: Option<String>,
    focus: FocusHandle,
    stops: Stops,
    _reads: Vec<gpui::Task<()>>,
    _subscriptions: Vec<Subscription>,
}

/// The focus stops that are not fields.
struct Stops {
    source: FocusHandle,
    skill: FocusHandle,
    files: FocusHandle,
    project: FocusHandle,
    branch: FocusHandle,
    agent: FocusHandle,
    cancel: FocusHandle,
    submit: FocusHandle,
}

impl NewWorkspaceDialog {
    fn new(
        dashboard: Entity<Dashboard>,
        repo: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let title = text_editor("Fix the login redirect", None, window, cx);
        let notes = text_editor(
            "Optional. What to change and why, where to look (files, screens), and how to tell \
             it is done.",
            Some((6, 12)),
            window,
            cx,
        );
        let reference = text_editor("", None, window, cx);
        let branch_name = text_editor(DEFAULT_BRANCH_HINT, None, window, cx);
        let ids = repo_ids(&dashboard, cx);
        let mut seen = Seen::default();
        let follow = cx.observe(&dashboard, move |this, dashboard, cx| {
            let gens = dashboard.read(cx).data.gens;
            if seen.changed(&gens, &[Part::Repos]) {
                this.read_repo(cx);
            }
        });
        let mut subscriptions = vec![follow];
        for field in [&title, &notes, &reference] {
            subscriptions.push(cx.subscribe(field, |_, _, event: &EditorEvent, cx| {
                if matches!(event, EditorEvent::BufferEdited) {
                    cx.notify();
                }
            }));
        }
        subscriptions.push(
            cx.subscribe(&branch_name, |this, editor, event: &EditorEvent, cx| {
                if matches!(event, EditorEvent::BufferEdited) {
                    let text = editor.read(cx).text(cx);
                    // A refill sets the text too; only another text is the user's.
                    if text != this.filled {
                        this.branch_edited = !text.is_empty();
                    }
                    cx.notify();
                }
            }),
        );
        let mut dialog = NewWorkspaceDialog {
            dashboard,
            title,
            notes,
            branch_name,
            filled: String::new(),
            branch_edited: false,
            branch_hint: DEFAULT_BRANCH_HINT.into(),
            reference,
            picked_repo: initial_repo(repo.as_deref(), &ids),
            base: String::new(),
            agent: DEFAULT_AGENT.into(),
            picked_source: "manual".into(),
            skill: String::new(),
            files: Vec::new(),
            refused: Vec::new(),
            next_file: 0,
            branches: None,
            remote_branches: None,
            settings: None,
            skills: None,
            read_for: None,
            busy: false,
            error: None,
            focus: cx.focus_handle(),
            stops: Stops {
                source: cx.focus_handle(),
                skill: cx.focus_handle(),
                files: cx.focus_handle(),
                project: cx.focus_handle(),
                branch: cx.focus_handle(),
                agent: cx.focus_handle(),
                cancel: cx.focus_handle(),
                submit: cx.focus_handle(),
            },
            _reads: Vec::new(),
            _subscriptions: subscriptions,
        };
        dialog.read_repo(cx);
        dialog
    }

    /// The project shown: the picked one, or the first while the picked one is gone.
    fn repo(&self, cx: &App) -> String {
        shown_repo(&self.picked_repo, &repo_ids(&self.dashboard, cx))
    }

    /// Reads the branches, settings and skills of the shown project once it changes.
    fn read_repo(&mut self, cx: &mut Context<Self>) {
        let repo = self.repo(cx);
        if self.read_for.as_deref() == Some(repo.as_str()) {
            return;
        }
        self.read_for = Some(repo.clone());
        self.branches = None;
        self.remote_branches = None;
        self.settings = None;
        self.skills = None;
        let Some(source) = self.dashboard.read(cx).source().cloned() else {
            return;
        };
        self._reads = vec![
            read(source.list_branches(Some(&repo)), cx, |this, list| {
                this.branches = Some(list)
            }),
            read_remote(source.list_remote_branches(&repo), cx),
            read(source.load_settings(&repo), cx, |this, settings| {
                this.settings = Some(settings)
            }),
            // Skills that fail to load offer no agent skill.
            read(source.list_skills(&repo), cx, |this, skills| {
                this.skills = Some(skills)
            }),
        ];
        cx.notify();
    }

    fn choices(&self) -> Vec<SourceChoice> {
        let configured = self
            .settings
            .as_ref()
            .map(|loaded| loaded.settings.context.sources.as_slice());
        let has_skills = self
            .skills
            .as_ref()
            .is_some_and(|list| !list.skills.is_empty());
        source_choices(configured, has_skills)
    }

    fn choice(&self) -> SourceChoice {
        pick_source(&self.choices(), &self.picked_source)
    }

    fn form(&self, cx: &App) -> StartForm {
        StartForm {
            title: self.title.read(cx).text(cx),
            repo: self.repo(cx),
            base: self.base.clone(),
            branch: self.branch_name.read(cx).text(cx),
            agent: self.agent.clone(),
            notes: self.notes.read(cx).text(cx),
            reference: self.reference.read(cx).text(cx),
            skill: self.skill.clone(),
        }
    }

    /// The project's `git.branchName`, once its settings are loaded.
    fn branch_pattern(&self) -> Option<String> {
        self.settings
            .as_ref()
            .and_then(|loaded| loaded.settings.git.branch_name.clone())
    }

    /// Runs on each render, so a title, project or settings change refills the branch with no
    /// event wiring. Once the user edited the field, it stays as typed.
    fn sync_branch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let pattern = self.branch_pattern();
        let hint = pattern
            .clone()
            .unwrap_or_else(|| DEFAULT_BRANCH_HINT.into());
        if hint != self.branch_hint {
            self.branch_hint = hint.clone();
            self.branch_name.update(cx, |editor, cx| {
                editor.set_placeholder_text(hint.as_str(), window, cx)
            });
        }
        if self.branch_edited {
            return;
        }
        let title = self.title.read(cx).text(cx);
        let value = prefill_branch(pattern.as_deref(), &title).unwrap_or_default();
        if self.branch_name.read(cx).text(cx) != value {
            self.filled = value.clone();
            self.branch_name
                .update(cx, |editor, cx| editor.set_text(value, window, cx));
        }
    }

    fn last_base(&self, cx: &App) -> Option<String> {
        let repo = self.repo(cx);
        self.dashboard
            .read(cx)
            .view()
            .last_base(&repo)
            .map(str::to_string)
    }

    fn start(&mut self, cx: &mut Context<Self>) {
        let form = self.form(cx);
        let choice = self.choice();
        if self.busy || blocked_reason(&form, &choice).is_some() {
            return;
        }
        self.busy = true;
        self.error = None;
        let files = self.files.clone();
        let dashboard = self.dashboard.clone();
        let uploads = cx
            .background_executor()
            .spawn(async move { read_uploads(&files) });
        cx.spawn(async move |this, cx| {
            let result = match uploads.await {
                Ok(uploads) => {
                    let input = new_workspace_input(&form, &choice, uploads);
                    let written =
                        dashboard.update(cx, |dashboard, cx| dashboard.new_workspace(input, cx));
                    written.await.map_err(|error| reason(&error))
                }
                Err(error) => Err(error),
            };
            match result {
                Ok(task) => {
                    // Opened even when the dialog was closed while it waited.
                    dashboard.update(cx, |dashboard, cx| dashboard.open(&task.id, cx));
                    this.update(cx, |_, cx| {
                        cx.emit(Created);
                        cx.emit(DismissEvent);
                    })
                    .ok();
                }
                Err(error) => {
                    this.update(cx, |this, cx| {
                        this.busy = false;
                        this.error = Some(error);
                        cx.notify();
                    })
                    .ok();
                }
            }
        })
        .detach();
        cx.notify();
    }
}

fn repo_ids(dashboard: &Entity<Dashboard>, cx: &App) -> Vec<String> {
    let board = dashboard.read(cx);
    board
        .data
        .repos
        .iter()
        .map(|repo| repo.id.clone())
        .collect()
}

/// Reads one answer for the dialog; an error leaves the part as not loaded.
fn read<T: 'static>(
    answer: Answer<T>,
    cx: &mut Context<NewWorkspaceDialog>,
    set: impl FnOnce(&mut NewWorkspaceDialog, T) + 'static,
) -> gpui::Task<()> {
    cx.spawn(async move |this, cx| {
        if let Ok(value) = answer.await {
            this.update(cx, |this, cx| {
                set(this, value);
                cx.notify();
            })
            .ok();
        }
    })
}

/// Reads the branches of origin. A failed read (no network) offers none, so the menu stops
/// saying they load.
fn read_remote(
    answer: Answer<RemoteBranchList>,
    cx: &mut Context<NewWorkspaceDialog>,
) -> gpui::Task<()> {
    cx.spawn(async move |this, cx| {
        let branches = answer.await.map(|list| list.branches).unwrap_or_default();
        this.update(cx, |this, cx| {
            this.remote_branches = Some(branches);
            cx.notify();
        })
        .ok();
    })
}

impl NewWorkspaceDialog {
    /// The title box, for the flow tests.
    #[cfg(test)]
    pub fn title_box(&self) -> &Entity<Editor> {
        &self.title
    }
}

impl EventEmitter<DismissEvent> for NewWorkspaceDialog {}

impl EventEmitter<Created> for NewWorkspaceDialog {}

impl Focusable for NewWorkspaceDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl DialogView for NewWorkspaceDialog {
    fn tab_ring(&self, cx: &App) -> Vec<FocusHandle> {
        let choice = self.choice();
        let mut ring = vec![self.title.focus_handle(cx)];
        if self.choices().len() > 1 {
            ring.push(self.stops.source.clone());
        }
        if choice == SourceChoice::AgentSkill {
            ring.push(self.stops.skill.clone());
        }
        let text = if choice.kind() == "manual" {
            &self.notes
        } else {
            &self.reference
        };
        ring.push(text.focus_handle(cx));
        let stops = &self.stops;
        ring.extend([&stops.files, &stops.project, &stops.agent].map(FocusHandle::clone));
        if self.branches.is_some() {
            ring.insert(ring.len() - 1, stops.branch.clone());
        }
        ring.insert(ring.len() - 1, self.branch_name.focus_handle(cx));
        ring.extend([stops.cancel.clone(), stops.submit.clone()]);
        ring
    }

    fn busy(&self, _: &App) -> bool {
        self.busy
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.start_checked(window, cx);
    }
}

impl Render for NewWorkspaceDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_branch(window, cx);
        let form = self.form(cx);
        let choice = self.choice();
        let missing = blocked_reason(&form, &choice);
        let close = cx.listener(|_, _, _, cx| cx.emit(DismissEvent));
        let cancel = cx.listener(|_, _, _, cx| cx.emit(DismissEvent));
        let submit = cx.listener(|this, _, window, cx| this.start_checked(window, cx));
        let stops = &self.stops;
        let start = primary_button(
            start_label(self.busy),
            &stops.submit,
            missing.is_some() || self.busy,
            submit,
        )
        .tooltip(ui::Tooltip::text(missing.unwrap_or(START_HINT)));
        let last_base = self.last_base(cx);
        let branch = branch_text(self.branches.as_ref(), &self.base, last_base.as_deref());
        let body = v_flex()
            .gap_5()
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                this.add_paths(paths.paths().to_vec(), cx)
            }))
            .child(self.render_context(&choice, cx))
            .child(self.render_setup(branch, cx))
            .children(
                self.error
                    .clone()
                    .map(|error| Label::new(error).color(Color::Error)),
            );
        div().track_focus(&self.focus).child(
            DialogFrame::new("Start a task", close)
                .child(body)
                .footer(cancel_button("Cancel", &stops.cancel, cancel))
                .footer(start),
        )
    }
}

#[cfg(test)]
#[path = "new_workspace_dialog_tests.rs"]
mod tests;
