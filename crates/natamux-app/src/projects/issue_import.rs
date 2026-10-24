//! Import issues (docs/ui-spec.md 4.14, `IssueImport.tsx`): the open GitHub issues of a project,
//! to pick some and make a task of each. The dialog stays open after an import and reads again.

use std::collections::{BTreeMap, BTreeSet};

use gpui::{DismissEvent, Entity, EventEmitter, FocusHandle, Focusable};
use natamux_proto::projects::issues::GitHubIssue;
use ui::{ButtonLike, Checkbox, Tooltip, prelude::*};

use super::issue_rules::{import_label, imported_text, issue_problem};
use super::projects_dialog::alert_line;
use crate::app::dashboard::Dashboard;
use crate::app::source::reason;
use crate::components::chips::label_chip;
use crate::components::dialog::{
    DialogFrame, DialogLayer, DialogView, cancel_button, primary_button,
};
use crate::components::empty_state::EmptyState;
use crate::components::skeleton::SkeletonList;
use crate::components::toast_rules::ToastTone;

/// Opens the dialog for `repo`; None is the daemon's default project.
pub fn open(
    dashboard: &Entity<Dashboard>,
    repo: Option<String>,
    window: &mut Window,
    cx: &mut App,
) {
    let dashboard = dashboard.clone();
    let view = cx.new(|cx| IssueImport::new(dashboard, repo, cx));
    let first = view.read(cx).focus.clone();
    DialogLayer::open(view, window, cx);
    window.focus(&first, cx);
}

enum Issues {
    Loading,
    Failed(String),
    Ready(Vec<GitHubIssue>),
}

pub struct IssueImport {
    dashboard: Entity<Dashboard>,
    repo: Option<String>,
    issues: Issues,
    picked: BTreeSet<u64>,
    reading: bool,
    importing: bool,
    import_error: Option<String>,
    focus: FocusHandle,
    retry_focus: FocusHandle,
    close_focus: FocusHandle,
    submit_focus: FocusHandle,
    row_stops: BTreeMap<u64, FocusHandle>,
    _read: Option<gpui::Task<()>>,
}

impl IssueImport {
    fn new(dashboard: Entity<Dashboard>, repo: Option<String>, cx: &mut Context<Self>) -> Self {
        let mut dialog = IssueImport {
            dashboard,
            repo,
            issues: Issues::Loading,
            picked: BTreeSet::new(),
            reading: false,
            importing: false,
            import_error: None,
            focus: cx.focus_handle(),
            retry_focus: cx.focus_handle(),
            close_focus: cx.focus_handle(),
            submit_focus: cx.focus_handle(),
            row_stops: BTreeMap::new(),
            _read: None,
        };
        dialog.read(cx);
        dialog
    }

