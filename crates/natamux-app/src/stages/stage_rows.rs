//! The parts of the stage list (`StageList.tsx`): a stage row, a group block, and a branch block
//! with a list of stages and groups for each path.

use gpui::{ClickEvent, DragMoveEvent, FontWeight, KeyDownEvent};
use natamux_proto::stages::pipeline_rules::{IssueLevel, PipelineIssue};
use natamux_proto::stages::schema::{Stage, Step};
use ui::{Checkbox, ToggleState, Tooltip, prelude::*};

use super::editor_catalog::is_custom;
use super::editor_fields::{has_custom_prompt, set_enabled};
use super::kind_icon::{kind_icon, prompt_icon, waits_icon};
use super::preview_rules::waits_for_person;
use super::source_tag::source_tag;
use super::stage_list::{StageList, issue_lines};
use crate::components::tab_order;
use natamux_proto::stages::capabilities::stage_name;

/// A row (or a group, or a branch) that is being dragged.
#[derive(Clone)]
pub(super) struct DraggedStage(pub(super) String);

/// What follows the pointer while a row is dragged.
pub(super) struct DragGhost(pub(super) SharedString);

pub(super) const ROW_HEIGHT: f32 = 28.;

/// What the rows read from the draft: the picked id and the problems.
pub(super) struct Ctx<'a> {
    pub(super) selected: Option<&'a str>,
    pub(super) issues: &'a [PipelineIssue],
}

impl Ctx<'_> {
    pub(super) fn of(&self, id: &str) -> Vec<PipelineIssue> {
        self.issues
            .iter()
            .filter(|issue| issue.stage.as_deref() == Some(id))
            .cloned()
            .collect()
    }
}

/// An uppercase caption, 10.5 px medium, in the placeholder colour.
pub(super) fn caps(text: &str) -> Label {
    Label::new(text.to_uppercase())
        .size(LabelSize::Custom(rems_from_px(10.5_f32)))
        .weight(FontWeight::MEDIUM)
        .color(Color::Placeholder)
}

/// A label of `size` px.
pub(super) fn sized(text: impl Into<SharedString>, size: f32) -> Label {
    Label::new(text).size(LabelSize::Custom(rems_from_px(size)))
}

/// The drag grip: two columns of three small dots, `size` px square, placeholder colour.
pub(super) fn grip(size: f32, cx: &App) -> AnyElement {
    let color = cx.theme().colors().text_placeholder;
    let dot = move || div().size(px(2.)).rounded_full().bg(color);
    let row = move || h_flex().gap(px(2.)).child(dot()).child(dot());
    v_flex()
        .flex_none()
        .size(rems_from_px(size))
        .gap(px(2.))
        .items_center()
        .justify_center()
        .child(row())
        .child(row())
        .child(row())
        .into_any_element()
}

/// The "..." button in a 20 px square: hidden until the row is hovered, unless `shown`.
pub(super) fn more_button(
    id: &str,
    group: SharedString,
    shown: bool,
    name: &str,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let button = IconButton::new(SharedString::from(format!("more-{id}")), IconName::Ellipsis)
        .size(ButtonSize::Compact)
        .icon_size(IconSize::Small)
        .tooltip(Tooltip::text(format!("Actions for {name}")))
        .on_click(on_click);
    let button = if shown {
        button.into_any_element()
    } else {
        button.visible_on_hover(group).into_any_element()
    };
    div()
        .flex_none()
        .size(rems_from_px(20.0_f32))
        .flex()
        .items_center()
        .justify_center()
        .child(button)
        .into_any_element()
}

impl StageList {
    /// Every step of the list, in order.
    pub(super) fn steps(
        &mut self,
        steps: &[Step],
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let selected = self.editor.read(cx).selected_id();
        let issues = self.editor.read(cx).issues().to_vec();
        let ctx = Ctx {
            selected: selected.as_deref(),
            issues: &issues,
        };
        steps
            .iter()
            .map(|step| match step {
                Step::Stage(stage) => self.stage_row(stage, &ctx, window, cx),
                Step::Group(group) => self.group_block(group, &ctx, window, cx),
                Step::Branch(branch) => self.branch_block(branch, &ctx, window, cx),
            })
            .collect()
    }

    /// The accent line where a dragged row would land, next to `id`.
    pub(super) fn drop_line(&self, id: &str, cx: &App) -> Option<impl IntoElement + use<>> {
        let (_, side) = self.target.as_ref().filter(|(target, _)| target == id)?;
        let line = div()
            .absolute()
            .left(px(4.))
            .right(px(4.))
            .h(rems_from_px(2.0_f32))
            .rounded_full()
            .bg(cx.theme().colors().text_accent);
        Some(match side {
            super::editor_moves::Side::Before => line.top(px(-2.)),
            super::editor_moves::Side::After => line.bottom(px(-2.)),
        })
    }

    /// Makes `element` a place to drop a row, and tracks where on it the pointer is.
    pub(super) fn drop_zone<E: InteractiveElement>(
        &self,
        element: E,
        id: &str,
        cx: &mut Context<Self>,
    ) -> E {
        let (over, dropped) = (id.to_owned(), id.to_owned());
        element
            .on_drag_move(
                cx.listener(move |this, event: &DragMoveEvent<DraggedStage>, _, cx| {
                    this.drag_over(&over, event, cx);
                }),
            )
            .on_drop(cx.listener(move |this, dragged: &DraggedStage, _, cx| {
                this.dropped(&dropped, dragged, cx);
            }))
    }

