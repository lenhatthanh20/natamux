//! The cells of a task table row (`TableRow` in `TaskTable.tsx`): what each column shows for a
//! task, with the web's tooltips. The board's cards use the same row data.

use gpui::FontWeight;
use natamux_proto::agents::activity::{TaskUsage, latest_line};
use natamux_proto::tasks::task::{Task, TaskState};
use natamux_proto::workspaces::workspace::DiffStat;
use ui::{Chip, Tooltip, prelude::*};

use super::table_columns::ColumnKey;
use super::table_rules::{
    ChecksTone, diff_size, format_tokens, format_usd, pull_request_cell, pull_request_number,
    waits_for,
};
use super::task_attention::is_quiet;
use super::task_rules::{lane_summary, stage_state_label};
use super::task_time::{AgeStyle, format_age, format_exact};
use crate::app::dashboard_data::DashboardData;
use crate::components::agent_badge::agent_name;
use crate::components::chips::{StateChip, label_chip};
use crate::components::colors::signal;
use crate::components::icons::icon_path;
use crate::sidebar::workspace_groups::short_branch;

/// What a row shows besides the task, read once per frame.
pub struct RowData {
    pub task: Task,
    pub quiet: bool,
    pub stat: Option<DiffStat>,
    pub usage: Option<TaskUsage>,
    /// What the agent does now, while it runs.
    pub note: Option<String>,
    /// "T-0003, T-0004": what the task still waits for. Empty: nothing.
    pub waits: String,
    pub show_repo: bool,
    pub now: i64,
}

impl RowData {
    pub fn of(task: &Task, data: &DashboardData, now: i64) -> Self {
        let note = (task.state == TaskState::Running)
            .then(|| data.activity.get(&task.id))
            .flatten()
            .and_then(|lines| latest_line(lines))
            .map(str::to_owned);
        RowData {
            task: task.clone(),
            quiet: is_quiet(task, now),
            stat: data.diff_stats.get(&task.id).cloned(),
            usage: data.usage.get(&task.id).cloned(),
            note,
            waits: waits_for(task, &data.tasks).join(", "),
            show_repo: data.repos.len() > 1,
            now,
        }
    }
}

/// Changes, Cost and Last activity line up on the right.
pub fn header_align_end(column: ColumnKey) -> bool {
    matches!(
        column,
        ColumnKey::Changes | ColumnKey::Cost | ColumnKey::Activity
    )
}

fn muted(text: impl Into<SharedString>) -> Label {
    Label::new(text).size(LabelSize::Small).color(Color::Muted)
}

fn dash() -> AnyElement {
    Label::new("-").color(Color::Muted).into_any_element()
}

/// A text with a tooltip; the id is unique inside its row.
fn tipped(id: &'static str, tip: String, child: impl IntoElement) -> AnyElement {
    div()
        .id(id)
        .min_w_0()
        .overflow_hidden()
        .child(child)
        .tooltip(Tooltip::text(tip))
        .into_any_element()
}

/// The "quiet" chip of an agent that went silent, in the signal colours.
pub fn quiet_chip(cx: &App) -> Chip {
    let signal = signal(cx);
    label_chip("quiet")
        .label_color(Color::Custom(signal.fg))
        .bg_color(signal.bg)
        .border_color(signal.border)
}

/// "+12 −3" with its file count in the tooltip, or nothing.
pub fn diff_label(stat: Option<&DiffStat>) -> Option<AnyElement> {
    let (added, removed, files) = diff_size(stat)?;
    // Never shrinks, as the web's `whitespace-nowrap`: a wide diff runs into the cell's padding.
    let text = h_flex()
        .flex_none()
        .gap_1()
        .child(
            Label::new(added)
                .size(LabelSize::Small)
                .color(Color::Created),
        )
        .child(
            Label::new(removed)
                .size(LabelSize::Small)
                .color(Color::Deleted),
        );
    // Not `tipped`: its wrapper shrinks and clips, which cut "+3120 −410" to "−41".
    Some(
        div()
            .id("diff")
            .flex_none()
            .child(text)
            .tooltip(Tooltip::text(files))
            .into_any_element(),
    )
}

