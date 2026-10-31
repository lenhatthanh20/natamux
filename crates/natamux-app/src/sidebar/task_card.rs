//! A task as a card in the sidebar (Option F): the closed card, the one-line finished row, and the
//! pieces the open card (`task_card_open.rs`) shares: the frame and the first line.

use gpui::{AnyElement, BoxShadow, ClickEvent, MouseButton, MouseDownEvent, Stateful, point};
use natamux_proto::tasks::task::Task;
use ui::{Tooltip, prelude::*};

use super::blur_bar::{BlurBar, private_text};
use super::sidebar::{mono, small};
use super::task_card_open::{is_blurred, open_title};
use super::workspace_row::{age_label, right_slot, title_label};
use super::workspace_tree::{Snapshot, WorkspaceTree};
use crate::agents::agent_choices::agent_label;
use crate::app::view_rules::is_unseen;
use crate::components::agent_badge::AgentIcon;
use crate::components::colors::state_colors;
use crate::components::glyph::StateGlyph;
use crate::components::shape::ShapeSize;
use crate::sidebar::card_rules::{
    AgentAvatar, MAX_AVATARS, card_agents, card_fold_key, card_stage, more_agents,
};
use crate::sidebar::shell_rows::task_glyph_state;
use crate::sidebar::sidebar_tree::{RowTier, row_tier};
use crate::sidebar::tree_entries::EntryAction;
use crate::sidebar::tree_model::{Line, TaskLook, TreeModel};
use crate::sidebar::workspace_groups::is_quiet;
use crate::tasks::task_rules::lane_summary;
use crate::tasks::task_table_cells::diff_label;

/// Which border a card has: the open or keyboard-focused one, one that needs the user, or none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CardBorder {
    Accent,
    Attention,
    Plain,
}

pub(super) fn card_border(current: bool, focused: bool, attention: bool) -> CardBorder {
    if current || focused {
        CardBorder::Accent
    } else if attention {
        CardBorder::Attention
    } else {
        CardBorder::Plain
    }
}

impl WorkspaceTree {
    /// A `Line::Task`, with the 8 px gap under a card. Padding, not margin: a list lays each line
    /// out as a root, and a root's margin only narrows it.
    pub(super) fn task_line(
        &self,
        line: &Line,
        model: &TreeModel,
        snap: &Snapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Line::Task {
            task,
            nested,
            pinned_row,
            look,
            ..
        } = line
        else {
            return div().into_any_element();
        };
        let card = match look {
            TaskLook::Closed => self.closed_card(task, model, snap, *pinned_row, *nested, cx),
            TaskLook::Open { rows, children } => {
                self.open_card(task, rows, children, model, snap, *pinned_row, cx)
            }
            TaskLook::Finished => return self.finished_task_row(task, model, snap, cx),
        };
        div()
            .w_full()
            .pb(rems_from_px(8.0_f32))
            .child(card)
            .into_any_element()
    }

    pub(super) fn closed_card(
        &self,
        task: &Task,
        model: &TreeModel,
        snap: &Snapshot,
        pinned_row: bool,
        nested: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let blurred = is_blurred(task, snap);
        let head = self.card_head(task, model, snap, false, nested, cx);
        let meta = self.closed_meta(task, model, snap, pinned_row, blurred, cx);
        let waiting = self.waiting_box(task, snap, blurred, false, cx);
        self.card_frame(task, snap, [7.0, 8.0, 8.0, 9.0], 4.0, false, cx)
            .child(head)
            .child(meta)
            .children(waiting)
            .into_any_element()
    }

