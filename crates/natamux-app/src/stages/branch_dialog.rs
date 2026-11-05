//! The Add branch and Edit branch dialog (docs/ui-spec.md 4.31): the branch name and a card for
//! each path. The dialog edits its own copy of the draft (`local`), so a half-made branch never
//! reaches the draft: Add branch or Save branch writes it, Cancel drops it.

use gpui::{
    DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, ScrollHandle, Subscription,
};
use natamux_proto::stages::schema::{StageBranch, Step};
use ui::{ScrollAxes, prelude::*};

use super::branch_dialog_fields::BranchFields;
use super::branch_dialog_paths::path_cards;
use super::editor_branches::{add_branch, add_branch_with, add_path, replace_branch};
use super::editor_draft::MAX_PATHS;
use super::editor_find::{find_branch, used_ids};
use super::pipeline_editor::PipelineEditor;
use super::stage_fields_view::{change, field_input, field_label, help_line, labelled};
use crate::components::dialog::{
    DialogFrame, DialogLayer, DialogSize, DialogView, cancel_button, primary_button,
};

use crate::components::scrollbars::scroll_area;
use crate::components::tab_order;

/// A new branch after the picked step (before the release when none), from the `+ Add` menu.
pub fn open_add(pipeline: &Entity<PipelineEditor>, window: &mut Window, cx: &mut App) {
    let after = pipeline.read(cx).selected_id();
    let steps = add_branch(pipeline.read(cx).steps(), after.as_deref());
    let before = used_ids(pipeline.read(cx).steps());
    let added = steps
        .iter()
        .find(|step| step.is_branch() && !before.contains(step.id()))
        .map(|step| step.id().to_owned());
    if let Some(id) = added {
        show(pipeline, steps, id, Some(after), window, cx);
    }
}

/// The branch `id` as it is, from the row menu or the branch details.
pub fn open_edit(pipeline: &Entity<PipelineEditor>, id: &str, window: &mut Window, cx: &mut App) {
    let steps = pipeline.read(cx).steps().to_vec();
    if find_branch(&steps, id).is_some() {
        show(pipeline, steps, id.to_owned(), None, window, cx);
    }
}

fn show(
    pipeline: &Entity<PipelineEditor>,
    steps: Vec<Step>,
    branch_id: String,
    adding: Option<Option<String>>,
    window: &mut Window,
    cx: &mut App,
) {
    let text = pipeline.read(cx).text().to_owned();
    let local = cx.new(|_| PipelineEditor::new(&text));
    local.update(cx, |local, cx| local.edit(steps, cx));
    let pipeline = pipeline.clone();
    let dialog = cx.new(|cx| BranchDialog::new(pipeline, local, branch_id, adding, window, cx));
    let first = dialog.read(cx).fields.name.editor.focus_handle(cx);
    DialogLayer::open(dialog, window, cx);
    window.focus(&first, cx);
}

pub struct BranchDialog {
    pipeline: Entity<PipelineEditor>,
    /// The dialog's copy of the draft, with the branch in it.
    local: Entity<PipelineEditor>,
    branch_id: String,
    /// Some(after): a new branch, to go after that step. None: an edit of the branch.
    adding: Option<Option<String>>,
    fields: BranchFields,
    scroll: ScrollHandle,
    /// The fields drawn last, in order, for the Tab ring.
    stops: Vec<FocusHandle>,
    focus: FocusHandle,
    cancel_focus: FocusHandle,
    submit_focus: FocusHandle,
    _watch: Subscription,
}

impl BranchDialog {
    fn new(
        pipeline: Entity<PipelineEditor>,
        local: Entity<PipelineEditor>,
        branch_id: String,
        adding: Option<Option<String>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let fields = BranchFields::new(&local, &branch_id, window, cx);
        let watch = cx.observe_in(&local, window, |this, local, window, cx| {
            let steps = local.read(cx).steps().to_vec();
            this.fields.sync(&local, &steps, window, cx);
            cx.notify();
        });
        BranchDialog {
            pipeline,
            local,
            branch_id,
            adding,
            fields,
            scroll: ScrollHandle::new(),
            stops: Vec::new(),
            focus: cx.focus_handle(),
            cancel_focus: cx.focus_handle(),
            submit_focus: cx.focus_handle(),
            _watch: watch,
        }
    }

