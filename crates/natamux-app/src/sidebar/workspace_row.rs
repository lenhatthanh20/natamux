//! The rows of the sidebar tree (docs/ui-spec.md 4.9, `WorkspaceRow.tsx`): a task row in one of
//! three tiers (attention card, live, quiet), a terminal row under the open task, and the inline
//! rename field. The tree draws them as a flat list, so each row can be scrolled to; the project
//! card around a group's rows is drawn per row.

use gpui::{AnyElement, ClickEvent, FontWeight, MouseButton, MouseDownEvent, Stateful};
use natamux_proto::tasks::task::{Task, TaskState};
use ui::{Tooltip, prelude::*};

use super::sidebar::{agent_mark, lucide, mono, private, pull_request_mark, small};
use super::workspace_tree::{OFFSET_MINUTES, Snapshot, WorkspaceTree};
use crate::app::view_rules::is_unseen;
use crate::components::colors::state_colors;
use crate::components::glyph::StateGlyph;
use crate::components::shape::ShapeSize;
use crate::sidebar::comet::comet_of;
use crate::sidebar::shell_rows::{ShellRow, shell_row_name, shell_tag, task_glyph_state};
use crate::sidebar::sidebar_text::step_text;
use crate::sidebar::sidebar_tree::{RowTier, row_tier};
use crate::sidebar::tree_model::TreeModel;
use crate::sidebar::workspace_groups::{DEFAULT_REPO_ID, is_quiet};
use crate::tasks::task_rules::lane_summary;
use crate::tasks::task_time::{AgeStyle, format_age, format_exact};

impl WorkspaceTree {
    /// The look every focusable row shares: hover, the open task, and the focus ring.
    pub(super) fn row_frame(&self, key: &str, current: bool, snap: &Snapshot, cx: &App) -> Div {
        let colors = cx.theme().colors();
        let focused = snap.focused && snap.stop.as_deref() == Some(key);
        div()
            .w_full()
            .rounded(px(6.))
            .cursor_pointer()
            .hover(|style| style.bg(colors.element_hover))
            .when(current, |row| {
                let bg = if snap.focused {
                    colors.element_selected
                } else {
                    colors.ghost_element_selected
                };
                row.bg(bg)
            })
            .border_2()
            .border_color(if focused {
                colors.border_focused
            } else {
                gpui::transparent_black()
            })
    }

