//! The parts of the stage list (`StageList.tsx`): a stage row, a group block, a branch block with
//! a list of stages for each path, and the problem lines under them.

use gpui::{ClickEvent, DragMoveEvent, FontWeight, KeyDownEvent};
use natamux_proto::stages::pipeline_rules::{IssueLevel, PipelineIssue};
use natamux_proto::stages::position::describe_condition;
use natamux_proto::stages::schema::{BranchPath, Stage, StageBranch, StageGroup, Step};
use ui::{Checkbox, ToggleState, Tooltip, prelude::*};

use super::editor_fields::{has_custom_prompt, set_enabled};
use super::editor_labels::{branch_name, group_name, path_name};
use super::kind_icon::{branch_icon, grip_icon, kind_icon, problem_icon, prompt_icon, waits_icon};
use super::preview_rules::waits_for_person;
use super::stage_list::StageList;
use crate::components::colors::signal;
use crate::components::tab_order;
use natamux_proto::stages::capabilities::stage_name;

/// A row (or a group, or a branch) that is being dragged.
#[derive(Clone)]
pub(super) struct DraggedStage(pub(super) String);

/// What follows the pointer while a row is dragged.
pub(super) struct DragGhost(pub(super) SharedString);

const ROW_HEIGHT: f32 = 28.;

/// What the rows read from the draft: the picked id and the problems.
pub(super) struct Ctx<'a> {
    pub(super) selected: Option<&'a str>,
    pub(super) issues: &'a [PipelineIssue],
}

impl Ctx<'_> {
    fn of(&self, id: &str) -> Vec<PipelineIssue> {
        self.issues
            .iter()
            .filter(|issue| issue.stage.as_deref() == Some(id))
            .cloned()
            .collect()
    }
}

/// The problems of a row, 12 px with the alert icon.
pub(super) fn issue_lines(issues: &[PipelineIssue], cx: &App) -> impl IntoElement + use<> {
    let warning = signal(cx).fg;
    v_flex().gap_0p5().children(issues.iter().map(move |issue| {
        let (icon_color, label_color) = match issue.level {
            IssueLevel::Error => (Color::Error, Color::Error),
            IssueLevel::Warning => (Color::Custom(warning), Color::Custom(warning)),
        };
        h_flex()
            .items_start()
            .gap_1p5()
            .child(
                div()
                    .mt_0p5()
                    .child(problem_icon().size(IconSize::XSmall).color(icon_color)),
            )
            .child(
                div().min_w_0().child(
                    Label::new(issue.message.clone())
                        .size(LabelSize::XSmall)
                        .color(label_color),
                ),
            )
    }))
}

