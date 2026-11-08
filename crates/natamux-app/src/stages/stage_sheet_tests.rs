//! The stage sheet of mockup B (T-0313): the prompt fills the left pane, the column is 296 px,
//! a stage without a prompt shows its run fields at the left, the stepper moves the sheet to
//! another stage, the groups fold, and the Tab ring follows the drawn order.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gpui::{Entity, FocusHandle, Focusable, TestAppContext, VisualTestContext};
use natamux_proto::stages::capabilities::stage_name;
use natamux_proto::stages::schema::{Gate, Stage, StageKind, Step};
use ui::prelude::*;

use super::StageSheet;
use crate::app::test_views;
use crate::components::dialog::DialogView;
use crate::stages::advanced_rules::SheetGroup;
use crate::stages::editor_draft::{RawSettings, format_settings, with_steps};
use crate::stages::editor_find::find_stage;
use crate::stages::editor_fixtures::branched;
use crate::stages::pipeline_editor::PipelineEditor;
use crate::stages::stage_fields_view::LiveField;
use crate::stages::stage_sheet_column::COLUMN_WIDTH;

/// The sheet in a box of the size the dialog layer gives it: 912 x 624 at 1024 x 768.
struct Host {
    sheet: Entity<StageSheet>,
    size: (f32, f32),
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let (width, height) = self.size;
        div().w(px(width)).h(px(height)).child(self.sheet.clone())
    }
}

/// The branched pipeline and a manual stage at its end.
fn steps() -> Vec<Step> {
    let mut steps = branched();
    steps.push(Step::Stage(Stage::new(
        "qa",
        StageKind::Manual,
        Gate::Manual,
    )));
    steps
}

fn open<'a>(
    id: &str,
    cx: &'a mut TestAppContext,
) -> (Entity<StageSheet>, &'a mut VisualTestContext) {
    let (host, cx) = open_in(id, (912., 624.), cx);
    let sheet = host.read_with(cx, |host, _| host.sheet.clone());
    (sheet, cx)
}

fn open_in<'a>(
    id: &str,
    size: (f32, f32),
    cx: &'a mut TestAppContext,
) -> (Entity<Host>, &'a mut VisualTestContext) {
    test_views::init(cx);
    let text = format_settings(&with_steps(&RawSettings::new(), &steps()));
    let id = id.to_owned();
    let (host, cx) = cx.add_window_view(move |window, cx| {
        let pipeline = cx.new(|_| PipelineEditor::new(&text));
        Host {
            sheet: cx.new(|cx| StageSheet::new(pipeline, &id, None, window, cx)),
            size,
        }
    });
    cx.run_until_parked();
    (host, cx)
}

fn assert_prompt_fills(min: f32, cx: &mut VisualTestContext) {
    let prompt = cx
        .debug_bounds("stage-prompt")
        .expect("the prompt was not drawn");
    assert!(
        prompt.size.height >= px(min),
        "the prompt is {:?} high, not at least {min} px",
        prompt.size.height
    );
    let column = cx
        .debug_bounds("stage-sheet-column")
        .expect("the column was not drawn");
    assert!(
        (column.size.width - px(COLUMN_WIDTH)).abs() <= px(0.5),
        "the column is {:?} wide, not {COLUMN_WIDTH} px",
        column.size.width
    );
}

#[gpui::test]
fn the_prompt_of_an_agent_stage_fills_the_left_pane(cx: &mut TestAppContext) {
    let (_, cx) = open("implement", cx);
    assert_prompt_fills(400., cx);
    assert!(cx.debug_bounds("stage-run-card").is_none());
}

#[gpui::test]
fn at_960_by_640_the_prompt_is_taller_and_the_column_keeps_296(cx: &mut TestAppContext) {
    let (_, cx) = open_in("implement", (960., 640.), cx);
    assert_prompt_fills(416., cx);
}

#[gpui::test]
fn a_stage_without_a_prompt_shows_its_run_fields_at_the_left(cx: &mut TestAppContext) {
    let (_, cx) = open("review", cx);
    assert!(cx.debug_bounds("stage-run-card").is_some());
    assert!(cx.debug_bounds("stage-prompt").is_none());
}

#[gpui::test]
fn the_stepper_moves_the_sheet_to_the_next_stage(cx: &mut TestAppContext) {
    let (sheet, cx) = open("implement", cx);
    sheet.update_in(cx, |sheet, window, cx| sheet.go_to("testing", window, cx));
    cx.run_until_parked();
    let (id, name, want) = sheet.read_with(cx, |sheet, cx| {
        let steps = sheet.pipeline.read(cx).steps();
        let want = find_stage(steps, "testing").map(stage_name).unwrap();
        (
            sheet.id.clone(),
            sheet.fields.name.editor.read(cx).text(cx),
            want,
        )
    });
    assert_eq!(id, "testing");
    assert_eq!(name, want);
    // A testing stage: its Command over the prompt.
    assert!(cx.debug_bounds("stage-command").is_some());
    assert!(cx.debug_bounds("stage-prompt").is_some());
    // A stage that is not there leaves the sheet where it is.
    sheet.update_in(cx, |sheet, window, cx| sheet.go_to("gone", window, cx));
    assert_eq!(sheet.read_with(cx, |sheet, _| sheet.id.clone()), "testing");
}