    fn branch(&self, cx: &App) -> Option<StageBranch> {
        find_branch(self.local.read(cx).steps(), &self.branch_id).cloned()
    }
}

impl EventEmitter<DismissEvent> for BranchDialog {}

impl Focusable for BranchDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl DialogView for BranchDialog {
    fn size(&self) -> DialogSize {
        DialogSize::Wide
    }

    fn tab_ring(&self, _: &App) -> Vec<FocusHandle> {
        let mut ring = self.stops.clone();
        ring.extend([self.cancel_focus.clone(), self.submit_focus.clone()]);
        ring
    }

    fn submit(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let Some(branch) = self.branch(cx) else {
            return;
        };
        let adding = self.adding.clone();
        change(&self.pipeline, cx, |steps| match &adding {
            Some(after) => add_branch_with(steps, branch.clone(), after.as_deref()),
            None => replace_branch(steps, &branch),
        });
        let id = branch.id.clone();
        self.pipeline
            .update(cx, |pipeline, cx| pipeline.select(&id, cx));
        cx.emit(DismissEvent);
    }
}

impl Render for BranchDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(branch) = self.branch(cx) else {
            return div().track_focus(&self.focus).into_any_element();
        };
        let count = branch.paths.len();
        let (local, branch_id) = (self.local.clone(), branch.id.clone());
        let add = Button::new("add-path", "Add path")
            .style(ButtonStyle::Outlined)
            .start_icon(Icon::new(IconName::Plus).size(IconSize::Small))
            .on_click(move |_, _, cx| change(&local, cx, |steps| add_path(steps, &branch_id)));
        let skip = branch.paths.last().is_some_and(|path| path.when.is_some());
        tab_order::begin();
        let name = labelled(
            "Branch name",
            None,
            None,
            field_input(&self.fields.name.editor, cx),
        );
        let paths_title = div()
            .mt(rems_from_px(10.0_f32))
            .child(field_label("Paths, checked in order"));
        let cards = path_cards(&branch, &self.fields, &self.local, cx);
        self.stops = tab_order::end();
        let adding_new = self.adding.is_some();
        let (title, submit) = match self.adding {
            Some(_) => ("Add a branch", "Add branch"),
            None => ("Edit branch", "Save branch"),
        };
        let body = v_flex()
            .id("branch-dialog-body")
            .max_h(rems_from_px(440.0_f32))
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .child(name)
            .child(paths_title)
            .children(cards)
            .child(
                h_flex()
                    .mt(rems_from_px(8.0_f32))
                    .items_center()
                    .gap(rems_from_px(10.0_f32))
                    .when(count < MAX_PATHS, |this| this.child(add))
                    .when(skip, |this| {
                        this.child(help_line("If no path holds, the task skips this branch."))
                    }),
            );
        let body = scroll_area(body, ScrollAxes::Vertical, &self.scroll, window, cx);
        let frame = DialogFrame::new(title, cx.listener(|_, _, _, cx| cx.emit(DismissEvent)))
            .footer_note(if adding_new {
                "You add stages to the paths after.".to_owned()
            } else {
                "Stages stay in their paths.".to_owned()
            })
            .footer(cancel_button(
                "Cancel",
                &self.cancel_focus,
                cx.listener(|_, _, _, cx| cx.emit(DismissEvent)),
            ))
            .footer(primary_button(
                submit,
                &self.submit_focus,
                false,
                cx.listener(|this, _, window, cx| this.submit(window, cx)),
            ))
            .child(body);
        div()
            .key_context("Dialog")
            .track_focus(&self.focus)
            .child(frame)
            .into_any_element()
    }
}
