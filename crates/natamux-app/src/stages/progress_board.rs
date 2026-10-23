//! The Progress layout of All tasks (`ProgressBoard.tsx`, ui-spec 4.31): every open task against
//! the stages of its pipeline, one box per project. The Task and Next columns stay in place while
//! the stage columns scroll sideways, so the box is three parts that share one row height. The
//! rules are `progress_board_rules.rs`; the settings are read by `app/settings_reads.rs`.

use std::collections::{BTreeMap, BTreeSet};

use gpui::{Entity, FocusHandle, Focusable, FontWeight, Hsla, Subscription};
use natamux_proto::stages::schema::Step;
use natamux_proto::tasks::task::Task;
use ui::prelude::*;

use self::cells::head_label;
use super::progress_board_rules::{Section, board_sections, stage_columns};
use crate::app::clock::now_ms;
use crate::app::dashboard::Dashboard;
use crate::app::settings_reads::SettingsReads;
use crate::app::view_rules::WorkspaceTab;
use crate::components::skeleton::SkeletonList;
use crate::sidebar::workspace_groups::RepoRef;

const TASK_WIDTH: f32 = 240.;
const NEXT_WIDTH: f32 = 120.;
const STAGE_WIDTH: f32 = 72.;
const ROW_HEIGHT: f32 = 52.;
const HEAD_HEIGHT: f32 = 56.;

pub struct ProgressBoard {
    dashboard: Entity<Dashboard>,
    reads: Option<Entity<SettingsReads>>,
    /// In the order to show, filtered; the task view holds the order still.
    rows: Vec<Task>,
    /// Sections whose done tasks are shown (not kept).
    expanded: BTreeSet<String>,
    hovered: Option<String>,
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

#[path = "progress_board_cells.rs"]
mod cells;

impl Focusable for ProgressBoard {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

/// What the settings reads say: the pipelines read, the first failure, and whether one is
/// still to come.
struct SettingsState {
    pipelines: BTreeMap<String, Vec<Step>>,
    failed: Option<String>,
    loading: bool,
}

impl ProgressBoard {
    pub fn new(dashboard: Entity<Dashboard>, cx: &mut Context<Self>) -> Self {
        let reads = SettingsReads::global(cx);
        let mut subscriptions = vec![cx.observe(&dashboard, |_, _, cx| cx.notify())];
        if let Some(reads) = &reads {
            subscriptions.push(cx.observe(reads, |_, _, cx| cx.notify()));
        }
        ProgressBoard {
            dashboard,
            reads,
            rows: Vec::new(),
            expanded: BTreeSet::new(),
            hovered: None,
            focus: cx.focus_handle(),
            _subscriptions: subscriptions,
        }
    }

    pub fn set_rows(&mut self, rows: Vec<Task>, cx: &mut Context<Self>) {
        if self.rows != rows {
            self.rows = rows;
            cx.notify();
        }
    }

    /// Reads the settings of the projects not read yet. The task view calls it while the layout
    /// shows, so a project that comes later is read too.
    pub fn read_settings(&self, cx: &mut Context<Self>) {
        let Some(reads) = &self.reads else {
            return;
        };
        let ids: Vec<String> = self
            .dashboard
            .read(cx)
            .data
            .repos
            .iter()
            .map(|repo| repo.id.clone())
            .collect();
        reads.update(cx, |reads, cx| reads.ensure(&ids, cx));
    }

    /// `onOpenTask` of the dashboard: select the task, open its workspace on the Terminal tab and
    /// show its details panel on Progress.
    fn open_task(&mut self, id: &str, cx: &mut Context<Self>) {
        self.dashboard.update(cx, |dashboard, cx| {
            dashboard.select(Some(id.to_owned()), cx);
            dashboard.open(id, cx);
            dashboard.pick_tab(WorkspaceTab::Terminal, cx);
            dashboard.show_progress(id, cx);
        });
    }

    fn set_hover(&mut self, id: &str, hovered: bool, cx: &mut Context<Self>) {
        if hovered && self.hovered.as_deref() != Some(id) {
            self.hovered = Some(id.to_owned());
            cx.notify();
        } else if !hovered && self.hovered.as_deref() == Some(id) {
            self.hovered = None;
            cx.notify();
        }
    }

    fn toggle_done(&mut self, key: &str, cx: &mut Context<Self>) {
        if !self.expanded.remove(key) {
            self.expanded.insert(key.to_owned());
        }
        cx.notify();
    }

    fn settings_state(&self, repos: &[RepoRef], cx: &App) -> SettingsState {
        let mut state = SettingsState {
            pipelines: BTreeMap::new(),
            failed: None,
            loading: false,
        };
        let reads = self.reads.as_ref().map(|reads| reads.read(cx));
        for repo in repos {
            let Some(read) = reads.and_then(|reads| reads.read_of(&repo.id)) else {
                state.loading = true;
                continue;
            };
            if let Some(data) = &read.data {
                state
                    .pipelines
                    .insert(repo.id.clone(), data.settings.stages.clone());
            }
            if state.failed.is_none() {
                state.failed = read.error.clone();
            }
            state.loading |= read.pending();
        }
        state
    }

