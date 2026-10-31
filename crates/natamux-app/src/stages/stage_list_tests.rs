//! Workflow test T26 (plan 7.5), in the Project settings page of the fake's project: Alt+Up and
//! Alt+Down move the focused row and keep the focus on it, a drop puts the row where the line
//! shows, the stage and branch menus move, split and remove (a remove that loses settings asks
//! first), and the checkbox turns a stage off and on. The checkbox and the drag have no key, so
//! the tests call what the click and the drop call.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{Entity, TestAppContext, VisualTestContext, point, px};

use super::super::editor_fields::set_enabled;
use super::super::editor_find::find_stage;
use super::super::editor_moves::Side;
use super::super::pipeline_editor::{DraftEdited, PipelineEditor};
use super::super::stage_rows::DraggedStage;
use super::StageList;
use crate::app::test_driver::{click_on, press};
use crate::app::test_views;
use crate::components::dialog::DialogLayer;
use crate::components::menu::MenuLayer;

struct Page<'a> {
    pipeline: Entity<PipelineEditor>,
    list: Entity<StageList>,
    writes: Rc<RefCell<usize>>,
    cx: &'a mut VisualTestContext,
}

fn page(cx: &mut TestAppContext) -> Page<'_> {
    let (page, _, cx) = test_views::init_project_page(cx);
    let pipeline = page.read_with(cx, |page, _| page.pipeline().expect("read"));
    let (_, list) = page.read_with(cx, |page, _| page.stage_parts().expect("read"));
    let writes = Rc::new(RefCell::new(0));
    let counted = writes.clone();
    cx.update(|_, cx| {
        cx.subscribe(&pipeline, move |_, _: &DraftEdited, _| {
            *counted.borrow_mut() += 1;
        })
        .detach()
    });
    Page {
        pipeline,
        list,
        writes,
        cx,
    }
}

/// The top-level steps, a group or branch with the ids it holds in brackets.
fn order(pipeline: &Entity<PipelineEditor>, cx: &mut VisualTestContext) -> String {
    pipeline.read_with(cx, |pipeline, _| {
        let steps = pipeline.steps().iter().map(|step| {
            let inner: Vec<&str> = step
                .stages()
                .into_iter()
                .map(|stage| stage.id.as_str())
                .collect();
            if inner.len() == 1 && inner[0] == step.id() {
                step.id().to_string()
            } else {
                format!("{}[{}]", step.id(), inner.join(" "))
            }
        });
        steps.collect::<Vec<_>>().join(" ")
    })
}

fn focused_row(list: &Entity<StageList>, cx: &mut VisualTestContext) -> Option<String> {
    let rows = list.read_with(cx, |list, _| list.row_focuses());
    cx.update(|window, _| {
        rows.into_iter()
            .find(|(_, handle)| handle.is_focused(window))
            .map(|(id, _)| id)
    })
}

fn focus_row(list: &Entity<StageList>, id: &str, cx: &mut VisualTestContext) {
    let rows = list.read_with(cx, |list, _| list.row_focuses());
    let (_, handle) = rows.into_iter().find(|(row, _)| row == id).expect("a row");
    cx.update(|window, cx| window.focus(&handle, cx));
    cx.run_until_parked();
}

const START: &str = "context spec kind[reproduce plan] build[test-spec implement] agent-review \
                     security review release";

#[gpui::test]
fn t26_the_fake_project_has_the_reference_pipeline(cx: &mut TestAppContext) {
    let page = page(cx);
    assert_eq!(order(&page.pipeline, page.cx), START);
}

#[gpui::test]
fn t26_alt_up_and_alt_down_move_the_focused_row_and_keep_the_focus(cx: &mut TestAppContext) {
    let page = page(cx);
    let cx = page.cx;
    focus_row(&page.list, "security", cx);
    let mut seen = Vec::new();
    for key in ["alt-up", "alt-up", "alt-down", "alt-down"] {
        press(cx, key);
        seen.push(format!(
            "{key}: {} | focus {:?}",
            order(&page.pipeline, cx),
            focused_row(&page.list, cx)
        ));
    }
    assert_eq!(
        seen,
        [
            "alt-up: context spec kind[reproduce plan] build[test-spec implement] security \
             agent-review review release | focus Some(\"security\")",
            "alt-up: context spec kind[reproduce plan] security build[test-spec implement] \
             agent-review review release | focus Some(\"security\")",
            "alt-down: context spec kind[reproduce plan] build[test-spec implement] security \
             agent-review review release | focus Some(\"security\")",
            "alt-down: context spec kind[reproduce plan] build[test-spec implement] \
             agent-review security review release | focus Some(\"security\")",
        ]
    );
    assert_eq!(*page.writes.borrow(), 4);
}

#[gpui::test]
fn t26_a_drop_puts_the_row_where_the_line_shows(cx: &mut TestAppContext) {
    let page = page(cx);
    let cx = page.cx;
    page.list.update(cx, |list, cx| {
        list.target = Some(("context".into(), Side::Before));
        list.dropped("context", &DraggedStage("release".into()), cx);
    });
    cx.run_until_parked();
    let mut seen = vec![format!(
        "{} | focus {:?}",
        order(&page.pipeline, cx),
        focused_row(&page.list, cx)
    )];
    // With no line shown, a drop goes after the row; a row dropped on itself stays.
    page.list.update(cx, |list, cx| {
        list.dropped("security", &DraggedStage("context".into()), cx);
        list.dropped("spec", &DraggedStage("spec".into()), cx);
    });
    cx.run_until_parked();
    seen.push(format!(
        "{} | writes {}",
        order(&page.pipeline, cx),
        page.writes.borrow()
    ));
    assert_eq!(
        seen,
        [
            "release context spec kind[reproduce plan] build[test-spec implement] agent-review \
             security review | focus Some(\"release\")",
            "release spec kind[reproduce plan] build[test-spec implement] agent-review security \
             context review | writes 2",
        ]
    );
}