    /// The frame both cards share: border, ring when current, click and right click.
    pub(super) fn card_frame(
        &self,
        task: &Task,
        snap: &Snapshot,
        pad: [f32; 4],
        gap: f32,
        open: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let colors = cx.theme().colors().clone();
        let key = card_fold_key(&task.id);
        let current = snap.current.as_deref() == Some(task.id.as_str());
        let focused = snap.focused && snap.stop.as_deref() == Some(key.as_str());
        let attention = row_tier(task.state) == RowTier::Attention;
        let border = match card_border(current, focused, attention) {
            CardBorder::Accent => colors.border_focused,
            CardBorder::Attention => state_colors(task.state, cx).fg.opacity(0.45),
            CardBorder::Plain if open => colors.border,
            CardBorder::Plain => colors.border_variant,
        };
        // Opaque: the ring is a box shadow, which shows through a see-through fill.
        let bg = if current {
            colors
                .panel_background
                .blend(colors.text_accent.opacity(0.06))
        } else {
            colors.element_background
        };
        let ring = BoxShadow {
            color: colors.text_accent.opacity(0.13),
            offset: point(px(0.), px(0.)),
            blur_radius: px(0.),
            spread_radius: px(3.),
            inset: false,
        };
        let hover = colors.element_hover;
        let active = colors.element_active;
        let open_key = key.clone();
        let menu_id = task.id.clone();
        div()
            .id(SharedString::from(key))
            .group(SharedString::from(format!("row-{}", task.id)))
            .w_full()
            .flex()
            .flex_col()
            .gap(rems_from_px(gap))
            .pt(rems_from_px(pad[0]))
            .pr(rems_from_px(pad[1]))
            .pb(rems_from_px(pad[2]))
            .pl(rems_from_px(pad[3]))
            .rounded(px(8.))
            .border_1()
            .border_color(border)
            .bg(bg)
            .cursor_pointer()
            .when(!current, |card| card.hover(|style| style.bg(hover)))
            .active(|style| style.bg(active))
            .when(current, |card| card.shadow(vec![ring]))
            .on_click(cx.listener(move |tree, _: &ClickEvent, window, cx| {
                tree.focus_key(open_key.clone(), window, cx);
                tree.activate(&open_key, cx);
                // A subtask card sits inside its parent's card.
                cx.stop_propagation();
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |tree, event: &MouseDownEvent, window, cx| {
                    tree.open_row_menu(&menu_id, event.position, window, cx);
                    cx.stop_propagation();
                }),
            )
    }

    /// Line 1: glyph, title, unseen dot, reason or age, and the hover buttons. The rename field
    /// replaces only this line.
    pub(super) fn card_head(
        &self,
        task: &Task,
        model: &TreeModel,
        snap: &Snapshot,
        open: bool,
        nested: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let renaming = self
            .rename
            .as_ref()
            .filter(|rename| rename.task_id == task.id);
        if let Some(rename) = renaming {
            return self.rename_field(rename.editor.clone(), cx);
        }
        let blurred = is_blurred(task, snap);
        let unseen = is_unseen(&snap.seen, &task.id, task.state);
        let glyph = StateGlyph::new(
            SharedString::from(format!("glyph-{}", task.id)),
            Some(task_glyph_state(task, model.rows_of(&task.id))),
        )
        .quiet(is_quiet(task, snap.now_ms));
        let glyph = match lane_summary(task) {
            Some(tip) => glyph.tip(tip),
            None => glyph,
        };
        let title = if open {
            open_title(task, blurred)
        } else {
            title_label(task, unseen, blurred, false, cx)
        };
        let rename_id = task.id.clone();
        let tip = snap.tips.get(&task.id).cloned().unwrap_or_default();
        let title = div()
            .id(SharedString::from(format!("title-{}", task.id)))
            .min_w_0()
            .flex_1()
            .child(title)
            .tooltip(Tooltip::text(tip))
            // A double click on the title is a shortcut for F2.
            .on_click(cx.listener(move |tree, event: &ClickEvent, window, cx| {
                if event.click_count() == 2 {
                    tree.start_rename(&rename_id, window, cx);
                    cx.stop_propagation();
                }
            }));
        let group = SharedString::from(format!("row-{}", task.id));
        let fold = (open || !nested).then(|| self.fold_button(&task.id, open, group.clone(), cx));
        let menu = self.menu_button(&task.id, group, cx);
        h_flex()
            .w_full()
            .gap(rems_from_px(7.0_f32))
            .when(!open, |line| line.h(rems_from_px(20.0_f32)))
            .when(open, |line| {
                line.min_h(rems_from_px(20.0_f32)).items_start()
            })
            .child(glyph)
            .child(title)
            .when(unseen, |line| {
                line.child(
                    div()
                        .flex_none()
                        .size(rems_from_px(6.0_f32))
                        .rounded_full()
                        .bg(cx.theme().colors().text_accent),
                )
            })
            .child(right_slot(task, snap, cx))
            .child(menu)
            .children(fold)
            .into_any_element()
    }

    /// Chevron down on a closed card (on hover), chevron up on an open one (always).
    fn fold_button(
        &self,
        id: &str,
        open: bool,
        group: SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = card_fold_key(id);
        let icon = if open {
            IconName::ChevronUp
        } else {
            IconName::ChevronDown
        };
        let button = IconButton::new(SharedString::from(format!("fold-{id}")), icon)
            .icon_size(IconSize::Small)
            .tooltip(Tooltip::text(if open { "Fold" } else { "Unfold" }))
            .on_click(cx.listener(move |tree, _: &ClickEvent, _window, cx| {
                tree.run_entry(
                    EntryAction::SetFold {
                        key: key.clone(),
                        open: !open,
                    },
                    cx,
                );
                cx.stop_propagation();
            }));
        if open {
            button.into_any_element()
        } else {
            button.visible_on_hover(group).into_any_element()
        }
    }

    fn menu_button(&self, id: &str, group: SharedString, cx: &mut Context<Self>) -> AnyElement {
        let task_id = id.to_owned();
        IconButton::new(SharedString::from(format!("menu-{id}")), IconName::Ellipsis)
            .icon_size(IconSize::Small)
            .visible_on_hover(group)
            .tooltip(Tooltip::text(format!("Actions for {id}")))
            .on_click(cx.listener(move |tree, event: &ClickEvent, window, cx| {
                tree.open_row_menu(&task_id, event.position(), window, cx);
                cx.stop_propagation();
            }))
            .into_any_element()
    }

    /// Line 2: avatars, project (pinned), id, stage, branch, diff.
    fn closed_meta(
        &self,
        task: &Task,
        model: &TreeModel,
        snap: &Snapshot,
        pinned_row: bool,
        blurred: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let avatars = card_agents(task, model.rows_of(&task.id));
        let more = more_agents(avatars.len());
        let stage = card_stage(task);
        let branch = task.branch.clone();
        let project = pinned_row.then(|| self.project_of(task, cx));
        let id = &task.id;
        let strip = (!avatars.is_empty()).then(|| {
            h_flex()
                .flex_none()
                .gap_1()
                .children(
                    avatars
                        .iter()
                        .take(MAX_AVATARS)
                        .enumerate()
                        .map(|(index, avatar)| avatar_view(id, index, avatar)),
                )
                .children(more.map(small))
        });
        let project = project.map(|(name, color)| {
            h_flex()
                .flex_none()
                .gap_1()
                .child(private_text(
                    blurred,
                    || BlurBar::new(name.clone(), 12.0),
                    || {
                        Label::new(name.clone())
                            .size(LabelSize::Custom(rems_from_px(12_f32)))
                            .color(Color::Custom(color))
                            .single_line()
                            .into_any_element()
                    },
                ))
                .child(small("·"))
        });
        let stage_part = stage.map(|stage| {
            h_flex()
                .flex_none()
                .gap_1()
                .child(private_text(
                    blurred,
                    || BlurBar::new(stage.clone(), 12.0),
                    || small(stage.clone()).into_any_element(),
                ))
                .when(branch.is_some(), |part| part.child(small("·")))
        });
        let branch_part = div().min_w_0().flex_1().children(branch.map(|name| {
            private_text(
                blurred,
                || BlurBar::new(name.clone(), 11.0).mono(),
                || mono(name.clone(), cx).truncate().into_any_element(),
            )
        }));
        h_flex()
            .w_full()
            .h(rems_from_px(18.0_f32))
            .pl(rems_from_px(21.0_f32))
            .gap(rems_from_px(5.0_f32))
            .children(strip)
            .children(project)
            .when(snap.show_ids, |line| {
                line.child(div().flex_none().child(mono(task.id.clone(), cx)))
            })
            .children(stage_part)
            .child(branch_part)
            .children(diff_label(snap.diff_stats.get(&task.id)))
            .into_any_element()
    }

    /// A row under "N finished": no frame, glyph, title, age.
    fn finished_task_row(
        &self,
        task: &Task,
        model: &TreeModel,
        snap: &Snapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = card_fold_key(&task.id);
        let current = snap.current.as_deref() == Some(task.id.as_str());
        let blurred = is_blurred(task, snap);
        let glyph = StateGlyph::new(
            SharedString::from(format!("glyph-{}", task.id)),
            Some(task_glyph_state(task, model.rows_of(&task.id))),
        );
        let group = SharedString::from(format!("row-{}", task.id));
        let menu = self.menu_button(&task.id, group.clone(), cx);
        let open_key = key.clone();
        let menu_id = task.id.clone();
        self.row_frame(&key, current, snap, cx)
            .id(SharedString::from(key))
            .group(group)
            .h(rems_from_px(24.0_f32))
            .px_2()
            .flex()
            .items_center()
            .gap_2()
            .child(glyph)
            .child(div().min_w_0().flex_1().child(title_label(
                task,
                false,
                blurred,
                snap.show_ids,
                cx,
            )))
            .child(age_label(task, snap))
            .child(menu)
            .on_click(cx.listener(move |tree, _: &ClickEvent, window, cx| {
                tree.focus_key(open_key.clone(), window, cx);
                tree.activate(&open_key, cx);
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |tree, event: &MouseDownEvent, window, cx| {
                    tree.open_row_menu(&menu_id, event.position, window, cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }
}

/// One agent: its icon and its state glyph side by side. An overlapping corner badge drew over
/// the stage name in the macOS shots, so the glyph sits next to the icon.
fn avatar_view(task_id: &str, index: usize, avatar: &AgentAvatar) -> AnyElement {
    let name = format!("avatar-{task_id}-{index}");
    h_flex()
        .id(SharedString::from(name.clone()))
        .flex_none()
        .gap(px(2.))
        .child(AgentIcon::new(Some(avatar.agent.clone())))
        .child(
            StateGlyph::new(SharedString::from(format!("{name}-badge")), avatar.state)
                .size(ShapeSize::Terminal),
        )
        .tooltip(Tooltip::text(agent_label(Some(avatar.agent.as_str()))))
        .into_any_element()
}

#[cfg(test)]
#[path = "task_card_tests.rs"]
mod tests;