fn caps(text: &str) -> Label {
    Label::new(text.to_uppercase())
        .size(LabelSize::XSmall)
        .weight(FontWeight::SEMIBOLD)
        .color(Color::Muted)
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
    fn drop_line(&self, id: &str, cx: &App) -> Option<impl IntoElement + use<>> {
        let (_, side) = self.target.as_ref().filter(|(target, _)| target == id)?;
        let line = div()
            .absolute()
            .left(px(4.))
            .right(px(4.))
            .h(px(2.))
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

    fn drag_source<E: StatefulInteractiveElement>(element: E, id: &str, label: String) -> E {
        let (id, label) = (id.to_owned(), SharedString::from(label));
        element.on_drag(DraggedStage(id), move |_, _, _, cx| {
            cx.new(|_| DragGhost(label.clone()))
        })
    }

    /// The button of a row: kind icon, name, and the marks. It takes the focus and picks the stage.
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
            Color::Muted
        };
        let label = Label::new(stage_name(stage)).color(color).truncate();
        let label = if on { label } else { label.strikethrough() };
        let key_id = id.clone();
        h_flex()
            .id(SharedString::from(format!("stage-row-{id}")))
            .track_focus(&handle)
            .flex_1()
            .min_w_0()
            .h_full()
            .gap_1p5()
            .px_1()
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
            .child(kind_icon(stage.kind).size(IconSize::Small).color(if on {
                Color::Default
            } else {
                Color::Disabled
            }))
            .child(label)
            .when(waits_for_person(stage), |this| {
                this.child(
                    div()
                        .id(SharedString::from(format!("waits-{id}")))
                        .child(waits_icon().size(IconSize::XSmall).color(Color::Muted))
                        .tooltip(Tooltip::text("Waits for approval")),
                )
            })
            .when(has_custom_prompt(stage), |this| {
                this.child(
                    div()
                        .id(SharedString::from(format!("prompt-{id}")))
                        .child(prompt_icon().size(IconSize::XSmall).color(Color::Muted))
                        .tooltip(Tooltip::text("Custom prompt")),
                )
            })
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
        let (hover, selected_bg) = (colors.element_hover, colors.element_selected);
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
            .px_1()
            .rounded_md()
            .when(picked, |this| this.bg(selected_bg))
            .when(!picked, |this| this.hover(|style| style.bg(hover)))
            .child(grip_icon().size(IconSize::Small).color(Color::Disabled))
            .child(
                Checkbox::new(SharedString::from(format!("on-{id}")), on.into()).on_click(
                    cx.listener(move |this, _: &ToggleState, _, cx| {
                        this.apply(|steps| set_enabled(steps, &toggle_id, !on), None, cx);
                    }),
                ),
            )
            .child(button)
            .child(
                IconButton::new(SharedString::from(format!("more-{id}")), IconName::Ellipsis)
                    .icon_size(IconSize::Small)
                    .visible_on_hover(hover_group)
                    .tooltip(Tooltip::text(format!("Actions for {name}")))
                    .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                        this.open_stage_menu(&menu_id, event.position(), window, cx);
                    })),
            );
        let line = Self::drag_source(line, &id, name);
        let shell = v_flex()
            .id(SharedString::from(format!("item-{id}")))
            .relative()
            .child(line)
            .when(!mine.is_empty(), |this| {
                this.child(div().pb_1().pl(px(48.)).child(issue_lines(&mine, cx)))
            })
            .children(self.drop_line(&id, cx));
        self.drop_zone(shell, &id, cx).into_any_element()
    }

    fn editor_click(&self, id: String) -> impl Fn(&ClickEvent, &mut Window, &mut App) + use<> {
        let editor = self.editor.clone();
        move |_, _, cx| editor.update(cx, |editor, cx| editor.select(&id, cx))
    }

    pub(super) fn select(&mut self, id: &str, cx: &mut Context<Self>) {
        self.editor.update(cx, |editor, cx| editor.select(id, cx));
    }

    fn group_block(
        &mut self,
        group: &StageGroup,
        ctx: &Ctx,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = group.id.clone();
        let mine = ctx.of(&id);
        let head = h_flex()
            .id(SharedString::from(format!("group-head-{id}")))
            .h(px(24.))
            .gap_1p5()
            .px_1()
            .cursor_grab()
            .child(grip_icon().size(IconSize::XSmall).color(Color::Disabled))
            .child(caps(&group_name(group)))
            .child(
                Label::new("side by side")
                    .size(LabelSize::XSmall)
                    .color(Color::Muted),
            );
        let head = Self::drag_source(head, &id, group_name(group));
        let rows: Vec<AnyElement> = group
            .parallel
            .iter()
            .map(|stage| self.stage_row(stage, ctx, window, cx))
            .collect();
        let colors = cx.theme().colors();
        let block = v_flex()
            .id(SharedString::from(format!("group-{id}")))
            .relative()
            .p_1()
            .rounded_lg()
            .border_1()
            .border_color(colors.border)
            .bg(colors.elevated_surface_background)
            .child(head)
            .when(!mine.is_empty(), |this| {
                this.child(div().px_1().pb_1().child(issue_lines(&mine, cx)))
            })
            .child(v_flex().gap_0p5().children(rows))
            .children(self.drop_line(&id, cx));
        self.drop_zone(block, &id, cx).into_any_element()
    }

    fn branch_block(
        &mut self,
        branch: &StageBranch,
        ctx: &Ctx,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = branch.id.clone();
        let mine = ctx.of(&id);
        let head = self.branch_head(branch, ctx.selected == Some(id.as_str()), &mine, cx);
        let paths: Vec<AnyElement> = branch
            .paths
            .iter()
            .map(|path| self.path_block(path, ctx, window, cx))
            .collect();
        let colors = cx.theme().colors();
        let block = v_flex()
            .id(SharedString::from(format!("branch-{id}")))
            .relative()
            .p_1()
            .rounded_lg()
            .border_1()
            .border_color(colors.border)
            .bg(colors.elevated_surface_background)
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(px(2.))
                    .rounded_l_lg()
                    .bg(colors.text_accent),
            )
            .child(head)
            .child(v_flex().gap_1().pl_2().children(paths))
            .children(self.drop_line(&id, cx));
        self.drop_zone(block, &id, cx).into_any_element()
    }

    /// One path of a branch: its name and condition, then its stages.
    fn path_block(
        &mut self,
        path: &BranchPath,
        ctx: &Ctx,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let stages: Vec<AnyElement> = path
            .stages
            .iter()
            .map(|stage| self.stage_row(stage, ctx, window, cx))
            .collect();
        let condition = if path.when.is_none() {
            "Otherwise".to_owned()
        } else {
            format!("if {}", describe_condition(path))
        };
        let title = h_flex()
            .id(SharedString::from(format!("path-{}", path.id)))
            .min_h(px(24.))
            .gap_1p5()
            .px_1()
            .child(caps(&path_name(path)))
            .child(
                Label::new(condition)
                    .size(LabelSize::XSmall)
                    .color(Color::Muted)
                    .truncate(),
            );
        let title = self.drop_zone(title, &path.id, cx);
        v_flex()
            .child(title)
            .child(if stages.is_empty() {
                div()
                    .px_2()
                    .pb_1()
                    .child(
                        Label::new("No stage yet")
                            .size(LabelSize::XSmall)
                            .color(Color::Muted),
                    )
                    .into_any_element()
            } else {
                v_flex().gap_0p5().children(stages).into_any_element()
            })
            .into_any_element()
    }

    /// The header of a branch block: its row (pick, menu) and its problems.
    fn branch_head(
        &mut self,
        branch: &StageBranch,
        picked: bool,
        mine: &[PipelineIssue],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors();
        let (hover, selected_bg) = (colors.element_hover, colors.element_selected);
        let id = branch.id.clone();
        let name = branch_name(branch);
        let handle = self.focus_of(&id, cx);
        tab_order::push(&handle);
        let failing = mine.iter().any(|issue| issue.level == IssueLevel::Error);
        let hover_group = SharedString::from(format!("row-{id}"));
        let (key_id, menu_id) = (id.clone(), id.clone());
        let button = h_flex()
            .id(SharedString::from(format!("stage-row-{id}")))
            .track_focus(&handle)
            .flex_1()
            .min_w_0()
            .h_full()
            .gap_1p5()
            .px_1()
            .rounded_sm()
            .cursor_pointer()
            .on_click(self.editor_click(id.clone()))
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    this.select(&key_id, cx);
                }
            }))
            .child(branch_icon().size(IconSize::Small).color(Color::Accent))
            .child(
                Label::new(name.clone())
                    .weight(FontWeight::MEDIUM)
                    .color(if failing {
                        Color::Error
                    } else {
                        Color::Default
                    })
                    .truncate(),
            )
            .child(Label::new("If").size(LabelSize::XSmall).color(Color::Muted));
        let line = h_flex()
            .id(SharedString::from(format!("row-{id}")))
            .group(hover_group.clone())
            .h(px(ROW_HEIGHT))
            .flex_none()
            .gap_1p5()
            .px_1()
            .rounded_md()
            .when(picked, |this| this.bg(selected_bg))
            .when(!picked, |this| this.hover(|style| style.bg(hover)))
            .child(grip_icon().size(IconSize::Small).color(Color::Disabled))
            .child(button)
            .child(
                IconButton::new(SharedString::from(format!("more-{id}")), IconName::Ellipsis)
                    .icon_size(IconSize::Small)
                    .visible_on_hover(hover_group)
                    .tooltip(Tooltip::text(format!("Actions for {name}")))
                    .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                        this.open_branch_menu(&menu_id, event.position(), window, cx);
                    })),
            );
        let line = Self::drag_source(line, &id, name);
        v_flex()
            .child(line)
            .when(!mine.is_empty(), |this| {
                this.child(div().px_1().pb_1().child(issue_lines(mine, cx)))
            })
            .into_any_element()
    }
}
