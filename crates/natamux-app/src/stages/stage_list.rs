//! The "Stages" group of the Stages tab (layout-b 5.2): the stages in pipeline order in one card,
//! as rows, group blocks and branch blocks. A row turns its stage on or off, opens its sheet, and
//! has a menu. Rows move by drag and drop, or Alt+Up and Alt+Down on the focused row. The rows are
//! in `stage_rows`, the blocks in `stage_blocks` and `branch_details`, the menus in
//! `stage_list_menus`.

use std::collections::HashMap;

use gpui::{DragMoveEvent, Entity, FocusHandle, Subscription};
use natamux_proto::stages::pipeline_rules::{IssueLevel, PipelineIssue};
use natamux_proto::stages::schema::Step;
use ui::prelude::*;

use super::add_menu::add_button;
use super::editor_moves::{Side, drop_stage, move_stage};
use super::kind_icon::problem_icon;
use super::list_rules::stage_count;
use super::pipeline_editor::PipelineEditor;
use super::stage_rows::{DragGhost, DraggedStage};
use crate::app::shortcuts::{MoveStageDown, MoveStageUp};
use crate::components::colors::signal;
use crate::components::settings_card::{
    SeparatorInset, SettingsCard, SettingsGroup, SettingsRow, foot_text,
};
use crate::components::tab_order;

/// The footnote of the list.
const PIPELINE_NOTE: &str = "New tasks use this pipeline. Tasks on their way keep their stages, \
                             but use the new prompts and approvals.";

pub struct StageList {
    pub(super) editor: Entity<PipelineEditor>,
    /// One handle for each row, by stage id, so focus stays on a row that moves.
    pub(super) focuses: HashMap<String, FocusHandle>,
    add_stage: FocusHandle,
    /// The focus stops drawn last, in order: the Tab ring of the dialog takes them.
    stops: Vec<FocusHandle>,
    /// Where a dragged row would land.
    pub(super) target: Option<(String, Side)>,
    /// The row to focus once the list shows a move, since a row that changes group is new.
    pub(super) refocus: Option<String>,
    _watch: Subscription,
}

impl StageList {
    pub fn new(editor: Entity<PipelineEditor>, cx: &mut Context<Self>) -> Self {
        let watch = cx.observe(&editor, |_, _, cx| cx.notify());
        StageList {
            editor,
            focuses: HashMap::new(),
            add_stage: cx.focus_handle(),
            stops: Vec::new(),
            target: None,
            refocus: None,
            _watch: watch,
        }
    }

    /// The focus stops of the list, in drawn order, for the Tab ring of the page.
    pub fn tab_stops(&self) -> Vec<FocusHandle> {
        self.stops.clone()
    }

    /// The focus of the Add button, for the flow tests.
    #[cfg(test)]
    pub fn add_stage_focus(&self) -> FocusHandle {
        self.add_stage.clone()
    }

    /// The focus of each row by stage id, for the flow tests.
    #[cfg(test)]
    pub fn row_focuses(&self) -> Vec<(String, FocusHandle)> {
        self.focuses
            .iter()
            .map(|(id, handle)| (id.clone(), handle.clone()))
            .collect()
    }

    pub(super) fn focus_of(&mut self, id: &str, cx: &mut Context<Self>) -> FocusHandle {
        self.focuses
            .entry(id.to_owned())
            .or_insert_with(|| cx.focus_handle().tab_stop(true))
            .clone()
    }

    /// Takes the steps a rule makes from the draft, and sets them when they differ.
    pub(super) fn apply(
        &mut self,
        rule: impl FnOnce(&[Step]) -> Vec<Step>,
        refocus: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let next = rule(self.editor.read(cx).steps());
        if next == self.editor.read(cx).steps() {
            return;
        }
        self.refocus = refocus;
        self.editor.update(cx, |editor, cx| editor.edit(next, cx));
    }

    pub(super) fn move_by(&mut self, id: &str, direction: i32, cx: &mut Context<Self>) {
        self.apply(
            |steps| move_stage(steps, id, direction),
            Some(id.to_owned()),
            cx,
        );
    }

    fn move_focused(&mut self, direction: i32, window: &Window, cx: &mut Context<Self>) {
        let focused = self
            .focuses
            .iter()
            .find(|(_, handle)| handle.is_focused(window))
            .map(|(id, _)| id.clone());
        if let Some(id) = focused {
            self.move_by(&id, direction, cx);
        }
    }

