//! The History tab (`GitGraph.tsx`, docs/ui-spec.md 4.19): what the task's branch has that its
//! base has not, one 28 px row per commit with the lanes drawn at the left.

use gpui::{ClickEvent, FontWeight, PathBuilder, canvas, point};
use natamux_proto::workspaces::workspace::{GitGraph, GraphCommit};
use ui::{Tooltip, prelude::*};

use super::graph_lanes::{MIDDLE, ROW_HEIGHT, dot_radius, is_head, lane_x, lanes_width, links};
use super::graph_layout::{
    GraphRow, ahead_label, hidden_refs_tip, layout_graph, shown_refs, truncated_label,
    uncommitted_label,
};
use crate::app::clock::now_ms;
use crate::components::colors::lane_color;
use crate::components::icons::icon_path;
use crate::components::skeleton::{Loading, SkeletonList};
use crate::tasks::task_time::{AgeStyle, format_age, format_exact};
use crate::workspaces::finished_banner::OnClick;

/// The sidebar and the lists show times in UTC until the app reads the local zone.
const OFFSET_MINUTES: i32 = 0;

#[derive(IntoElement)]
pub struct HistoryView {
    pub id: String,
    /// None while read.
    pub graph: Option<GitGraph>,
    pub error: Option<String>,
    /// Files that are not committed, which no commit below shows yet.
    pub uncommitted: usize,
    /// Hides the author column.
    pub narrow: bool,
    pub on_show_changes: OnClick,
    pub on_refresh: OnClick,
}

fn element_id(prefix: &str, index: usize) -> SharedString {
    SharedString::from(format!("{prefix}-{index}"))
}

fn alert(message: String, cx: &App) -> impl IntoElement {
    let status = cx.theme().status();
    h_flex()
        .gap_2()
        .px_3()
        .py_2()
        .rounded_md()
        .border_1()
        .border_color(status.error_border)
        .bg(status.error_background)
        .child(
            Icon::from_path(icon_path("circle-alert"))
                .size(IconSize::Small)
                .color(Color::Error),
        )
        .child(Label::new(message).color(Color::Error))
}

fn summary(graph: &GitGraph, uncommitted: usize, on_show: OnClick, on_refresh: OnClick) -> Div {
    let link = (uncommitted > 0).then(|| {
        Button::new("history-uncommitted", uncommitted_label(uncommitted))
            .style(ButtonStyle::Transparent)
            .color(Color::Accent)
            .on_click(move |_: &ClickEvent, window, cx| on_show(window, cx))
    });
    let text = h_flex()
        .min_w_0()
        .flex_1()
        .flex_wrap()
        .gap_x_1()
        .child(Label::new(ahead_label(graph)).color(Color::Muted))
        .children(link)
        .when(graph.truncated, |this| {
            let text = truncated_label(graph.commits.len());
            this.child(Label::new(text.trim_start().to_owned()).color(Color::Muted))
        });
    h_flex()
        .flex_none()
        .items_center()
        .gap_2()
        .child(text)
        .child(
            IconButton::new("history-refresh", IconName::RotateCw)
                .icon_size(IconSize::Small)
                .tooltip(Tooltip::text("Refresh (Shift+R)"))
                .on_click(move |_: &ClickEvent, window, cx| on_refresh(window, cx)),
        )
}

/// The lines are drawn on a canvas, the dot is a round div over them.
fn lane_drawing(row: &GraphRow, width: f32, head: bool, on_branch: bool, cx: &App) -> Div {
    let lines: Vec<_> = links(row)
        .into_iter()
        .map(|line| (line, lane_color(line.color_lane, cx)))
        .collect();
    let lines = canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let at = |(x, y): (f32, f32)| point(bounds.origin.x + px(x), bounds.origin.y + px(y));
            for (line, color) in &lines {
                let mut path = PathBuilder::stroke(px(2.));
                path.move_to(at(line.from));
                match line.controls() {
                    Some([first, second]) => {
                        path.cubic_bezier_to(at(line.to), at(first), at(second))
                    }
                    None => path.line_to(at(line.to)),
                }
                if let Ok(path) = path.build() {
                    window.paint_path(path, *color);
                }
            }
        },
    )
    .absolute()
    .size_full();
    let color = lane_color(row.lane, cx);
    // The ring is 2 px wide, outside the radius, as the SVG stroke is.
    let outer = dot_radius(head) + 1.;
    let fill = if on_branch {
        color
    } else {
        cx.theme().colors().surface_background
    };
    let dot = div()
        .absolute()
        .left(px(lane_x(row.lane) - outer))
        .top(px(MIDDLE - outer))
        .w(px(outer * 2.))
        .h(px(outer * 2.))
        .rounded_full()
        .border_2()
        .border_color(color)
        .bg(fill);
    div()
        .relative()
        .flex_none()
        .ml_2()
        .w(px(width))
        .h(px(ROW_HEIGHT))
        .child(lines)
        .child(dot)
}

