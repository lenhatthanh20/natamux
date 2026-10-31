//! Workflow test T19 (plan 7.5), in the app's root view on the fake daemon: the edit form opens
//! with the title focused and selected, the panel counts as editing while it is open, Mod+Enter
//! saves from every field, a blank title blocks the save, the priority menu picks what is saved,
//! and Tab and Esc do what they do today (D34).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use editor::Editor;
use gpui::{Entity, Focusable, TestAppContext, VisualTestContext};
use natamux_proto::tasks::task::Task;

use super::EditForm;
use crate::app::dashboard::Dashboard;
use crate::app::test_driver::{activate, click_on, is_focused, press, selection_of, text_of};
use crate::app::window::flow_tests::{REVIEWED, not_editing, parts, root};
use crate::components::menu::MenuLayer;
use crate::tasks::details_panel::DetailsPanel;

/// The names of the form's stops, in the order of `boxes` and then the priority button.
const FIELDS: [&str; 5] = ["title", "notes", "labels", "depends on", "priority"];

fn reviewed(dashboard: &Entity<Dashboard>, cx: &mut VisualTestContext) -> Task {
    dashboard.read_with(cx, |dashboard, _| {
        dashboard.data.task(REVIEWED).cloned().expect("a task")
    })
}

/// Opens the form as the pencil and `e` do: on the panel's next draw.
fn open_form(details: &Entity<DetailsPanel>, cx: &mut VisualTestContext) -> Entity<EditForm> {
    details.update(cx, |details, cx| details.start_edit(cx));
    cx.run_until_parked();
    let form = details.read_with(cx, |details, _| details.edit.clone());
    form.expect("the edit form did not open")
}

fn is_open(details: &Entity<DetailsPanel>, cx: &mut VisualTestContext) -> bool {
    details.read_with(cx, |details, _| details.edit.is_some())
}

fn title_box(form: &Entity<EditForm>, cx: &mut VisualTestContext) -> Entity<Editor> {
    form.read_with(cx, |form, _| form.title.clone())
}

/// The handle of each stop, named as in `FIELDS`.
fn stops(form: &Entity<EditForm>, cx: &mut VisualTestContext) -> Vec<gpui::FocusHandle> {
    form.read_with(cx, |form, cx| {
        let mut handles: Vec<_> = form
            .boxes()
            .iter()
            .map(|text_box| text_box.focus_handle(cx))
            .collect();
        handles.push(form.priority_focus.clone());
        handles
    })
}

/// The name of the stop that has the focus, or where else it is.
fn focused_stop(
    form: &Entity<EditForm>,
    details: &Entity<DetailsPanel>,
    cx: &mut VisualTestContext,
) -> String {
    let handles = stops(form, cx);
    let panel = details.read_with(cx, |details, cx| details.focus_handle(cx));
    cx.update(|window, cx| {
        if let Some(at) = handles.iter().position(|h| h.is_focused(window)) {
            return FIELDS[at].to_string();
        }
        if panel.contains_focused(window, cx) {
            "in the panel".into()
        } else {
            "outside the panel".into()
        }
    })
}

fn focus(handle: &gpui::FocusHandle, cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.focus(handle, cx));
    cx.run_until_parked();
}

fn retitle(form: &Entity<EditForm>, title: &str, cx: &mut VisualTestContext) {
    let title_box = title_box(form, cx);
    title_box.update_in(cx, |editor, window, cx| editor.set_text(title, window, cx));
    cx.run_until_parked();
}

#[gpui::test]
fn t19_the_title_is_focused_and_selected_on_open(cx: &mut TestAppContext) {
    let (root, dashboard, cx) = root(cx);
    let details = parts(&root, cx).details;
    let form = open_form(&details, cx);
    let title = title_box(&form, cx);
    assert!(is_focused(&title, cx));
    let text = reviewed(&dashboard, cx).title;
    assert_eq!(text_of(&title, cx), text);
    assert_eq!(selection_of(&title, cx), 0..text.encode_utf16().count());
}

#[gpui::test]
fn t19_the_panel_is_editing_while_the_form_is_open(cx: &mut TestAppContext) {
    let (root, _, cx) = root(cx);
    let details = parts(&root, cx).details;
    let form = open_form(&details, cx);
    let panel = details.read_with(cx, |details, cx| details.focus_handle(cx));
    let mut seen = vec![format!("title {:?}", not_editing(cx))];
    // Out of the boxes, but the form is still open.
    focus(&panel, cx);
    seen.push(format!("panel, form open {:?}", not_editing(cx)));
    let priority = form.read_with(cx, |form, _| form.priority_focus.clone());
    focus(&priority, cx);
    seen.push(format!("priority {:?}", not_editing(cx)));
    // Cancel emits Saved, which closes the form.
    form.update(cx, |_, cx| cx.emit(super::Saved));
    cx.run_until_parked();
    assert!(!is_open(&details, cx));
    focus(&panel, cx);
    seen.push(format!("panel, form closed {:?}", not_editing(cx)));
    assert_eq!(
        seen,
        [
            "title Some(false)",
            "panel, form open Some(false)",
            "priority Some(false)",
            "panel, form closed Some(true)",
        ]
    );
}

