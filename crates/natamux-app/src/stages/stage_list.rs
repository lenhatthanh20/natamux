//! The stage list of the Stages tab (`StageList.tsx`, docs/ui-spec.md 4.31): the stages in
//! pipeline order, as rows, group blocks and branch blocks. A row turns its stage on or off,
//! picks it, and has a menu. Rows move by drag and drop, or Alt+Up and Alt+Down on the focused
//! row. The parts of a row are in `stage_rows`, the menus and confirms in `stage_list_menus`.

use std::collections::HashMap;

use gpui::{DragMoveEvent, Entity, FocusHandle, Subscription};
use natamux_proto::stages::schema::Step;
use ui::{Button, prelude::*};

use super::editor_branches::add_branch;
use super::editor_find::used_ids;
use super::editor_moves::{Side, drop_stage, move_stage};
use super::kind_icon::branch_icon;
use super::pipeline_editor::PipelineEditor;
use super::stage_rows::{DragGhost, DraggedStage, issue_lines};
use crate::app::shortcuts::{MoveStageDown, MoveStageUp};
use crate::components::tab_order;

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

    /// The dialog gives focus back here when the stage picker closes.
    pub fn tab_stops(&self) -> Vec<FocusHandle> {
        self.stops.clone()
    }

    pub fn add_stage_focus(&self) -> FocusHandle {
        self.add_stage.clone()
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

    fn add_branch(&mut self, cx: &mut Context<Self>) {
        let (before, after_id) = {
            let editor = self.editor.read(cx);
            (used_ids(editor.steps()), editor.selected_id())
        };
        self.apply(|steps| add_branch(steps, after_id.as_deref()), None, cx);
        let added = self
            .editor
            .read(cx)
            .steps()
            .iter()
            .find(|step| step.is_branch() && !before.contains(step.id()))
            .map(|step| step.id().to_owned());
        if let Some(id) = added {
            self.editor.update(cx, |editor, cx| editor.select(&id, cx));
        }
    }

    fn header(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let picking = self.editor.read(cx).picking;
        let toggle = self.editor.clone();
        h_flex()
            .gap_2()
            .child(Label::new("Stages in order").weight(gpui::FontWeight::SEMIBOLD))
            .child(div().flex_1())
            .child(
                Button::new("add-branch", "Add branch")
                    .start_icon(branch_icon().size(IconSize::Small))
                    .style(ButtonStyle::Outlined)
                    .label_size(LabelSize::Small)
                    .on_click(cx.listener(|this, _, _, cx| this.add_branch(cx))),
            )
            .child(
                Button::new("add-stage", "Add stage")
                    .start_icon(Icon::from_path("icons/lucide/plus.svg").size(IconSize::Small))
                    .style(ButtonStyle::Outlined)
                    .label_size(LabelSize::Small)
                    .toggle_state(picking)
                    .track_focus(&self.add_stage)
                    .on_click(move |_, _, cx| {
                        toggle.update(cx, |editor, cx| {
                            let picking = editor.picking;
                            editor.set_picking(!picking, cx);
                        });
                    }),
            )
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
        let header = self.header(cx);
        let rows = self.steps(&steps, window, cx);
        self.stops = tab_order::end();
        if let Some(handle) = self
            .refocus
            .take()
            .and_then(|id| self.focuses.get(&id).cloned())
        {
            window.focus(&handle, cx);
        }
        v_flex()
            .id("stage-list")
            .key_context("StageList")
            .on_action(cx.listener(|this, _: &MoveStageUp, window, cx| {
                this.move_focused(-1, window, cx);
            }))
            .on_action(cx.listener(|this, _: &MoveStageDown, window, cx| {
                this.move_focused(1, window, cx);
            }))
            .gap_2()
            .child(header)
            .when(!general.is_empty(), |this| {
                this.child(issue_lines(&general, cx))
            })
            .child(v_flex().id("stage-rows").gap_1().children(rows))
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