    fn retry(&mut self, cx: &mut Context<Self>) {
        if let Some(reads) = &self.reads {
            reads.update(cx, |reads, cx| reads.retry_failed(cx));
        }
    }

    fn alert(&self, message: &str, cx: &Context<Self>) -> impl IntoElement + use<> {
        h_flex()
            .flex_wrap()
            .gap_2()
            .child(
                Label::new(format!(
                    "Could not read the stage settings, so the stages shown are the ones each task carries. {message}"
                ))
                .size(LabelSize::Small)
                .color(Color::Error),
            )
            .child(
                Button::new("progress-retry", "Retry")
                    .style(ButtonStyle::Filled)
                    .size(ButtonSize::Compact)
                    .on_click(cx.listener(|this, _, _, cx| this.retry(cx))),
            )
    }
}

impl ProgressBoard {
    fn section(&self, section: &Section, now: i64, cx: &Context<Self>) -> AnyElement {
        let colors = cx.theme().colors();
        let columns = stage_columns(&section.steps, &section.open);
        let mut rows: Vec<&Task> = section.open.iter().collect();
        if self.expanded.contains(&section.key) {
            rows.extend(section.done.iter());
        }
        let head_border = colors.border_variant;
        let (surface, border) = (colors.surface_background, colors.border);
        let left = v_flex()
            .flex_none()
            .w(px(TASK_WIDTH))
            .child(fixed_head("Task", false, head_border))
            .children(rows.iter().map(|task| self.task_cell(task, now, cx)));
        let middle = v_flex()
            .w_full()
            .min_w(px(columns.len() as f32 * STAGE_WIDTH))
            .child(
                h_flex()
                    .h(px(HEAD_HEIGHT))
                    .border_b_1()
                    .border_color(head_border)
                    .children(columns.iter().map(|column| self.column_head(column, cx))),
            )
            .children(
                rows.iter()
                    .map(|task| self.stage_cells(task, &section.steps, cx)),
            );
        let right = v_flex()
            .flex_none()
            .w(px(NEXT_WIDTH))
            .child(fixed_head("Next", true, head_border))
            .children(rows.iter().map(|task| self.next_cell(task, now, cx)));
        v_flex()
            .flex_none()
            .overflow_hidden()
            .rounded(px(12.))
            .border_1()
            .border_color(border)
            .bg(surface)
            .child(section_title(section))
            .child(
                h_flex()
                    .items_start()
                    .child(left)
                    .child(
                        div()
                            .id(SharedString::from(format!(
                                "progress-scroll-{}",
                                section.key
                            )))
                            .flex_1()
                            .min_w_0()
                            .overflow_x_scroll()
                            .child(middle),
                    )
                    .child(right),
            )
            .when(!section.done.is_empty(), |body| {
                body.child(self.disclosure(section, cx))
            })
            .into_any_element()
    }
}

/// The project's name and "N open, M done".
fn section_title(section: &Section) -> impl IntoElement {
    h_flex()
        .gap_2()
        .px_4()
        .pt_3()
        .pb_1()
        .items_baseline()
        .child(Label::new(section.title.clone()).weight(FontWeight::SEMIBOLD))
        .child(
            Label::new(format!(
                "{} open, {} done",
                section.open.len(),
                section.done.len()
            ))
            .size(LabelSize::XSmall)
            .color(Color::Muted),
        )
}

/// The head cell of the Task or the Next column, which stay put while the stages scroll.
fn fixed_head(label: &'static str, end: bool, border: Hsla) -> impl IntoElement {
    h_flex()
        .h(px(HEAD_HEIGHT))
        .px_4()
        .when(end, |head| head.justify_end())
        .border_b_1()
        .border_color(border)
        .child(head_label(label))
}

impl Render for ProgressBoard {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = now_ms();
        let repos: Vec<RepoRef> = self
            .dashboard
            .read(cx)
            .data
            .repos
            .iter()
            .map(|repo| RepoRef {
                id: repo.id.clone(),
                name: repo.name.clone(),
            })
            .collect();
        let settings = self.settings_state(&repos, cx);
        let sections = board_sections(&self.rows, &repos, &settings.pipelines, now);
        let body = v_flex()
            .id("progress-board")
            .track_focus(&self.focus)
            .size_full()
            .min_h_0()
            .overflow_y_scroll()
            .gap_4()
            .p_4();
        if settings.loading && !sections.is_empty() {
            return body.child(SkeletonList::new(4, 48.)).into_any_element();
        }
        if sections.is_empty() {
            return body
                .child(Label::new("No tasks to show.").color(Color::Muted))
                .into_any_element();
        }
        body.children(settings.failed.map(|message| self.alert(&message, cx)))
            .children(
                sections
                    .iter()
                    .map(|section| self.section(section, now, cx))
                    .collect::<Vec<_>>(),
            )
            .into_any_element()
    }
}