    /// Asks GitHub for the open issues. The list on screen stays until the answer comes.
    fn read(&mut self, cx: &mut Context<Self>) {
        let Some(source) = self.dashboard.read(cx).source().cloned() else {
            return;
        };
        self.reading = true;
        let answer = source.list_issues(self.repo.as_deref());
        self._read = Some(cx.spawn(async move |this, cx| {
            let result = answer.await.map_err(|error| reason(&error));
            this.update(cx, |this, cx| {
                this.reading = false;
                this.issues = match result {
                    Ok(list) => Issues::Ready(list),
                    Err(message) => Issues::Failed(message),
                };
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    fn toggle(&mut self, number: u64, cx: &mut Context<Self>) {
        if !self.picked.remove(&number) {
            self.picked.insert(number);
        }
        cx.notify();
    }

    fn import(&mut self, cx: &mut Context<Self>) {
        if self.importing || self.picked.is_empty() {
            return;
        }
        let Some(source) = self.dashboard.read(cx).source().cloned() else {
            return;
        };
        self.importing = true;
        self.import_error = None;
        let numbers: Vec<u64> = self.picked.iter().copied().collect();
        let answer = source.import_issues(&numbers, self.repo.as_deref());
        cx.spawn(async move |this, cx| {
            let result = answer.await.map_err(|error| reason(&error));
            this.update(cx, |this, cx| {
                this.importing = false;
                match result {
                    Ok(tasks) => this.imported(tasks.len(), cx),
                    Err(message) => this.import_error = Some(message),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn imported(&mut self, count: usize, cx: &mut Context<Self>) {
        self.picked.clear();
        self.read(cx);
        self.dashboard.update(cx, |dashboard, cx| {
            dashboard.toast(&imported_text(count), ToastTone::Success, None, cx)
        });
    }

    fn issue_row(&self, issue: &GitHubIssue, cx: &mut Context<Self>) -> impl IntoElement {
        let number = issue.number;
        let imported = issue.task_id.is_some();
        let checked = imported || self.picked.contains(&number);
        let title = if issue.title.is_empty() {
            "(no title)"
        } else {
            &issue.title
        };
        let mut row = ButtonLike::new(SharedString::from(format!("issue-{number}")))
            .full_width()
            .style(ButtonStyle::Subtle)
            .disabled(imported)
            .on_click(cx.listener(move |this, _, _, cx| this.toggle(number, cx)));
        if let Some(stop) = self.row_stops.get(&number) {
            row = row.track_focus(stop);
        }
        row.child(
            h_flex()
                .w_full()
                .gap_3()
                .child(
                    Checkbox::new(
                        SharedString::from(format!("issue-box-{number}")),
                        checked.into(),
                    )
                    .visualization_only(true),
                )
                .child(
                    div().w(rems_from_px(48.0_f32)).flex_none().child(
                        Label::new(format!("#{number}"))
                            .color(Color::Muted)
                            .buffer_font(cx),
                    ),
                )
                .child(div().min_w_0().flex_1().child(Label::new(title).truncate()))
                .children(issue.labels.iter().map(|label| label_chip(label.clone())))
                .children(
                    issue
                        .task_id
                        .as_ref()
                        .map(|id| Label::new(id.clone()).color(Color::Muted).buffer_font(cx)),
                ),
        )
    }

    fn failed(&self, message: &str, cx: &mut Context<Self>) -> impl IntoElement {
        EmptyState::error(
            "issues-error",
            "Could not load the issues",
            message.to_owned(),
        )
        .body(issue_problem(message))
        .action(
            Button::new("issues-retry", "Retry")
                .track_focus(&self.retry_focus)
                .disabled(self.reading)
                .on_click(cx.listener(|this, _, _, cx| this.read(cx))),
        )
    }

    fn list(&mut self, cx: &mut Context<Self>) -> AnyElement {
        match &self.issues {
            Issues::Loading => SkeletonList::new(6, 28.).into_any_element(),
            Issues::Failed(message) => {
                let message = message.clone();
                self.failed(&message, cx).into_any_element()
            }
            Issues::Ready(list) if list.is_empty() => Label::new("No open issues.")
                .color(Color::Muted)
                .into_any_element(),
            Issues::Ready(list) => {
                let list = list.clone();
                for issue in &list {
                    self.row_stops
                        .entry(issue.number)
                        .or_insert_with(|| cx.focus_handle());
                }
                let mut rows = Vec::new();
                for issue in &list {
                    rows.push(self.issue_row(issue, cx).into_any_element());
                }
                v_flex().children(rows).into_any_element()
            }
        }
    }
}

impl EventEmitter<DismissEvent> for IssueImport {}

impl Focusable for IssueImport {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl DialogView for IssueImport {
    fn tab_ring(&self, _: &App) -> Vec<FocusHandle> {
        let mut ring = Vec::new();
        match &self.issues {
            Issues::Failed(_) => ring.push(self.retry_focus.clone()),
            Issues::Ready(list) => ring.extend(
                list.iter()
                    .filter(|issue| issue.task_id.is_none())
                    .filter_map(|issue| self.row_stops.get(&issue.number).cloned()),
            ),
            Issues::Loading => {}
        }
        ring.extend([self.close_focus.clone(), self.submit_focus.clone()]);
        ring
    }

    fn busy(&self, _: &App) -> bool {
        self.importing
    }

    fn submit(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.import(cx);
    }
}

impl Render for IssueImport {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let count = self.picked.len();
        let import = primary_button(
            import_label(count),
            &self.submit_focus,
            false,
            count == 0 || self.importing,
            cx.listener(|this, _, _, cx| this.import(cx)),
        )
        .when(count == 0, |button| {
            button.tooltip(Tooltip::text("Pick an issue first"))
        });
        let close = cancel_button(
            "Close",
            &self.close_focus,
            cx.listener(|_, _, _, cx| cx.emit(DismissEvent)),
        );
        let error = self.import_error.clone().map(alert_line);
        let list = self.list(cx);
        div().track_focus(&self.focus).child(
            DialogFrame::new(
                "Import GitHub issues",
                cx.listener(|_, _, _, cx| cx.emit(DismissEvent)),
            )
            .description("Open GitHub issues of this project. Each one you pick becomes a task.")
            .children(error)
            .child(list)
            .footer(close)
            .footer(import),
        )
    }
}