    pub(super) fn task_row(
        &self,
        task: &Task,
        pinned_row: bool,
        model: &TreeModel,
        snap: &Snapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = format!("task:{}", task.id);
        let tier = row_tier(task.state);
        let current = snap.current.as_deref() == Some(task.id.as_str());
        let repo = task.repo.as_deref().unwrap_or(DEFAULT_REPO_ID);
        let blurred = snap.blurred.iter().any(|id| id == repo);
        let renaming = self
            .rename
            .as_ref()
            .filter(|rename| rename.task_id == task.id);
        let is_renaming = renaming.is_some();
        let title_line = self.title_line(task, model, snap, is_renaming, blurred, cx);
        let title: AnyElement = match renaming {
            Some(rename) => self.rename_field(rename.editor.clone(), cx),
            None => title_line,
        };
        let colors = state_colors(task.state, cx);
        let (min_height, pad_y) = match tier {
            RowTier::Attention => (68., 8.),
            RowTier::Live => (40., 6.),
            RowTier::Quiet => (24., 4.),
        };
        let group_name = SharedString::from(format!("row-{}", task.id));
        let open_id = task.id.clone();
        let menu_id = task.id.clone();
        let mut row = self
            .row_frame(&key, current, snap, cx)
            .id(SharedString::from(key.clone()))
            .group(group_name.clone())
            .relative()
            .flex()
            .flex_col()
            .justify_center()
            .min_h(px(min_height))
            .px(px(10.))
            .py(px(pad_y))
            .child(title)
            .on_click(cx.listener(move |tree, _: &ClickEvent, window, cx| {
                tree.focus_key(format!("task:{open_id}"), window, cx);
                tree.activate(&format!("task:{open_id}"), cx);
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |tree, event: &MouseDownEvent, window, cx| {
                    tree.open_row_menu(&menu_id, event.position, window, cx);
                    cx.stop_propagation();
                }),
            );
        if tier == RowTier::Attention {
            row = row.rounded(px(8.)).my(px(2.)).bg(colors.bg);
        }
        let meta = self.meta_line(task, tier, pinned_row, model, snap, blurred, cx);
        let menu_button = (!is_renaming).then(|| {
            let id = task.id.clone();
            div().absolute().top(px(2.)).right(px(4.)).child(
                IconButton::new(
                    SharedString::from(format!("menu-{}", task.id)),
                    IconName::Ellipsis,
                )
                .icon_size(IconSize::Small)
                .visible_on_hover(group_name.clone())
                .tooltip(Tooltip::text(format!("Actions for {}", task.id)))
                .on_click(cx.listener(
                    move |tree, event: &ClickEvent, window, cx| {
                        tree.open_row_menu(&id, event.position(), window, cx);
                        cx.stop_propagation();
                    },
                )),
            )
        });
        row.children(meta).children(menu_button).into_any_element()
    }

    fn title_line(
        &self,
        task: &Task,
        model: &TreeModel,
        snap: &Snapshot,
        renaming: bool,
        blurred: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
        let rename_id = task.id.clone();
        h_flex()
            .w_full()
            .h(px(16.))
            .gap_2()
            .child(glyph)
            .child(
                div()
                    .id(SharedString::from(format!("title-{}", task.id)))
                    .min_w_0()
                    .flex_1()
                    .child(title_label(task, unseen, blurred))
                    // A double click on the title is a shortcut for F2.
                    .on_click(cx.listener(move |tree, event: &ClickEvent, window, cx| {
                        if event.click_count() == 2 {
                            tree.start_rename(&rename_id, window, cx);
                            cx.stop_propagation();
                        }
                    })),
            )
            .when(unseen, |line| {
                line.child(
                    div()
                        .flex_none()
                        .size(px(6.))
                        .rounded_full()
                        .bg(cx.theme().colors().text_accent),
                )
            })
            .when(!renaming, |line| line.child(age_label(task, snap)))
            .into_any_element()
    }

    /// Line 2 of a live row, or lines 2 and 3 of an attention card.
    #[allow(clippy::too_many_arguments)]
    fn meta_line(
        &self,
        task: &Task,
        tier: RowTier,
        pinned_row: bool,
        model: &TreeModel,
        snap: &Snapshot,
        blurred: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let parts = meta_parts(task, blurred, cx);
        match tier {
            RowTier::Quiet => Vec::new(),
            RowTier::Attention => self.card_lines(task, parts, snap, blurred, cx),
            RowTier::Live => vec![live_line(task, pinned_row, model, parts, snap, blurred, cx)],
        }
    }

    /// The question or reason on up to 2 lines, then Answer, id, step, stage and agent.
    fn card_lines(
        &self,
        task: &Task,
        parts: Meta,
        snap: &Snapshot,
        blurred: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let said = snap.cards.get(&task.id).map(|text| {
            div().pl(px(22.)).mt_1().line_clamp(2).child(
                Label::new(private(text, blurred)).size(LabelSize::Custom(rems_from_px(12_f32))),
            )
        });
        let answer = (task.state == TaskState::NeedsInput).then(|| {
            let id = task.id.clone();
            Button::new(SharedString::from(format!("answer-{}", task.id)), "Answer")
                .label_size(LabelSize::XSmall)
                .style(ButtonStyle::Tinted(ui::TintColor::Warning))
                .on_click(cx.listener(move |tree, _: &ClickEvent, _, cx| {
                    tree.activate(&format!("task:{id}"), cx);
                    cx.stop_propagation();
                }))
        });
        let last = meta_row()
            .mt(px(6.))
            .children(answer)
            .child(parts.id)
            .children(parts.step)
            .children(parts.stage)
            .child(div().flex_1())
            .children(parts.agent);
        said.map(IntoElement::into_any_element)
            .into_iter()
            .chain([last.into_any_element()])
            .collect()
    }

    pub(super) fn terminal_row(
        &self,
        task_id: &str,
        row: &ShellRow,
        snap: &Snapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = format!("agent:{task_id}:{}", row.key);
        let blurred = self
            .dashboard
            .read(cx)
            .data
            .task(task_id)
            .map(|task| task.repo.as_deref().unwrap_or(DEFAULT_REPO_ID).to_owned())
            .is_some_and(|repo| snap.blurred.contains(&repo));
        let glyph: AnyElement = if row.plain {
            div()
                .flex_none()
                .size(px(14.))
                .flex()
                .items_center()
                .justify_center()
                .child(lucide("square-terminal").color(Color::Muted))
                .into_any_element()
        } else {
            StateGlyph::new(SharedString::from(format!("{key}-glyph")), row.state)
                .size(ShapeSize::Terminal)
                .into_any_element()
        };
        let activity = row
            .shell
            .as_ref()
            .and_then(|shell| snap.terminal_activity.get(task_id)?.get(shell))
            .map(|text| small(private(text, blurred)).truncate());
        let tag = shell_tag(row).map(|tag| {
            div()
                .id(SharedString::from(format!("{key}-tag")))
                .flex_none()
                .child(mono(private(&tag, blurred), cx))
                .tooltip(Tooltip::text(format!(
                    "Terminal {}",
                    row.shell.as_deref().unwrap_or_default()
                )))
        });
        let agent = row
            .agent
            .as_deref()
            .map(|agent| agent_mark(SharedString::from(format!("{key}-agent")).into(), agent));
        let label = Label::new(private(&row.label, blurred))
            .size(LabelSize::Custom(rems_from_px(12_f32)))
            .weight(FontWeight::MEDIUM)
            .color(if row.plain {
                Color::Muted
            } else {
                Color::Default
            })
            .single_line();
        let open = key.clone();
        self.row_frame(&key, false, snap, cx)
            .id(SharedString::from(key.clone()))
            .h(px(22.))
            .px_2()
            .flex()
            .items_center()
            .gap_2()
            .child(glyph)
            .children(agent)
            .child(label)
            .child(div().min_w_0().flex_1().children(activity))
            .children(tag)
            .tooltip(Tooltip::text(shell_row_name(row)))
            .on_click(cx.listener(move |tree, _: &ClickEvent, window, cx| {
                tree.focus_key(open.clone(), window, cx);
                tree.activate(&open, cx);
            }))
            .into_any_element()
    }

    /// The field that takes the title's place while renaming: Enter saves, Esc cancels (the tree
    /// handles `Back`), and leaving it saves.
    fn rename_field(
        &self,
        editor: gpui::Entity<editor::Editor>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors();
        div()
            .h(px(24.))
            .w_full()
            .px(px(6.))
            .flex()
            .items_center()
            .rounded_sm()
            .border_1()
            .border_color(colors.border_focused)
            .bg(colors.editor_background)
            .capture_key_down(cx.listener(|tree, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "enter" {
                    tree.finish_rename(true, true, window, cx);
                    cx.stop_propagation();
                }
            }))
            .child(editor)
            .into_any_element()
    }
}

