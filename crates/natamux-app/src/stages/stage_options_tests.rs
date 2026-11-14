use gpui::TestAppContext;
use natamux_proto::stages::schema::{Gate, Stage, StageKind, Step};
use ui::prelude::*;

use super::option_rows;
use crate::app::test_views;
use crate::stages::pipeline_editor::PipelineEditor;
use crate::stages::sheet_rules::sheet_shows;

/// How many Options rows a stage alone in the pipeline gets.
fn rows_of(stage: Stage, cx: &mut TestAppContext) -> usize {
    cx.update(|cx| {
        let editor = cx.new(|_| PipelineEditor::new("{}"));
        let steps = vec![Step::Stage(stage.clone())];
        let shows = sheet_shows(&stage, &steps);
        option_rows(&stage, &shows, &editor, cx).len()
    })
}

#[gpui::test]
fn an_agent_stage_has_two_options_and_a_manual_stage_none(cx: &mut TestAppContext) {
    test_views::init(cx);
    assert_eq!(
        rows_of(Stage::new("implement", StageKind::Agent, Gate::Auto), cx),
        2
    );
    assert_eq!(
        rows_of(Stage::new("qa", StageKind::Manual, Gate::Manual), cx),
        0
    );
}

#[test]
fn the_end_agent_switch_has_the_short_label_and_the_rest_in_its_help() {
    use super::{END_AGENT, END_AGENT_HELP};
    assert_eq!(END_AGENT, "End the agent when done");
    assert_eq!(
        END_AGENT_HELP,
        "Closes its terminal and frees the memory of the stage. A next stage in the same \
         terminal starts a new agent session."
    );
}

/// Workflow test T25 (plan 7.5), on main's switch rows (T-0313): a switch row toggles once per
/// click and not when disabled; Enter and Space do what they do today; the stage rows write the
/// draft only when it changes.
mod rows {
    use std::cell::RefCell;
    use std::rc::Rc;

    use gpui::{
        AppContext as _, Entity, Modifiers, Subscription, TestAppContext, VisualTestContext,
    };
    use natamux_proto::stages::schema::{Gate, Stage, StageKind};
    use ui::prelude::*;

    use super::super::super::editor_find::find_stage;
    use super::super::super::pipeline_editor::{DraftEdited, PipelineEditor};
    use super::super::super::stage_fields_view::change;
    use super::super::{Flag, approval_row, flag_row};
    use crate::app::test_driver::press;
    use crate::app::test_views;

    type Log = Rc<RefCell<Vec<bool>>>;

    /// One plain row that logs what its click asks for and changes nothing.
    struct Plain {
        pipeline: Entity<PipelineEditor>,
        stage: Stage,
        on: bool,
        disabled: bool,
        log: Log,
    }