/// A branch or tag name: mono, bordered, the whole name in the tooltip.
fn ref_chip(
    id: SharedString,
    text: String,
    tip: String,
    muted: bool,
    cx: &App,
) -> impl IntoElement {
    div()
        .id(id)
        .flex_none()
        .max_w(px(160.))
        .px_1p5()
        .rounded_sm()
        .border_1()
        .border_color(cx.theme().colors().border)
        .tooltip(Tooltip::text(tip))
        .child(
            Label::new(text)
                .size(LabelSize::Small)
                .color(if muted { Color::Muted } else { Color::Default })
                .buffer_font(cx)
                .truncate(),
        )
}

fn commit_row(
    index: usize,
    commit: &GraphCommit,
    lanes: &GraphRow,
    graph: &GitGraph,
    (width, narrow, now): (f32, bool, i64),
    cx: &App,
) -> impl IntoElement {
    let head = is_head(graph, index, commit);
    let color = if commit.on_branch {
        Color::Default
    } else {
        Color::Muted
    };
    let (shown, more) = shown_refs(&commit.refs);
    let chips = shown
        .iter()
        .enumerate()
        .map(|(n, name)| {
            let id = element_id(&format!("history-ref-{index}"), n);
            ref_chip(id, name.clone(), name.clone(), false, cx)
        })
        .chain(more.map(|text| {
            let id = element_id("history-more", index);
            ref_chip(id, text, hidden_refs_tip(&commit.refs), true, cx)
        }));
    let subject = Label::new(commit.subject.clone())
        .color(color)
        .weight(if head {
            FontWeight::SEMIBOLD
        } else {
            FontWeight::NORMAL
        })
        .truncate();
    let hash: String = commit.hash.chars().take(7).collect();
    let age = format_age(&commit.date, now, AgeStyle::Short, OFFSET_MINUTES);
    h_flex()
        .id(element_id("history-row", index))
        .flex_none()
        .h(px(ROW_HEIGHT))
        .items_center()
        .gap_3()
        .pr_3()
        .child(lane_drawing(lanes, width, head, commit.on_branch, cx))
        .child(
            h_flex()
                .min_w_0()
                .flex_1()
                .gap_2()
                .children(chips)
                .child(div().min_w_0().child(subject)),
        )
        .when(!narrow, |this| {
            let author = Label::new(commit.author.clone()).color(Color::Muted);
            this.child(div().flex_none().max_w(px(160.)).child(author.truncate()))
        })
        .child(
            div()
                .id(element_id("history-hash", index))
                .flex_none()
                .tooltip(Tooltip::text(commit.hash.clone()))
                .child(
                    Label::new(hash)
                        .size(LabelSize::Small)
                        .color(Color::Muted)
                        .buffer_font(cx),
                ),
        )
        .child(
            h_flex()
                .id(element_id("history-age", index))
                .flex_none()
                .w(px(80.))
                .justify_end()
                .tooltip(Tooltip::text(format_exact(&commit.date, OFFSET_MINUTES)))
                .child(Label::new(age).color(Color::Muted).single_line()),
        )
}

fn commit_list(id: &str, graph: &GitGraph, narrow: bool, cx: &App) -> impl IntoElement {
    let (rows, width) = layout_graph(&graph.commits);
    let facts = (lanes_width(width), narrow, now_ms());
    v_flex()
        .id(SharedString::from(format!("{id}-commits")))
        .flex_shrink_1()
        .min_h_0()
        .overflow_y_scroll()
        .py_1()
        .rounded_md()
        .border_1()
        .border_color(cx.theme().colors().border)
        .bg(cx.theme().colors().surface_background)
        .children(
            graph
                .commits
                .iter()
                .zip(&rows)
                .enumerate()
                .map(|(index, (commit, lanes))| commit_row(index, commit, lanes, graph, facts, cx)),
        )
}

impl RenderOnce for HistoryView {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let frame = v_flex()
            .id(SharedString::from(self.id.clone()))
            .size_full()
            .p_4()
            .gap_3();
        if let Some(message) = self.error {
            return frame.child(alert(message, cx)).into_any_element();
        }
        let Some(graph) = self.graph else {
            let loading = Loading::new(
                SharedString::from(format!("{}-loading", self.id)),
                true,
                SkeletonList::new(8, ROW_HEIGHT),
            );
            return frame.child(loading).into_any_element();
        };
        frame
            .child(summary(
                &graph,
                self.uncommitted,
                self.on_show_changes,
                self.on_refresh,
            ))
            .child(commit_list(&self.id, &graph, self.narrow, cx))
            .into_any_element()
    }
}
