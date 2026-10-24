//! The project groups of the sidebar tree (docs/ui-spec.md 4.9, `RepoHeader.tsx`): the card
//! around a group, its header with colour, count and fold (its buttons and menu are in
//! `project_buttons.rs`), the "N finished" fold, the lines of an empty group and the pinned
//! header. Also the loop that draws every line of the tree in order.

use gpui::{
    AnyElement, ClickEvent, Entity, FontWeight, MouseButton, MouseDownEvent, linear_color_stop,
    linear_gradient,
};
use ui::{Tooltip, prelude::*};

use super::project_buttons::hover_button;
use super::sidebar::{lucide, private, small, state_dot};
use super::workspace_tree::{Snapshot, WorkspaceTree};
use crate::app::dashboard::Dashboard;
use crate::app::dashboard_data::BulkRun;
use crate::components::chips::{BadgeTone, CountBadge};
use crate::components::colors::project_color;
use crate::sidebar::project_color::project_color as project_color_index;
use crate::sidebar::sidebar_text::{shown_name, workspace_count};
use crate::sidebar::tree_entries::EntryAction;
use crate::sidebar::tree_model::{GroupInfo, Line, TreeModel};
use crate::sidebar::workspace_groups::{RepoGroup, collapsed_hint};
use crate::workspaces::{new_workspace_dialog, remove_finished};

/// Where a line sits in its project card.
struct CardPart {
    group: Option<usize>,
    first: bool,
    last: bool,
    header: bool,
    /// The first line of the tree: the card keeps the small gap under the tree's header.
    top: bool,
}

fn in_card(element: AnyElement, part: CardPart, cx: &App) -> AnyElement {
    if part.group.is_none() {
        return element;
    }
    let colors = cx.theme().colors();
    div()
        .bg(colors.surface_background.opacity(0.55))
        .border_color(colors.border_variant)
        .border_l_1()
        .border_r_1()
        .when(!part.header, |card| card.px(rems_from_px(2.0_f32)))
        .when(part.first, |card| {
            card.border_t_1()
                .rounded_t(px(10.))
                .mt(px(if part.top { 4. } else { 8. }))
        })
        .when(part.last, |card| card.border_b_1().rounded_b(px(10.)))
        .when(part.last && !part.header, |card| {
            card.pb(rems_from_px(4.0_f32))
        })
        .child(element)
        .into_any_element()
}

/// The 1 px guide under the task glyph, in front of the rows under a task.
fn under_guide(element: AnyElement, cx: &App) -> AnyElement {
    div()
        .ml(rems_from_px(16.0_f32))
        .pl(rems_from_px(7.0_f32))
        .border_l_1()
        .border_color(cx.theme().colors().border_variant)
        .child(element)
        .into_any_element()
}

impl WorkspaceTree {
    pub(super) fn render_lines(&self, snap: &Snapshot, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(model) = self.model.as_ref() else {
            return Vec::new();
        };
        let lines = &model.lines;
        let mut out = Vec::with_capacity(lines.len());
        for (index, line) in lines.iter().enumerate() {
            let group = line.group();
            let part = CardPart {
                group,
                first: index == 0 || lines[index - 1].group() != group,
                last: lines.get(index + 1).map(Line::group) != Some(group),
                header: matches!(line, Line::GroupHeader { .. }),
                top: index == 0,
            };
            let element = match line {
                Line::PinnedHeader { count } => pinned_header(*count),
                Line::GroupHeader { group } => self.group_header(*group, model, snap, cx),
                Line::Task {
                    task,
                    nested,
                    pinned_row,
                    ..
                } => {
                    let row = self.task_row(task, *pinned_row, model, snap, cx);
                    if *nested { under_guide(row, cx) } else { row }
                }
                Line::Terminal { task_id, row, .. } => {
                    let row = self.terminal_row(task_id, row, snap, cx);
                    under_guide(row, cx)
                }
                Line::NoWorkspaces { group } => no_workspaces(*group, model, &self.dashboard),
                Line::NoMatches { group } => no_matches(*group, model, snap),
                Line::Finished { group } => self.finished_row(*group, model, snap, cx),
            };
            out.push(in_card(element, part, cx));
        }
        out
    }