    impl Render for Plain {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let log = self.log.clone();
            let flag = Flag {
                key: "plain",
                label: "Plain row",
                help: None,
                on: self.on,
                default: false,
                changed: false,
                disabled: self.disabled,
            };
            let set = move |steps: &[natamux_proto::stages::schema::Step], _: &str, on: bool| {
                log.borrow_mut().push(on);
                steps.to_vec()
            };
            v_flex()
                .w(px(400.))
                .child(flag_row(flag, &self.stage, &self.pipeline, set, cx))
        }
    }

    fn plain(disabled: bool, cx: &mut TestAppContext) -> (Log, &mut VisualTestContext) {
        let log: Log = Rc::default();
        let shown = log.clone();
        let (_, _, cx) = test_views::init_app(cx, move |_, _, cx| {
            let pipeline = cx.new(|_| PipelineEditor::new("{}"));
            cx.new(|_| Plain {
                pipeline,
                stage: Stage::new("x", StageKind::Agent, Gate::Auto),
                on: false,
                disabled,
                log: shown,
            })
        });
        (log, cx)
    }

    fn click_switch(cx: &mut VisualTestContext, selector: &'static str) {
        let switch = cx.debug_bounds(selector).expect("the switch is drawn");
        cx.simulate_click(switch.center(), Modifiers::none());
        cx.run_until_parked();
    }

    #[gpui::test]
    fn t25_a_click_asks_for_the_other_value_once(cx: &mut TestAppContext) {
        let (log, cx) = plain(false, cx);
        click_switch(cx, "plain-x-switch");
        assert_eq!(*log.borrow(), [true]);
    }

    #[gpui::test]
    fn t25_a_disabled_row_does_nothing(cx: &mut TestAppContext) {
        let (log, cx) = plain(true, cx);
        click_switch(cx, "plain-x-switch");
        press(cx, "enter space");
        assert!(log.borrow().is_empty());
    }

    /// The rows have no focus stop: Enter and Space after a click reach no row.
    #[gpui::test]
    fn t25_enter_and_space_reach_no_row(cx: &mut TestAppContext) {
        let (log, cx) = plain(false, cx);
        click_switch(cx, "plain-x-switch");
        press(cx, "enter");
        press(cx, "space");
        assert_eq!(*log.borrow(), [true]);
    }

    /// The approval row of one stage of a draft, drawn again on each change.
    struct Approval {
        pipeline: Entity<PipelineEditor>,
        id: &'static str,
        _watch: Subscription,
    }

    impl Render for Approval {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let steps = self.pipeline.read(cx).steps().to_vec();
            let stage = find_stage(&steps, self.id).cloned().expect("the stage");
            v_flex()
                .w(px(400.))
                .child(approval_row(&stage, &self.pipeline, cx))
        }
    }

    const SWITCH_IMPLEMENT: &str = "gate-implement-switch";

    const DRAFT: &str = r#"{ "stages": [
        { "id": "implement", "kind": "agent", "gate": "auto" },
        { "id": "review", "kind": "review", "gate": "manual" }
    ] }"#;

    fn approval_of<'a>(
        id: &'static str,
        cx: &'a mut TestAppContext,
    ) -> (
        Entity<PipelineEditor>,
        Rc<RefCell<usize>>,
        &'a mut VisualTestContext,
    ) {
        let writes = Rc::new(RefCell::new(0));
        let counted = writes.clone();
        let (view, _, cx) = test_views::init_app(cx, move |_, _, cx| {
            let pipeline = cx.new(|_| PipelineEditor::new(DRAFT));
            cx.new(|cx| {
                let watch = cx.observe(&pipeline, |_, _, cx| cx.notify());
                Approval {
                    pipeline,
                    id,
                    _watch: watch,
                }
            })
        });
        let pipeline = view.read_with(cx, |view, _| view.pipeline.clone());
        cx.update(|_, cx| {
            cx.subscribe(&pipeline, move |_, _: &DraftEdited, _| {
                *counted.borrow_mut() += 1;
            })
            .detach()
        });
        (pipeline, writes, cx)
    }

    fn gate_of(pipeline: &Entity<PipelineEditor>, id: &str, cx: &mut VisualTestContext) -> String {
        pipeline.read_with(cx, |pipeline, _| {
            let stage = find_stage(pipeline.steps(), id).expect("the stage");
            format!("{:?}", stage.gate)
        })
    }

    #[gpui::test]
    fn t25_a_click_on_the_approval_row_writes_the_draft_once_each_way(cx: &mut TestAppContext) {
        let (pipeline, writes, cx) = approval_of("implement", cx);
        let mut seen = vec![format!("{} after 0", gate_of(&pipeline, "implement", cx))];
        for _ in 0..2 {
            click_switch(cx, SWITCH_IMPLEMENT);
            seen.push(format!(
                "{} after {}",
                gate_of(&pipeline, "implement", cx),
                writes.borrow()
            ));
        }
        assert_eq!(seen, ["Auto after 0", "Manual after 1", "Auto after 2"]);
    }

    #[gpui::test]
    fn t25_a_stage_that_always_waits_has_a_disabled_row(cx: &mut TestAppContext) {
        let (pipeline, writes, cx) = approval_of("review", cx);
        let before = gate_of(&pipeline, "review", cx);
        click_switch(cx, "gate-review-switch");
        assert_eq!(*writes.borrow(), 0);
        assert_eq!(gate_of(&pipeline, "review", cx), before);
    }

    #[gpui::test]
    fn t25_change_writes_only_on_a_difference(cx: &mut TestAppContext) {
        let (pipeline, writes, cx) = approval_of("implement", cx);
        cx.update(|_, cx| change(&pipeline, cx, |steps| steps.to_vec()));
        assert_eq!(*writes.borrow(), 0);
        cx.update(|_, cx| {
            change(&pipeline, cx, |steps| {
                super::super::super::editor_fields::set_enabled(steps, "implement", false)
            })
        });
        assert_eq!(*writes.borrow(), 1);
    }
}