    pub(super) fn drag_source<E: StatefulInteractiveElement>(
        element: E,
        id: &str,
        label: String,
    ) -> E {
        let (id, label) = (id.to_owned(), SharedString::from(label));
        element.on_drag(DraggedStage(id), move |_, _, _, cx| {
            cx.new(|_| DragGhost(label.clone()))
        })
    }

    /// The button of a row: kind icon, name, and the marks at the right edge. It takes the focus
    /// and picks the stage.
    fn row_button(
        &mut self,
        stage: &Stage,
        failing: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focus_border = cx.theme().colors().border_focused;
        let id = stage.id.clone();
        let handle = self.focus_of(&id, cx);
        tab_order::push(&handle);
        let on = stage.is_on();
        let color = if failing {
            Color::Error
        } else if on {
            Color::Default
        } else {
            Color::Placeholder
        };
        let label = sized(stage_name(stage), 13.).color(color).truncate();
        let label = if on { label } else { label.strikethrough() };
        let key_id = id.clone();
        let custom = is_custom(stage);
        h_flex()
            .id(SharedString::from(format!("stage-row-{id}")))
            .track_focus(&handle)
            .flex_1()
            .min_w_0()
            .h_full()
            .gap_1p5()
            .rounded_sm()
            .cursor_pointer()
            .when(handle.is_focused(window), |this| {
                this.border_1().border_color(focus_border)
            })
            .on_click(self.editor_click(id.clone()))
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    this.select(&key_id, cx);
                }
            }))
            .child(
                kind_icon(stage.kind)
                    .size(IconSize::Small)
                    .color(if custom { Color::Accent } else { Color::Muted }),
            )
            .child(div().flex_1().min_w_0().child(label))
            .when(waits_for_person(stage), |this| {
                this.child(
                    div()
                        .id(SharedString::from(format!("waits-{id}")))
                        .child(
                            waits_icon()
                                .size(IconSize::XSmall)
                                .color(Color::Placeholder),
                        )
                        .tooltip(Tooltip::text("Waits for approval")),
                )
            })
            .when(has_custom_prompt(stage), |this| {
                this.child(
                    div()
                        .id(SharedString::from(format!("prompt-{id}")))
                        .child(
                            prompt_icon()
                                .size(IconSize::XSmall)
                                .color(Color::Placeholder),
                        )
                        .tooltip(Tooltip::text("Custom prompt")),
                )
            })
            .when(custom, |this| this.child(source_tag(true, cx)))
            .into_any_element()
    }

    pub(super) fn stage_row(
        &mut self,
        stage: &Stage,
        ctx: &Ctx,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors();
        let (hover, selected_bg) = (colors.ghost_element_hover, colors.ghost_element_selected);
        let id = stage.id.clone();
        let mine = ctx.of(&id);
        let failing = mine.iter().any(|issue| issue.level == IssueLevel::Error);
        let (on, picked) = (stage.is_on(), ctx.selected == Some(id.as_str()));
        let hover_group = SharedString::from(format!("row-{id}"));
        let name = stage_name(stage);
        let (toggle_id, menu_id) = (id.clone(), id.clone());
        let button = self.row_button(stage, failing, window, cx);
        let line = h_flex()
            .id(SharedString::from(format!("row-{id}")))
            .group(hover_group.clone())
            .h(px(ROW_HEIGHT))
            .flex_none()
            .gap_1p5()
            .pl(px(2.))
            .pr(px(4.))
            .rounded(px(5.))
            .when(picked, |this| this.bg(selected_bg))
            .when(!picked, |this| this.hover(|style| style.bg(hover)))
            .child(grip(12., cx))
            .child(
                Checkbox::new(SharedString::from(format!("on-{id}")), on.into()).on_click(
                    cx.listener(move |this, _: &ToggleState, _, cx| {
                        this.apply(|steps| set_enabled(steps, &toggle_id, !on), None, cx);
                    }),
                ),
            )
            .child(button)
            .child(more_button(
                &id,
                hover_group,
                picked,
                &name,
                cx.listener(move |this, event: &ClickEvent, window, cx| {
                    this.open_stage_menu(&menu_id, event.position(), window, cx);
                }),
            ));
        let line = Self::drag_source(line, &id, name);
        let shell = v_flex()
            .id(SharedString::from(format!("item-{id}")))
            .relative()
            .child(line)
            .when(!mine.is_empty(), |this| {
                this.child(div().pb(px(3.)).pl(px(36.)).child(issue_lines(&mine, cx)))
            })
            .children(self.drop_line(&id, cx));
        self.drop_zone(shell, &id, cx).into_any_element()
    }

    pub(super) fn editor_click(
        &self,
        id: String,
    ) -> impl Fn(&ClickEvent, &mut Window, &mut App) + use<> {
        let editor = self.editor.clone();
        move |_, _, cx| editor.update(cx, |editor, cx| editor.select(&id, cx))
    }

    pub(super) fn select(&mut self, id: &str, cx: &mut Context<Self>) {
        self.editor.update(cx, |editor, cx| editor.select(id, cx));
    }
}
