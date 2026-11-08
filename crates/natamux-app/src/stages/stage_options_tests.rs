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
