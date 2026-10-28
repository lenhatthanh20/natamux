//! The project groups of the sidebar tree (docs/ui-spec.md 4.9, `RepoHeader.tsx`): a flat tree,
//! each group's header with colour, count and fold (its buttons and menu are in
//! `project_buttons.rs`), the "N finished" fold, the lines of an empty group and the pinned
//! header. Also the loop that draws every line of the tree in order.

use gpui::{AnyElement, ClickEvent, Entity, FontWeight, Hsla, MouseButton, MouseDownEvent};
use ui::{Tooltip, prelude::*};

use super::blur_bar::{BlurBar, private_text};
use super::project_buttons::hover_button;
use super::sidebar::{lucide, private, small, state_dot};
use super::workspace_tree::{Snapshot, WorkspaceTree};
use crate::app::dashboard::Dashboard;
use crate::app::dashboard_data::BulkRun;
use crate::components::button_style::quiet_button;
use crate::components::chips::{BadgeTone, CountBadge};
use crate::components::colors::project_color;
use crate::components::text_size::row_text;
use crate::sidebar::project_color::project_color as project_color_index;
use crate::sidebar::sidebar_text::{shown_name, workspace_count};
use crate::sidebar::tree_entries::EntryAction;
use crate::sidebar::tree_model::{GroupInfo, Line, TreeModel};
use crate::sidebar::workspace_groups::{RepoGroup, collapsed_hint};
use crate::workspaces::{new_workspace_dialog, remove_finished};

/// The 1 px guide in front of the rows under a task: 12 px of indent per level, as Zed's
/// `ListItem`.
fn under_guide(element: AnyElement, cx: &App) -> AnyElement {
    // Padding, not margin: a list lays each line out as a root, and a root's margin only
    // narrows it.
    div()
        .w_full()
        .pl(rems_from_px(11.0_f32))
        .child(
            div()
                .w_full()
                .border_l_1()
                .border_color(cx.theme().colors().border_variant)
                .child(element),
        )
        .into_any_element()
}

/// A project header line: the fold takes the free width, so the buttons sit at the right end.
fn header_row(
    group: SharedString,
    ring: Hsla,
    fold: AnyElement,
    buttons: Vec<AnyElement>,
    height: f32,
    cx: &App,
) -> AnyElement {
    h_flex()
        .group(group)
        .w_full()
        .h(rems_from_px(height))
        .pr(rems_from_px(2.0_f32))
        .rounded_sm()
        .hover(|style| style.bg(cx.theme().colors().ghost_element_hover))
        .border_1()
        .border_color(ring)
        .child(fold)
        .children(buttons)
        .into_any_element()
}

/// The focus ring of a header row while keyboard focus is on `key`.
fn focus_ring(key: &str, snap: &Snapshot, cx: &App) -> Hsla {
    if snap.focused && snap.stop.as_deref() == Some(key) {
        cx.theme().colors().border_focused
    } else {
        gpui::transparent_black()
    }
}

impl WorkspaceTree {
    /// Line `index` of the tree, drawn when the list shows it.
    pub(super) fn render_line(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let (Some(model), Some(snap)) = (self.model.as_ref(), self.shown.as_ref()) else {
            return div().into_any_element();
        };
        let Some(line) = model.lines.get(index) else {
            return div().into_any_element();
        };
        match line {
            Line::PinnedHeader { count } => pinned_header(*count),
            Line::GroupHeader { group } => self.group_header(*group, model, snap, cx),
            Line::Task { task, nested, .. } => {
                let row = self.task_row(task, model, snap, cx);
                if *nested { under_guide(row, cx) } else { row }
            }
            Line::Terminal { task_id, row, .. } => {
                let row = self.terminal_row(task_id, row, snap, cx);
                under_guide(row, cx)
            }
            Line::NoWorkspaces { group } => no_workspaces(*group, model, &self.dashboard),
            Line::NoMatches { group } => no_matches(*group, model, snap),
            Line::Finished { group } => self.finished_row(*group, model, snap, cx),
        }
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
        let ring = focus_ring(&key, snap, cx);
        let fold = self.header_fold(repo_group, info, snap, cx);
        let buttons = self.header_buttons(repo, info, snap, cx);
        header_row(
            SharedString::from(format!("header-{}", repo.id)),
            ring,
            fold,
            buttons,
            26.0,
            cx,
        )
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
            .child(private_text(
                blurred,
                || BlurBar::new(repo.name.clone(), 13.0).weight(FontWeight::SEMIBOLD),
                || {
                    Label::new(repo.name.clone())
                        .size(row_text())
                        .weight(FontWeight::SEMIBOLD)
                        .truncate()
                        .into_any_element()
                },
            ))
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
        let ring = focus_ring(&key, snap, cx);
        let fold = h_flex()
            .id(SharedString::from(key.clone()))
            .flex_1()
            .min_w_0()
            .h_full()
            .pl(rems_from_px(22.0_f32))
            .gap_1()
            .cursor_pointer()
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
            }))
            .into_any_element();
        let button = hover_button(
            SharedString::from(format!("remove-finished-{}", repo.id)),
            "folder-minus",
            format!("Remove finished workspaces in {name}"),
            group_name.clone(),
            move |_, window, cx| {
                remove_finished::open(&dashboard, remove_repo.clone(), window, cx);
            },
        )
        .disabled(removing)
        .into_any_element();
        // The header's frame, so the hover and the focus ring cover the button too.
        header_row(group_name, ring, fold, vec![button], 24.0, cx)
    }
}

/// "Pinned" and its count, over the pinned rows.
fn pinned_header(count: usize) -> AnyElement {
    h_flex()
        .h(rems_from_px(26.0_f32))
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
        quiet_button(
            SharedString::from(format!("empty-start-{repo}")),
            "Start a task",
        )
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

#[cfg(test)]
#[path = "project_header_tests.rs"]
mod tests;
