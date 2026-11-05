//! The "Add" link of an empty path in the stage list: a menu of the catalog stages that fit in a
//! path, added at the end of that path.

use gpui::{Entity, Pixels, Point};
use natamux_proto::stages::capabilities::stage_name;
use natamux_proto::stages::catalog::catalog_entry;
use natamux_proto::stages::schema::StageKind;
use ui::prelude::*;

use super::editor_catalog::{add_catalog_stage, catalog_choices};
use super::pipeline_editor::PipelineEditor;
use super::stage_fields_view::{Choice, change, open_choices};

/// Opens the menu at `position`; a pick adds that stage to the path `path_id`.
pub fn open_path_add(
    pipeline: &Entity<PipelineEditor>,
    path_id: &str,
    position: Point<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let ids: Vec<String> = catalog_choices(pipeline.read(cx).steps())
        .into_iter()
        .filter(|entry| !matches!(entry.stage.kind, StageKind::Context | StageKind::Release))
        .map(|entry| entry.stage.id.clone())
        .collect();
    let choices = ids
        .iter()
        .filter_map(|id| catalog_entry(id))
        .map(|entry| {
            let (pipeline, path_id, id) =
                (pipeline.clone(), path_id.to_owned(), entry.stage.id.clone());
            Choice::new(stage_name(&entry.stage), None, move |_, cx| {
                change(&pipeline, cx, |steps| {
                    add_catalog_stage(steps, &id, Some(&path_id))
                })
            })
        })
        .collect();
    open_choices(choices, position, window, cx);
}