    fn set_fold(&mut self, key: String, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_key(key.clone(), window, cx);
        self.run_entry(EntryAction::SetFold { key, open }, cx);
    }

    fn group_header(
        &self,
        group: usize,
        model: &TreeModel,
        snap: &Snapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (Some(repo_group), Some(info)) =
            (model.groups.groups.get(group), model.infos.get(group))
        else {
            return div().into_any_element();
        };
        let repo = &repo_group.repo;
        let key = format!("repo:{}", repo.id);
        let color = project_color(project_color_index(&repo.id), cx);
        let ring = if snap.focused && snap.stop.as_deref() == Some(key.as_str()) {
            cx.theme().colors().border_focused
        } else {
            gpui::transparent_black()
        };
        let collapsed = !info.open;
        let band = |from: f32, to: f32| {
            linear_gradient(
                90.,
                linear_color_stop(color.opacity(from), 0.),
                linear_color_stop(color.opacity(to), 0.85),
            )
        };
        let fold = self.header_fold(repo_group, info, snap, cx);
        let buttons = self.header_buttons(repo, info, snap, cx);
        h_flex()
            .group(SharedString::from(format!("header-{}", repo.id)))
            .h(rems_from_px(28.0_f32))
            .pr(rems_from_px(2.0_f32))
            .bg(band(0.16, 0.))
            .hover(|style| style.bg(band(0.2, 0.04)))
            .when(collapsed, |header| header.rounded_md())
            .when(!collapsed, |header| header.rounded_t_md())
            .border_2()
            .border_color(ring)
            .child(fold)
            .children(buttons)
            .into_any_element()
    }