fn task_cell(row: &RowData, cx: &App) -> AnyElement {
    let task = &row.task;
    h_flex()
        .min_w_0()
        .gap_2()
        .child(
            div().min_w_0().child(
                Label::new(task.title.clone())
                    .weight(FontWeight::MEDIUM)
                    .truncate(),
            ),
        )
        .when(row.quiet, |cell| cell.child(quiet_chip(cx)))
        .when(!row.waits.is_empty(), |cell| {
            cell.child(label_chip(format!("waits for {}", row.waits)))
        })
        .when_some(task.parent_id.clone(), |cell, parent| {
            cell.child(label_chip(format!("part of {parent}")))
        })
        .when_some(task.repo.clone().filter(|_| row.show_repo), |cell, repo| {
            cell.child(label_chip(repo))
        })
        .into_any_element()
}

fn pr_cell(row: &RowData) -> AnyElement {
    let Some(url) = row.task.pr.clone() else {
        return dash();
    };
    let link = h_flex()
        .id("pr-link")
        .flex_none()
        .gap_1()
        .cursor_pointer()
        .child(Label::new(pull_request_number(&url)).color(Color::Accent))
        .child(Icon::from_path(icon_path("external-link")).size(IconSize::XSmall))
        // The link opens in the browser and does not pick the row.
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            cx.open_url(&url);
        });
    let checks = pull_request_cell(&row.task).map(|(text, tone)| {
        let color = match tone {
            ChecksTone::Passing => Color::Success,
            ChecksTone::Failing => Color::Error,
            ChecksTone::Pending => Color::Warning,
            ChecksTone::None => Color::Muted,
        };
        Label::new(text)
            .size(LabelSize::Small)
            .color(color)
            .truncate()
    });
    h_flex()
        .min_w_0()
        .gap_1p5()
        .child(link)
        .children(checks)
        .into_any_element()
}

/// What one column shows for a row.
pub fn cell(column: ColumnKey, row: &RowData, cx: &App) -> AnyElement {
    let task = &row.task;
    match column {
        ColumnKey::Id => muted(task.id.clone()).buffer_font(cx).into_any_element(),
        ColumnKey::Task => task_cell(row, cx),
        ColumnKey::State => {
            let tip = match lane_summary(task) {
                Some(lanes) => format!("{} \u{b7} {lanes}", stage_state_label(task)),
                None => stage_state_label(task),
            };
            tipped("state", tip, StateChip::new(task.state))
        }
        ColumnKey::Agent => match (&row.note, task.agent.as_deref()) {
            (Some(note), _) => tipped("note", note.clone(), muted(note.clone()).truncate()),
            (None, None) => dash(),
            (None, Some(agent)) => agent_name(agent, LabelSize::Small).into_any_element(),
        },
        ColumnKey::Branch => match &task.branch {
            None => dash(),
            Some(branch) => tipped(
                "branch",
                branch.clone(),
                muted(short_branch(task)).buffer_font(cx).truncate(),
            ),
        },
        ColumnKey::Changes => diff_label(row.stat.as_ref()).unwrap_or_else(dash),
        ColumnKey::Pr => pr_cell(row),
        ColumnKey::Cost => match &row.usage {
            None => dash(),
            Some(usage) => {
                let tip = format!(
                    "{} tokens, estimated at API list prices",
                    format_tokens(usage.tokens)
                );
                let color = if usage.cost_usd.is_none() {
                    Color::Muted
                } else {
                    Color::Default
                };
                tipped(
                    "cost",
                    tip,
                    Label::new(format_usd(usage.cost_usd)).color(color),
                )
            }
        },
        ColumnKey::Activity => {
            let tip = format!(
                "Updated {}. Created {}.",
                format_exact(&task.updated_at, 0),
                format_exact(&task.created_at, 0)
            );
            let age = format_age(&task.updated_at, row.now, AgeStyle::Short, 0);
            tipped("activity", tip, muted(age))
        }
    }
}