#[gpui::test]
fn t19_mod_enter_saves_from_every_field(cx: &mut TestAppContext) {
    let (root, dashboard, cx) = root(cx);
    let details = parts(&root, cx).details;
    let mut wrong = Vec::new();
    for (at, name) in FIELDS.into_iter().enumerate() {
        let form = open_form(&details, cx);
        let title = format!("Saved from {name}");
        retitle(&form, &title, cx);
        let handle = stops(&form, cx)[at].clone();
        focus(&handle, cx);
        press(cx, "mod-enter");
        let saved = reviewed(&dashboard, cx).title;
        let open = is_open(&details, cx);
        if saved != title || open {
            wrong.push(format!("{name}: title {saved:?}, form open {open}"));
        }
    }
    assert!(wrong.is_empty(), "\n{}", wrong.join("\n"));
}

#[gpui::test]
fn t19_a_blank_title_blocks_the_save(cx: &mut TestAppContext) {
    let (root, dashboard, cx) = root(cx);
    let details = parts(&root, cx).details;
    let before = reviewed(&dashboard, cx).title;
    let form = open_form(&details, cx);
    retitle(&form, "   ", cx);
    let notes = form.read_with(cx, |form, cx| form.notes.focus_handle(cx));
    focus(&notes, cx);
    press(cx, "mod-enter");
    assert!(is_open(&details, cx));
    assert!(!form.read_with(cx, |form, _| form.busy));
    assert_eq!(reviewed(&dashboard, cx).title, before);
}

#[gpui::test]
fn t19_the_priority_menu_picks_what_is_saved(cx: &mut TestAppContext) {
    let (root, dashboard, cx) = root(cx);
    let details = parts(&root, cx).details;
    let form = open_form(&details, cx);
    let priority = form.read_with(cx, |form, _| form.priority_focus.clone());
    focus(&priority, cx);
    activate(cx);
    cx.run_until_parked();
    assert!(cx.update(|_, cx| MenuLayer::open_menu(cx)).is_some());
    click_on(cx, "MENU_ITEM-Urgent");
    cx.run_until_parked();
    assert!(cx.update(|_, cx| MenuLayer::open_menu(cx)).is_none());
    assert_eq!(
        form.read_with(cx, |form, _| form.priority.clone()),
        "urgent"
    );
    // The menu gave the focus back to the button, so Mod+Enter still saves.
    assert!(cx.update(|window, _| priority.is_focused(window)));
    press(cx, "mod-enter");
    assert!(!is_open(&details, cx));
    assert_eq!(reviewed(&dashboard, cx).priority, "urgent");
}

/// Tab in each stop, and Esc in the title, as they are today (D34 keeps them): a box takes the
/// Tab as text (the one-line boxes too, see "Bugs found"), the button keeps the focus, and Esc
/// leaves the form open.
#[gpui::test]
fn t19_tab_and_esc_do_what_they_do_today(cx: &mut TestAppContext) {
    let (root, dashboard, cx) = root(cx);
    let details = parts(&root, cx).details;
    let mut seen = Vec::new();
    for (at, name) in FIELDS.into_iter().enumerate() {
        let form = open_form(&details, cx);
        let handles = stops(&form, cx);
        let boxes = form.read_with(cx, |form, _| form.boxes());
        if let Some(text_box) = boxes.get(at) {
            text_box.update_in(cx, |editor, window, cx| editor.set_text("x", window, cx));
        }
        focus(&handles[at], cx);
        if at < boxes.len() {
            press(cx, "end");
        }
        press(cx, "tab");
        let texts: Vec<String> = boxes.iter().map(|b| text_of(b, cx)).collect();
        seen.push(format!(
            "{name}: focus {}, texts {texts:?}",
            focused_stop(&form, &details, cx)
        ));
        form.update(cx, |_, cx| cx.emit(super::Saved));
        cx.run_until_parked();
    }
    let form = open_form(&details, cx);
    let title = reviewed(&dashboard, cx).title;
    press(cx, "escape");
    seen.push(format!(
        "escape: form open {}, focus {}, title kept {}",
        is_open(&details, cx),
        focused_stop(&form, &details, cx),
        reviewed(&dashboard, cx).title == title
    ));
    assert_eq!(
        seen,
        [
            "title: focus title, texts [\"x\\t\", \"A coupon larger than the cart total gives a negative amount at checkout.\", \"bug\", \"\"]",
            "notes: focus notes, texts [\"Rust migration phase 1\", \"x\\t\", \"bug\", \"\"]",
            "labels: focus labels, texts [\"Rust migration phase 1\", \"A coupon larger than the cart total gives a negative amount at checkout.\", \"x\\t\", \"\"]",
            "depends on: focus depends on, texts [\"Rust migration phase 1\", \"A coupon larger than the cart total gives a negative amount at checkout.\", \"bug\", \"x\\t\"]",
            "priority: focus priority, texts [\"Rust migration phase 1\", \"A coupon larger than the cart total gives a negative amount at checkout.\", \"bug\", \"\"]",
            "escape: form open true, focus title, title kept true",
        ]
    );
}
