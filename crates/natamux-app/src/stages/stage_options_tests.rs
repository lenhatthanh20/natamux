#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gpui::TestAppContext;
use ui::prelude::*;

use super::{Row, render};
use crate::app::test_views;

/// One checkbox row with a note.
struct OptionRow;

impl Render for OptionRow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex().w(px(400.)).child(render(
            Row {
                id: "gate-spec".into(),
                label: "Wait for my approval",
                note: Some("This stage is a check by a person."),
                on: false,
                disabled: false,
                mark: None,
                toggle: Box::new(|_, _, _| {}),
            },
            cx,
        ))
    }
}

#[gpui::test]
fn a_note_starts_under_its_checkbox_label(cx: &mut TestAppContext) {
    // The note started under the box, left of the label (T-0215).
    test_views::init(cx);
    let (_view, cx) = cx.add_window_view(|_, _| OptionRow);
    cx.run_until_parked();
    let [label, note] = ["option-label", "option-note"].map(|selector| {
        cx.debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} was not drawn"))
    });
    assert_eq!(note.left(), label.left());
}

/// Workflow test T25 (plan 7.5): a checkbox row toggles once per click and not when disabled;
/// Enter and Space do what they do today; the stage rows write the draft only when it changes.
mod rows {
    use std::cell::RefCell;
    use std::rc::Rc;

    use gpui::{
        AppContext as _, Entity, Modifiers, Subscription, TestAppContext, VisualTestContext,
    };
    use ui::prelude::*;

    use super::super::super::editor_find::find_stage;
    use super::super::super::pipeline_editor::{DraftEdited, PipelineEditor};
    use super::super::super::stage_fields_view::change;
    use super::super::{Row, approval, render};
    use crate::app::test_driver::press;
    use crate::app::test_views;

    type Log = Rc<RefCell<Vec<bool>>>;

    /// One plain row that logs what its click asks for.
    struct Plain {
        on: bool,
        disabled: bool,
        log: Log,
    }

    impl Render for Plain {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let log = self.log.clone();
            v_flex().w(px(400.)).child(render(
                Row {
                    id: "plain".into(),
                    label: "Plain row",
                    note: None,
                    on: self.on,
                    disabled: self.disabled,
                    mark: None,
                    toggle: Box::new(move |on, _, _| log.borrow_mut().push(on)),
                },
                cx,
            ))
        }
    }

    fn plain(disabled: bool, cx: &mut TestAppContext) -> (Log, &mut VisualTestContext) {
        let log: Log = Rc::default();
        let shown = log.clone();
        let (_, _, cx) = test_views::init_app(cx, move |_, _, cx| {
            cx.new(|_| Plain {
                on: false,
                disabled,
                log: shown,
            })
        });
        (log, cx)
    }

    fn click_label(cx: &mut VisualTestContext) {
        let label = cx.debug_bounds("option-label").expect("the row is drawn");
        cx.simulate_click(label.center(), Modifiers::none());
        cx.run_until_parked();
    }

    #[gpui::test]
    fn t25_a_click_asks_for_the_other_value_once(cx: &mut TestAppContext) {
        let (log, cx) = plain(false, cx);
        click_label(cx);
        assert_eq!(*log.borrow(), [true]);
    }

    #[gpui::test]
    fn t25_a_disabled_row_does_nothing(cx: &mut TestAppContext) {
        let (log, cx) = plain(true, cx);
        click_label(cx);
        press(cx, "enter space");
        assert!(log.borrow().is_empty());
    }

    /// The rows have no focus stop: Enter and Space after a click reach no row.
    #[gpui::test]
    fn t25_enter_and_space_reach_no_row(cx: &mut TestAppContext) {
        let (log, cx) = plain(false, cx);
        click_label(cx);
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
                .child(approval(&stage, &self.pipeline, cx))
        }
    }

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
            click_label(cx);
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
        click_label(cx);
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
