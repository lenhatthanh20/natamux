//! The History tab (`GitGraph.tsx`, docs/ui-spec.md 4.19): what the task's branch has that its
//! base has not, one 28 px row per commit with the lanes drawn at the left. The panel shows it
//! narrow; its expand button shows it over main at full width (T-0231).

use std::sync::Arc;

use gpui::{ClickEvent, FontWeight, ListSizingBehavior, PathBuilder, canvas, point, uniform_list};
use natamux_proto::workspaces::workspace::{GitGraph, GraphCommit};
use ui::{Tooltip, prelude::*};

use super::graph_lanes::{MIDDLE, ROW_HEIGHT, dot_radius, is_head, lane_x, lanes_width, links};
use super::graph_layout::{
    GraphRow, ahead_label, hidden_refs_tip, layout_graph, shown_refs, truncated_label,
    uncommitted_label,
};
use crate::app::clock::now_ms;
use crate::components::button_style::quiet_button;
use crate::components::colors::lane_color;
use crate::components::icons::icon_path;
use crate::components::skeleton::{Loading, SkeletonList};
use crate::components::terminals_button::terminals_button;
use crate::components::text_size::row_text;
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
    /// Over main at full width; else the narrow panel's copy, with no author or hash column.
    pub in_main: bool,
    /// Main shows History: the panel's expand button is pressed.
    pub main_shows: bool,
    pub on_show_changes: OnClick,
    pub on_refresh: OnClick,
    /// The panel's expand button and main's Terminals button: main shows History, or the
    /// terminals again.
    pub on_toggle_main: OnClick,
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

/// The panel's expand button, pressed while main shows History; main's copy has Terminals.
fn toggle_main(in_main: bool, main_shows: bool, on_toggle: OnClick) -> AnyElement {
    let click = move |_: &ClickEvent, window: &mut Window, cx: &mut App| on_toggle(window, cx);
    if in_main {
        return terminals_button("history-terminals")
            .on_click(click)
            .into_any_element();
    }
    let (icon, tip) = if main_shows {
        (IconName::Minimize, "Show terminals")
    } else {
        (IconName::Maximize, "Show in main")
    };
    IconButton::new("history-expand", icon)
        .icon_size(IconSize::Small)
        .toggle_state(main_shows)
        .tooltip(Tooltip::text(tip))
        .on_click(click)
        .into_any_element()
}