    /// Chevron, project icon, name, count and the dot of a folded group. A click folds; a right
    /// click opens the project menu.
    fn header_fold(
        &self,
        repo_group: &RepoGroup,
        info: &GroupInfo,
        snap: &Snapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let repo = &repo_group.repo;
        let blurred = snap.blurred.contains(&repo.id);
        let name = shown_name(&repo.name, blurred);
        let index = project_color_index(&repo.id);
        let collapsed = !info.open;
        let hint = collapsed
            .then(|| collapsed_hint(repo_group, snap.current.as_deref(), snap.now_ms))
            .flatten();
        let key = format!("repo:{}", repo.id);
        let menu_repo = repo.id.clone();
        let chevron = if collapsed {
            IconName::ChevronRight
        } else {
            IconName::ChevronDown
        };
        h_flex()
            .id(SharedString::from(key.clone()))
            .flex_1()
            .min_w_0()
            .h_full()
            .gap(rems_from_px(6.0_f32))
            .px(rems_from_px(6.0_f32))
            .cursor_pointer()
            .child(
                Icon::new(chevron)
                    .size(IconSize::XSmall)
                    .color(Color::Muted),
            )
            .child(
                lucide("folder-git-2")
                    .size(IconSize::Small)
                    .color(Color::Custom(project_color(index, cx))),
            )
            .child(
                Label::new(private(&repo.name, blurred))
                    .size(LabelSize::Custom(rems_from_px(13_f32)))
                    .weight(FontWeight::SEMIBOLD)
                    .truncate(),
            )
            .child(
                div()
                    .id(SharedString::from(format!("count-{}", repo.id)))
                    .child(CountBadge::new(info.count).tone(BadgeTone::Project(index)))
                    .tooltip(Tooltip::text(workspace_count(info.count))),
            )
            .children(hint.map(|state| {
                div()
                    .id(SharedString::from(format!("hidden-{}", repo.id)))
                    .child(state_dot(state, cx))
                    .tooltip(Tooltip::text(format!(
                        "A workspace in {name} is hidden here"
                    )))
            }))
            .on_click(cx.listener(move |tree, _: &ClickEvent, window, cx| {
                tree.set_fold(key.clone(), collapsed, window, cx);
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |tree, event: &MouseDownEvent, window, cx| {
                    tree.open_project_menu(&menu_repo, event.position, window, cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn finished_row(
        &self,
        group: usize,
        model: &TreeModel,
        snap: &Snapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (Some(repo_group), Some(info)) =
            (model.groups.groups.get(group), model.infos.get(group))
        else {
            return div().into_any_element();
        };
        let repo = &repo_group.repo;
        let key = format!("finished:{}", repo.id);
        let shown = info.finished_shown;
        let name = shown_name(&repo.name, snap.blurred.contains(&repo.id));
        let group_name = SharedString::from(key.clone());
        let toggle_key = key.clone();
        let remove_repo = repo.id.clone();
        let removing = self
            .dashboard
            .read(cx)
            .bulk
            .running(BulkRun::RemoveFinished);
        let dashboard = self.dashboard.clone();
        h_flex()
            .group(group_name.clone())
            .w_full()
            .pr(rems_from_px(2.0_f32))
            .child(
                self.row_frame(&key, false, snap, cx)
                    .id(SharedString::from(key.clone()))
                    .h(rems_from_px(24.0_f32))
                    .pl(rems_from_px(22.0_f32))
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        Icon::new(if shown {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        })
                        .size(IconSize::XSmall)
                        .color(Color::Muted),
                    )
                    .child(small(format!("{} finished", repo_group.finished.len())))
                    .on_click(cx.listener(move |tree, _: &ClickEvent, window, cx| {
                        tree.set_fold(toggle_key.clone(), !shown, window, cx);
                    })),
            )
            .child(
                hover_button(
                    SharedString::from(format!("remove-finished-{}", repo.id)),
                    "folder-minus",
                    format!("Remove finished workspaces in {name}"),
                    group_name,
                    move |_, window, cx| {
                        remove_finished::open(&dashboard, remove_repo.clone(), window, cx);
                    },
                )
                .disabled(removing),
            )
            .into_any_element()
    }
}

/// "Pinned" and its count, over the pinned rows.
fn pinned_header(count: usize) -> AnyElement {
    h_flex()
        .h(rems_from_px(28.0_f32))
        .px_2()
        .gap(rems_from_px(6.0_f32))
        .child(lucide("pin").color(Color::Muted))
        .child(
            Label::new("PINNED")
                .size(LabelSize::Custom(rems_from_px(11_f32)))
                .weight(FontWeight::SEMIBOLD)
                .color(Color::Muted),
        )
        .child(CountBadge::new(count))
        .into_any_element()
}

/// "No workspaces. Start a task": an empty group of a project the daemon still has.
fn no_workspaces(group: usize, model: &TreeModel, dashboard: &Entity<Dashboard>) -> AnyElement {
    let (Some(repo_group), Some(info)) = (model.groups.groups.get(group), model.infos.get(group))
    else {
        return div().into_any_element();
    };
    let repo = repo_group.repo.id.clone();
    let dashboard = dashboard.clone();
    let start = (!info.gone).then(|| {
        Button::new(
            SharedString::from(format!("empty-start-{repo}")),
            "Start a task",
        )
        .style(ButtonStyle::Transparent)
        .label_size(LabelSize::Small)
        .on_click(move |_, window, cx| {
            new_workspace_dialog::open(&dashboard, Some(repo.clone()), window, cx);
        })
    });
    h_flex()
        .py_1()
        .pl(rems_from_px(32.0_f32))
        .gap_1()
        .child(small("No workspaces."))
        .children(start)
        .into_any_element()
}

/// "No matches in <project> (N hidden)": the filter hides every workspace of the group.
fn no_matches(group: usize, model: &TreeModel, snap: &Snapshot) -> AnyElement {
    let (Some(repo_group), Some(info)) = (model.groups.groups.get(group), model.infos.get(group))
    else {
        return div().into_any_element();
    };
    let repo = &repo_group.repo;
    let name = private(&repo.name, snap.blurred.contains(&repo.id));
    div()
        .py_1()
        .pl(rems_from_px(32.0_f32))
        .child(small(format!(
            "No matches in {name} ({} hidden)",
            info.hidden
        )))
        .into_any_element()
}