fn stage_menu(list: &Entity<StageList>, id: &str, cx: &mut VisualTestContext) {
    list.update_in(cx, |list, window, cx| {
        list.open_stage_menu(id, point(px(200.), px(200.)), window, cx)
    });
    cx.run_until_parked();
    assert!(cx.update(|_, cx| MenuLayer::open_menu(cx)).is_some());
}

#[gpui::test]
fn t26_the_stage_menu_moves_splits_joins_and_removes(cx: &mut TestAppContext) {
    let page = page(cx);
    let cx = page.cx;
    let mut seen = Vec::new();
    // The first row cannot move up: the item is off and its click does nothing.
    stage_menu(&page.list, "context", cx);
    click_on(cx, "MENU_ITEM-Move up");
    seen.push(format!("context up: {}", order(&page.pipeline, cx)));
    press(cx, "escape");
    stage_menu(&page.list, "security", cx);
    click_on(cx, "MENU_ITEM-Move up");
    seen.push(format!("security up: {}", order(&page.pipeline, cx)));
    stage_menu(&page.list, "test-spec", cx);
    click_on(cx, "MENU_ITEM-Run on its own");
    seen.push(format!("test-spec alone: {}", order(&page.pipeline, cx)));
    stage_menu(&page.list, "test-spec", cx);
    click_on(cx, "MENU_ITEM-Run alongside the stage above");
    seen.push(format!(
        "test-spec alongside: {}",
        order(&page.pipeline, cx)
    ));
    // Remove loses the stage's prompt and settings, so a confirm asks first.
    stage_menu(&page.list, "agent-review", cx);
    click_on(cx, "MENU_ITEM-Remove");
    seen.push(format!(
        "remove asks: {}, {}",
        cx.update(|_, cx| DialogLayer::is_open(cx)),
        order(&page.pipeline, cx)
    ));
    press(cx, "mod-enter");
    seen.push(format!("removed: {}", order(&page.pipeline, cx)));
    assert_eq!(
        seen,
        [
            format!("context up: {START}"),
            "security up: context spec kind[reproduce plan] build[test-spec implement] security \
             agent-review review release"
                .to_string(),
            "test-spec alone: context spec kind[reproduce plan] implement test-spec security \
             agent-review review release"
                .to_string(),
            "test-spec alongside: context spec kind[reproduce plan] \
             implement-group[implement test-spec] security agent-review review release"
                .to_string(),
            "remove asks: true, context spec kind[reproduce plan] implement-group[implement test-spec] \
             security agent-review review release"
                .to_string(),
            "removed: context spec kind[reproduce plan] implement-group[implement test-spec] security \
             review release"
                .to_string(),
        ]
    );
}

#[gpui::test]
fn t26_the_branch_menu_removes_the_branch_after_a_confirm(cx: &mut TestAppContext) {
    let page = page(cx);
    let cx = page.cx;
    page.list.update_in(cx, |list, window, cx| {
        list.open_branch_menu("kind", point(px(200.), px(200.)), window, cx)
    });
    cx.run_until_parked();
    click_on(cx, "MENU_ITEM-Remove branch");
    assert!(cx.update(|_, cx| DialogLayer::is_open(cx)));
    assert_eq!(order(&page.pipeline, cx), START);
    // Esc keeps it; the second time the confirm removes it.
    press(cx, "escape");
    assert_eq!(order(&page.pipeline, cx), START);
    page.list.update_in(cx, |list, window, cx| {
        list.open_branch_menu("kind", point(px(200.), px(200.)), window, cx)
    });
    cx.run_until_parked();
    click_on(cx, "MENU_ITEM-Remove branch");
    press(cx, "mod-enter");
    assert_eq!(
        order(&page.pipeline, cx),
        "context spec build[test-spec implement] agent-review security review release"
    );
}

#[gpui::test]
fn t26_the_checkbox_turns_a_stage_off_and_on(cx: &mut TestAppContext) {
    let page = page(cx);
    let cx = page.cx;
    let on = |pipeline: &Entity<PipelineEditor>, cx: &mut VisualTestContext| {
        pipeline.read_with(cx, |pipeline, _| {
            find_stage(pipeline.steps(), "security")
                .expect("a stage")
                .is_on()
        })
    };
    let mut seen = vec![on(&page.pipeline, cx)];
    for _ in 0..2 {
        // What the row's checkbox click does with the value it shows.
        let now = on(&page.pipeline, cx);
        page.list.update(cx, |list, cx| {
            list.apply(|steps| set_enabled(steps, "security", !now), None, cx)
        });
        cx.run_until_parked();
        seen.push(on(&page.pipeline, cx));
    }
    assert_eq!(seen, [true, false, true]);
    assert_eq!(*page.writes.borrow(), 2);
}
