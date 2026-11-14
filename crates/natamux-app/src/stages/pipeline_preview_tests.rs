//! The preview box is not a second vertical scroller inside the settings page: it grows to show
//! every branch note, and keeps its 196 px for a short pipeline (T-0293).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gpui::{Bounds, Entity, Pixels, ScrollHandle, TestAppContext};
use natamux_proto::stages::schema::{Gate, StageKind, Step};
use ui::prelude::*;

use super::{FLOW_HEIGHT, pipeline_preview};
use crate::app::test_views;
use crate::stages::editor_draft::{RawSettings, format_settings, with_steps};
use crate::stages::editor_fixtures::{branched, stage_step};
use crate::stages::pipeline_editor::PipelineEditor;

struct Host {
    editor: Entity<PipelineEditor>,
    scroll: ScrollHandle,
}

impl Render for Host {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(pipeline_preview(&self.editor, &self.scroll, window, cx))
    }
}

/// The bounds of the box, and of the notes of the branch `after-plan` when it is drawn.
fn draw(steps: Vec<Step>, cx: &mut TestAppContext) -> (Bounds<Pixels>, Option<Bounds<Pixels>>) {
    test_views::init(cx);
    let text = format_settings(&with_steps(&RawSettings::new(), &steps));
    let (_host, cx) = cx.add_window_view(move |_, cx| Host {
        editor: cx.new(|_| PipelineEditor::new(&text)),
        scroll: ScrollHandle::new(),
    });
    cx.run_until_parked();
    let frame = cx
        .debug_bounds("pipeline-preview")
        .expect("the preview box was not drawn");
    (frame, cx.debug_bounds("preview-notes-after-plan"))
}

#[gpui::test]
fn the_box_grows_to_show_every_branch_note(cx: &mut TestAppContext) {
    let (frame, note) = draw(branched(), cx);
    let note = note.expect("the branch notes were not drawn");
    assert!(
        note.bottom() <= frame.bottom() + px(0.5),
        "the notes end at {:?}, under the box bottom {:?}: they are cut off",
        note.bottom(),
        frame.bottom()
    );
}

#[gpui::test]
fn a_short_pipeline_keeps_the_box_height(cx: &mut TestAppContext) {
    let steps = vec![stage_step("plan", StageKind::Agent, Gate::Manual)];
    let (frame, _) = draw(steps, cx);
    assert!(
        (frame.size.height - px(FLOW_HEIGHT)).abs() <= px(0.5),
        "the box is {:?} high, not {FLOW_HEIGHT} px",
        frame.size.height
    );
}