fn summary(
    graph: &GitGraph,
    uncommitted: usize,
    (on_show, on_refresh): (OnClick, OnClick),
    toggle: AnyElement,
) -> Div {
    let link = (uncommitted > 0).then(|| {
        quiet_button("history-uncommitted", uncommitted_label(uncommitted))
            .label_size(LabelSize::Small)
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
        .child(toggle)
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
    // Shrinks with the row: in the narrow details panel (T-0183) a long name would cover the
    // subject.
    div()
        .id(id)
        .min_w_0()
        .flex_shrink(1.)
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

/// The short hash, the full one in its tooltip.
fn hash_cell(index: usize, commit: &GraphCommit, cx: &App) -> impl IntoElement {
    let hash: String = commit.hash.chars().take(7).collect();
    div()
        .id(element_id("history-hash", index))
        .flex_none()
        .tooltip(Tooltip::text(commit.hash.clone()))
        .child(
            Label::new(hash)
                .size(LabelSize::Small)
                .color(Color::Muted)
                .buffer_font(cx),
        )
}

fn commit_row(
    index: usize,
    commit: &GraphCommit,
    lanes: &GraphRow,
    graph: &GitGraph,
    (width, narrow, now): (f32, bool, i64),
    cx: &App,
) -> AnyElement {
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
        .size(row_text())
        .color(color)
        .weight(if head {
            FontWeight::SEMIBOLD
        } else {
            FontWeight::NORMAL
        })
        .truncate();
    let age = format_age(&commit.date, now, AgeStyle::Short, OFFSET_MINUTES);
    // The subject cell has the full text: a cut subject is read on hover.
    let subject = div()
        .id(element_id("history-subject", index))
        .debug_selector(|| format!("history-subject-{index}"))
        .min_w_0()
        .when(!narrow, |cell| {
            cell.tooltip(Tooltip::text(commit.subject.clone()))
        })
        .child(subject);
    h_flex()
        .id(element_id("history-row", index))
        .when(narrow, |row| {
            row.tooltip(Tooltip::text(format!("{}\n\n{}", commit.subject, commit.hash)))
        })
        .flex_none()
        // uniform_list lays a row out on its own: with no width it takes its content's width, so
        // nothing truncates and the list cuts the end off (T-0231).
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
                .child(subject),
        )
        .when(!narrow, |this| {
            let author = Label::new(commit.author.clone())
                .size(LabelSize::Small)
                .color(Color::Muted);
            this.child(div().flex_none().max_w(px(160.)).child(author.truncate()))
        })
        // The narrow panel keeps the subject: the hash is in the row's tooltip there.
        .when(!narrow, |this| this.child(hash_cell(index, commit, cx)))
        .child(
            h_flex()
                .id(element_id("history-age", index))
                .debug_selector(|| format!("history-age-{index}"))
                .flex_none()
                .w(px(if narrow { 40. } else { 80. }))
                .justify_end()
                .tooltip(Tooltip::text(format_exact(&commit.date, OFFSET_MINUTES)))
                .child(Label::new(age).color(Color::Muted).single_line()),
        )
        .into_any_element()
}

/// The graph the rows were laid out from, and its layout. Kept by the list between frames: the
/// graph is read every few seconds, the frames come much more often.
#[derive(Clone)]
struct Layout {
    graph: Arc<GitGraph>,
    rows: Arc<Vec<GraphRow>>,
    width: usize,
}

fn layout_of(id: &str, graph: &GitGraph, window: &mut Window, cx: &mut App) -> Layout {
    let key = SharedString::from(format!("{id}-layout"));
    let state = window.use_keyed_state(key, cx, |_, _| None::<Layout>);
    if let Some(kept) = state.read(cx).clone().filter(|kept| *kept.graph == *graph) {
        return kept;
    }
    let (rows, width) = layout_graph(&graph.commits);
    let layout = Layout {
        graph: Arc::new(graph.clone()),
        rows: Arc::new(rows),
        width,
    };
    state.update(cx, |state, _| *state = Some(layout.clone()));
    layout
}

fn commit_list(
    id: &str,
    graph: &GitGraph,
    narrow: bool,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let layout = layout_of(id, graph, window, cx);
    let lanes = lanes_width(layout.width);
    let count = graph.commits.len();
    let rows = uniform_list(
        SharedString::from(format!("{id}-commits")),
        count,
        move |range, _, cx| {
            let facts = (lanes, narrow, now_ms());
            let graph = &layout.graph;
            range
                .map(|index| {
                    let (commit, lanes) = (&graph.commits[index], &layout.rows[index]);
                    commit_row(index, commit, lanes, graph, facts, cx)
                })
                .collect::<Vec<_>>()
        },
    )
    .with_sizing_behavior(ListSizingBehavior::Infer)
    .min_h_0();
    v_flex()
        .flex_shrink_1()
        .min_h_0()
        .py_1()
        .rounded_md()
        .border_1()
        .border_color(cx.theme().colors().border)
        .bg(cx.theme().colors().surface_background)
        .child(rows)
}

impl RenderOnce for HistoryView {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
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
        let toggle = toggle_main(self.in_main, self.main_shows, self.on_toggle_main);
        frame
            .child(summary(
                &graph,
                self.uncommitted,
                (self.on_show_changes, self.on_refresh),
                toggle,
            ))
            .child(commit_list(&self.id, &graph, !self.in_main, window, cx))
            .into_any_element()
    }
}

#[cfg(test)]
#[path = "history_view_tests.rs"]
mod tests;