#[gpui::test]
fn done_check_starts_open_and_a_group_folds_on_toggle(cx: &mut TestAppContext) {
    let (sheet, cx) = open("implement", cx);
    assert_eq!(
        sheet.read_with(cx, |sheet, _| sheet.open),
        [true, false, false]
    );
    sheet.update(cx, |sheet, cx| {
        sheet.toggle_group(SheetGroup::Files, cx);
        sheet.toggle_group(SheetGroup::DoneCheck, cx);
    });
    assert_eq!(
        sheet.read_with(cx, |sheet, _| sheet.open),
        [false, true, false]
    );
}

// Regression: in a short sheet the column squeezed the Stage card, which cut off its rows
// (shot project-settings-stage-sheet-testing). The column scrolls; the card keeps its height.
#[gpui::test]
fn a_short_sheet_scrolls_the_column_and_keeps_the_stage_card_whole(cx: &mut TestAppContext) {
    let (host, cx) = open_in("testing", (912., 1200.), cx);
    let card = |cx: &mut VisualTestContext| {
        cx.debug_bounds("stage-card")
            .expect("no Stage card")
            .size
            .height
    };
    let tall = card(cx);
    host.update(cx, |host, cx| {
        host.size = (912., 300.);
        cx.notify();
    });
    cx.run_until_parked();
    let column = cx
        .debug_bounds("stage-sheet-column")
        .expect("no column")
        .size
        .height;
    assert!(
        column < tall,
        "the sheet is not short: column {column:?}, card {tall:?}"
    );
    let short = card(cx);
    assert!(
        (tall - short).abs() <= px(0.5),
        "the Stage card is {short:?} high in a short sheet, {tall:?} in a tall one"
    );
}

/// The left pane and the fold headers of a stage without a prompt to edit.
fn assert_run_card(id: &str, cx: &mut TestAppContext) -> Vec<bool> {
    let (_, cx) = open(id, cx);
    assert!(
        cx.debug_bounds("stage-run-card").is_some(),
        "{id}: no run card"
    );
    assert!(cx.debug_bounds("stage-prompt").is_none(), "{id}: a prompt");
    for field in ["field-Check command", "field-Finished when"] {
        let bounds = cx.debug_bounds(field).expect("a run field is missing");
        let card = cx.debug_bounds("stage-run-card").expect("no run card");
        assert!(
            card.contains(&bounds.origin),
            "{id}: {field} is not in the run card"
        );
    }
    ["stage-fold-0", "stage-fold-1", "stage-fold-2"]
        .into_iter()
        .map(|fold| cx.debug_bounds(fold).is_some())
        .collect()
}

#[gpui::test]
fn a_context_stage_has_its_run_fields_at_the_left_and_no_done_check(cx: &mut TestAppContext) {
    let folds = assert_run_card("context", cx);
    assert!(!folds[0] && !folds[1], "context: {folds:?}");
}

#[gpui::test]
fn a_release_stage_has_its_run_fields_at_the_left_and_no_done_check(cx: &mut TestAppContext) {
    let folds = assert_run_card("release", cx);
    assert!(!folds[0] && !folds[1], "release: {folds:?}");
}

#[gpui::test]
fn a_manual_stage_has_its_run_fields_at_the_left_and_no_groups(cx: &mut TestAppContext) {
    assert_eq!(assert_run_card("qa", cx), [false, false, false]);
}

#[gpui::test]
fn the_tab_ring_follows_the_drawn_order_and_skips_folded_fields(cx: &mut TestAppContext) {
    let (sheet, cx) = open("implement", cx);
    let (ring, want) = sheet.read_with(cx, |sheet, cx| {
        let field = |live: &LiveField| live.editor.focus_handle(cx);
        let fields = &sheet.fields;
        let want = vec![
            sheet.prev_focus.clone(),
            sheet.next_focus.clone(),
            field(&fields.prompt),
            field(&fields.name),
            sheet.fold_focus[0].clone(),
            field(&fields.check),
            field(&fields.done_when),
            sheet.fold_focus[1].clone(),
            sheet.fold_focus[2].clone(),
            sheet.remove_focus.clone(),
            sheet.done_focus.clone(),
        ];
        (sheet.tab_ring(cx), want)
    });
    assert!(ring == want, "the Tab ring is not in drawn order");
    // Files opens: its fields join after its header.
    sheet.update(cx, |sheet, cx| sheet.toggle_group(SheetGroup::Files, cx));
    let (ring, output, header) = sheet.read_with(cx, |sheet, cx| {
        (
            sheet.tab_ring(cx),
            sheet.fields.output.editor.focus_handle(cx),
            sheet.fold_focus[1].clone(),
        )
    });
    let at = |handle: &FocusHandle| ring.iter().position(|stop| stop == handle);
    assert!(at(&output) > at(&header) && at(&header).is_some());
}

#[gpui::test]
fn the_first_stage_has_no_previous_stop_in_the_tab_ring(cx: &mut TestAppContext) {
    let (sheet, cx) = open("context", cx);
    let (ring, prev, next) = sheet.read_with(cx, |sheet, cx| {
        (
            sheet.tab_ring(cx),
            sheet.prev_focus.clone(),
            sheet.next_focus.clone(),
        )
    });
    assert!(!ring.contains(&prev));
    assert!(ring.first() == Some(&next));
}