/// The title in the look of its tier: bold when unseen or waiting, muted when finished.
fn title_label(task: &Task, unseen: bool, blurred: bool) -> Label {
    let tier = row_tier(task.state);
    let weight = match tier {
        RowTier::Quiet => FontWeight::NORMAL,
        _ if unseen || tier == RowTier::Attention => FontWeight::SEMIBOLD,
        _ => FontWeight::MEDIUM,
    };
    let size = if tier == RowTier::Quiet {
        12_f32
    } else {
        13_f32
    };
    let finished = matches!(task.state, TaskState::Done | TaskState::Cancelled);
    let title = Label::new(private(&task.title, blurred))
        .size(LabelSize::Custom(rems_from_px(size)))
        .weight(weight)
        .color(if finished {
            Color::Muted
        } else {
            Color::Default
        })
        .truncate();
    if task.state == TaskState::Cancelled {
        title.strikethrough()
    } else {
        title
    }
}

/// The age of the last change; the menu button takes its place on hover.
fn age_label(task: &Task, snap: &Snapshot) -> Stateful<Div> {
    let age = format_age(
        &task.updated_at,
        snap.now_ms,
        AgeStyle::Short,
        OFFSET_MINUTES,
    );
    div()
        .id(SharedString::from(format!("age-{}", task.id)))
        .flex_none()
        .group_hover(SharedString::from(format!("row-{}", task.id)), |style| {
            style.invisible()
        })
        .child(small(age))
        .tooltip(Tooltip::text(format_exact(
            &task.updated_at,
            OFFSET_MINUTES,
        )))
}