    /// Looks at where a dragged row is, to show the drop line and work out its side.
    pub(super) fn drag_over(
        &mut self,
        id: &str,
        event: &DragMoveEvent<DraggedStage>,
        cx: &mut Context<Self>,
    ) {
        let dragged = event.drag(cx).0.clone();
        let hovered = event.bounds.contains(&event.event.position);
        if hovered && dragged != id {
            let side = if event.event.position.y < event.bounds.center().y {
                Side::Before
            } else {
                Side::After
            };
            let next = Some((id.to_owned(), side));
            if self.target != next {
                self.target = next;
                cx.notify();
            }
        } else if self.target.as_ref().is_some_and(|(target, _)| target == id) {
            self.target = None;
            cx.notify();
        }
    }

    pub(super) fn dropped(&mut self, id: &str, dragged: &DraggedStage, cx: &mut Context<Self>) {
        let side = self
            .target
            .take()
            .filter(|(target, _)| target == id)
            .map_or(Side::After, |(_, side)| side);
        if dragged.0 != id {
            let from = dragged.0.clone();
            self.apply(
                |steps| drop_stage(steps, &from, id, side),
                Some(from.clone()),
                cx,
            );
        }
        cx.notify();
    }
}

impl StageList {
    /// Every step of the list, in order: the top-level items of the card.
    fn steps(
        &mut self,
        steps: &[Step],
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let issues = self.editor.read(cx).issues().to_vec();
        steps
            .iter()
            .map(|step| match step {
                Step::Stage(stage) => self.stage_row(stage, &issues, window, cx),
                Step::Group(group) => self.group_block(group, &issues, window, cx),
                Step::Branch(branch) => self.branch_block(branch, &issues, window, cx),
            })
            .collect()
    }
}

impl Render for StageList {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The line goes with the drag: a drop outside any row leaves no target behind.
        if !cx.has_active_drag() {
            self.target = None;
        }
        let steps = self.editor.read(cx).steps().to_vec();
        let general = self.editor.read(cx).issues_of(None);
        tab_order::begin();
        tab_order::push(&self.add_stage);
        let mut rows = self.steps(&steps, window, cx);
        self.stops = tab_order::end();
        if rows.is_empty() {
            rows.push(
                SettingsRow::new("no-stage", "No stage yet")
                    .label_color(Color::Muted)
                    .into_any_element(),
            );
        }
        if let Some(handle) = self
            .refocus
            .take()
            .and_then(|id| self.focuses.get(&id).cloned())
        {
            window.focus(&handle, cx);
        }
        let mut group = SettingsGroup::new("Stages")
            .count(stage_count(&steps).to_string())
            .action(add_button(&self.editor, &self.add_stage))
            .child(SettingsCard::new(SeparatorInset::Tile).children(rows));
        if !general.is_empty() {
            group = group.footnote_child(issue_lines(&general, cx));
        }
        let note = h_flex()
            .items_start()
            .gap(rems_from_px(6.0_f32))
            .child(
                div().mt(rems_from_px(2.0_f32)).child(
                    Icon::new(IconName::Info)
                        .size(IconSize::XSmall)
                        .color(Color::Muted),
                ),
            )
            .child(div().min_w_0().flex_1().child(foot_text(PIPELINE_NOTE)));
        v_flex()
            .id("stage-list")
            .key_context("StageList")
            .on_action(cx.listener(|this, _: &MoveStageUp, window, cx| {
                this.move_focused(-1, window, cx);
            }))
            .on_action(cx.listener(|this, _: &MoveStageDown, window, cx| {
                this.move_focused(1, window, cx);
            }))
            .w_full()
            .child(group.footnote_child(note))
    }
}

impl Render for DragGhost {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        div()
            .px_2()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(colors.border)
            .bg(colors.elevated_surface_background)
            .child(Label::new(self.0.clone()).size(LabelSize::Small))
    }
}

/// The problems of a row, 11 px with the alert icon.
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
                        .size(LabelSize::Custom(rems_from_px(11.0_f32)))
                        .color(label_color),
                ),
            )
    }))
}

#[cfg(test)]
#[path = "stage_list_tests.rs"]
mod tests;
