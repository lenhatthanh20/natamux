//! The text fields of the picked branch and its paths: one editor each, made when a path shows.

use gpui::Entity;
use natamux_proto::stages::capabilities::stage_name_of_id;
use natamux_proto::stages::schema::{BranchPath, Condition, Step};
use ui::prelude::*;

use super::editor_branches::{
    ConditionField, edit_condition, set_branch_name, set_path_condition, set_path_name,
};
use super::editor_find::find_branch;
use super::pipeline_editor::PipelineEditor;
use super::stage_details::DetailsPane;
use super::stage_fields_view::LiveField;

fn path_of<'a>(steps: &'a [Step], path_id: &str) -> Option<&'a BranchPath> {
    steps.iter().find_map(|step| match step {
        Step::Branch(branch) => branch.paths.iter().find(|path| path.id == path_id),
        _ => None,
    })
}

fn condition_value(path: Option<&BranchPath>, field: ConditionField) -> String {
    let value = match (path.and_then(|path| path.when.as_ref()), field) {
        (Some(Condition::Output { file, .. }), ConditionField::File) => Some(file.clone()),
        (Some(Condition::Output { section, .. }), ConditionField::Section) => section.clone(),
        (Some(Condition::Output { contains, .. }), ConditionField::Contains) => {
            Some(contains.clone())
        }
        (Some(Condition::Command { run }), ConditionField::Run) => Some(run.clone()),
        _ => None,
    };
    value.unwrap_or_default()
}

/// The text fields of one path. All five exist at once; the card shows the ones of its type.
pub(super) struct PathFields {
    pub(super) id: String,
    pub(super) name: LiveField,
    pub(super) file: LiveField,
    pub(super) section: LiveField,
    pub(super) contains: LiveField,
    pub(super) run: LiveField,
}

impl PathFields {
    fn new(
        pipeline: &Entity<PipelineEditor>,
        branch_id: &str,
        path_id: &str,
        window: &mut Window,
        cx: &mut Context<DetailsPane>,
    ) -> Self {
        let name = {
            let (read_id, branch, id) =
                (path_id.to_owned(), branch_id.to_owned(), path_id.to_owned());
            LiveField::new(
                pipeline,
                &stage_name_of_id(path_id),
                None,
                move |steps| {
                    path_of(steps, &read_id)
                        .and_then(|path| path.name.clone())
                        .unwrap_or_default()
                },
                move |steps, text| set_path_name(steps, &branch, &id, text),
                window,
                cx,
            )
        };
        let mut condition =
            |field: ConditionField, placeholder: &str, cx: &mut Context<DetailsPane>| {
                let (read_id, branch, id) =
                    (path_id.to_owned(), branch_id.to_owned(), path_id.to_owned());
                LiveField::new(
                    pipeline,
                    placeholder,
                    None,
                    move |steps| condition_value(path_of(steps, &read_id), field),
                    move |steps, text| {
                        let Some(path) = path_of(steps, &id) else {
                            return steps.to_vec();
                        };
                        let Some(when) = &path.when else {
                            return steps.to_vec();
                        };
                        let next = edit_condition(when, field, text);
                        set_path_condition(steps, &branch, &id, &next, path.not == Some(true))
                    },
                    window,
                    cx,
                )
            };
        PathFields {
            id: path_id.to_owned(),
            name,
            file: condition(ConditionField::File, "plan.md", cx),
            section: condition(ConditionField::Section, "Size", cx),
            contains: condition(ConditionField::Contains, "small", cx),
            run: condition(ConditionField::Run, "test -f .no-review", cx),
        }
    }

    fn sync(&self, steps: &[Step], window: &mut Window, cx: &mut App) {
        for field in [
            &self.name,
            &self.file,
            &self.section,
            &self.contains,
            &self.run,
        ] {
            field.sync(steps, window, cx);
        }
    }
}

/// The text fields of one branch and its paths.
pub(super) struct BranchFields {
    id: String,
    pub(super) name: LiveField,
    paths: Vec<PathFields>,
}

impl BranchFields {
    pub fn new(
        pipeline: &Entity<PipelineEditor>,
        id: &str,
        window: &mut Window,
        cx: &mut Context<DetailsPane>,
    ) -> Self {
        let (read_id, write_id) = (id.to_owned(), id.to_owned());
        let name = LiveField::new(
            pipeline,
            &stage_name_of_id(id),
            None,
            move |steps| {
                find_branch(steps, &read_id)
                    .and_then(|branch| branch.name.clone())
                    .unwrap_or_default()
            },
            move |steps, text| set_branch_name(steps, &write_id, text),
            window,
            cx,
        );
        let mut fields = BranchFields {
            id: id.to_owned(),
            name,
            paths: Vec::new(),
        };
        let steps = pipeline.read(cx).steps().to_vec();
        fields.sync(pipeline, &steps, window, cx);
        fields
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    /// Fields for the new paths, none for the removed ones, and the draft in the ones without the
    /// focus.
    pub fn sync(
        &mut self,
        pipeline: &Entity<PipelineEditor>,
        steps: &[Step],
        window: &mut Window,
        cx: &mut Context<DetailsPane>,
    ) {
        let Some(branch) = find_branch(steps, &self.id) else {
            return;
        };
        let mut old = std::mem::take(&mut self.paths);
        for path in &branch.paths {
            let kept = old.iter().position(|fields| fields.id == path.id);
            let fields = match kept {
                Some(index) => old.swap_remove(index),
                None => PathFields::new(pipeline, &self.id, &path.id, window, cx),
            };
            self.paths.push(fields);
        }
        self.name.sync(steps, window, cx);
        for fields in &self.paths {
            fields.sync(steps, window, cx);
        }
    }

    pub(super) fn path(&self, id: &str) -> Option<&PathFields> {
        self.paths.iter().find(|fields| fields.id == id)
    }
}