fn meta_row() -> Div {
    h_flex().w_full().min_w_0().gap(px(6.)).pl(px(22.))
}

/// The parts that live rows and cards share.
struct Meta {
    id: Stateful<Div>,
    step: Option<Label>,
    stage: Option<Label>,
    agent: Option<AnyElement>,
}

fn meta_parts(task: &Task, blurred: bool, cx: &App) -> Meta {
    let comet = comet_of(task);
    Meta {
        // The branch is mostly the title, so it waits in the tooltip.
        id: div()
            .id(SharedString::from(format!("id-{}", task.id)))
            .flex_none()
            .child(mono(private(&task.id, blurred), cx))
            .when_some(task.branch.clone().filter(|_| !blurred), |id, branch| {
                id.tooltip(Tooltip::text(branch))
            }),
        step: step_text(comet.at, comet.of).map(|text| mono(private(&text, blurred), cx)),
        stage: comet
            .stage
            .as_deref()
            .map(|stage| small(private(stage, blurred)).truncate()),
        agent: task.agent.as_deref().map(|agent| {
            agent_mark(
                SharedString::from(format!("agent-{}", task.id)).into(),
                agent,
            )
            .into_any_element()
        }),
    }
}

/// Line 2 of a live row: project (pinned rows), id, step, stage, subtasks, what the agent does,
/// pull request and agent.
fn live_line(
    task: &Task,
    pinned_row: bool,
    model: &TreeModel,
    parts: Meta,
    snap: &Snapshot,
    blurred: bool,
    cx: &App,
) -> AnyElement {
    let repo_chip = pinned_row.then(|| {
        let repo = task.repo.as_deref().unwrap_or(DEFAULT_REPO_ID);
        let name = snap
            .repos
            .iter()
            .find(|known| known.id == repo)
            .map_or(repo, |known| known.name.as_str());
        div()
            .flex_none()
            .px_1()
            .rounded_sm()
            .bg(cx.theme().colors().element_background)
            .child(mono(private(name, blurred), cx))
    });
    let subtasks = snap.subtasks.get(&task.id).map(|text| {
        h_flex()
            .id(SharedString::from(format!("subtasks-{}", task.id)))
            .flex_none()
            .gap_0p5()
            .child(lucide("list-tree").color(Color::Muted))
            .child(small(text.clone()))
            .tooltip(Tooltip::text("Subtasks done"))
    });
    // Open, the terminal rows say what each agent does.
    let doing = (task.state == TaskState::Running && !model.is_open(&task.id))
        .then(|| snap.activity.get(&task.id))
        .flatten()
        .map(|text| small(private(text, blurred)).truncate());
    meta_row()
        .mt(px(2.))
        .children(repo_chip)
        .child(parts.id)
        .children(parts.step)
        .children(parts.stage)
        .children(subtasks)
        .child(div().min_w_0().flex_1().children(doing))
        .children(pull_request_mark(task))
        .children(parts.agent)
        .into_any_element()
}
