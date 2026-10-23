//! The text fields of the picked stage: one editor each, made when the pick changes.

use gpui::Entity;
use natamux_proto::stages::capabilities::stage_name;
use natamux_proto::stages::schema::{Stage, Step};
use ui::prelude::*;

use super::editor_catalog::is_catalog_stage;
use super::editor_fields::{TextField, set_sections, set_text, shown_default_prompt};
use super::editor_find::find_stage;
use super::pipeline_editor::PipelineEditor;
use super::stage_details::DetailsPane;
use super::stage_fields_view::LiveField;
use super::stage_view_rules::finished_when;

/// The text fields of one stage.
pub(super) struct StageFields {
    pub(super) id: String,
    pub(super) name: LiveField,
    pub(super) args: LiveField,
    pub(super) prompt: LiveField,
    pub(super) command: LiveField,
    pub(super) check: LiveField,
    pub(super) output: LiveField,
    pub(super) sections: LiveField,
    pub(super) done_when: LiveField,
}

fn stage_value(stage: &Stage, field: TextField) -> String {
    let text = match field {
        TextField::Name => return stage_name(stage),
        TextField::Prompt => {
            return stage
                .prompt
                .clone()
                .unwrap_or_else(|| shown_default_prompt(stage));
        }
        TextField::Command => &stage.command,
        TextField::Check => &stage.check,
        TextField::Output => &stage.output,
        TextField::DoneWhen => &stage.done_when,
        TextField::Agent => &stage.agent,
        TextField::Args => &stage.args,
    };
    text.clone().unwrap_or_default()
}

impl StageFields {
    pub(super) fn new(
        pipeline: &Entity<PipelineEditor>,
        id: &str,
        window: &mut Window,
        cx: &mut Context<DetailsPane>,
    ) -> Self {
        let text = |field: TextField,
                    placeholder: &str,
                    lines: Option<(usize, usize)>,
                    window: &mut Window,
                    cx: &mut Context<DetailsPane>| {
            let (read_id, write_id) = (id.to_owned(), id.to_owned());
            LiveField::new(
                pipeline,
                placeholder,
                lines,
                move |steps| {
                    find_stage(steps, &read_id)
                        .map_or_else(String::new, |stage| stage_value(stage, field))
                },
                move |steps, text| set_text(steps, &write_id, field, text),
                window,
                cx,
            )
        };
        let sections_id = id.to_owned();
        let read_id = id.to_owned();
        let sections = LiveField::new(
            pipeline,
            "",
            Some((3, 8)),
            move |steps| {
                find_stage(steps, &read_id)
                    .and_then(|stage| stage.sections.as_ref())
                    .map_or_else(String::new, |list| list.join("\n"))
            },
            move |steps, text| {
                set_sections(steps, &sections_id, &text.split('\n').collect::<Vec<_>>())
            },
            window,
            cx,
        );
        let fields = StageFields {
            id: id.to_owned(),
            name: text(TextField::Name, "", None, window, cx),
            args: text(TextField::Args, "--model sonnet", None, window, cx),
            prompt: text(TextField::Prompt, "", Some((9, 24)), window, cx),
            command: text(TextField::Command, "pnpm test", None, window, cx),
            check: text(TextField::Check, "pnpm typecheck", None, window, cx),
            output: text(TextField::Output, "notes.md", None, window, cx),
            sections,
            done_when: text(TextField::DoneWhen, "", None, window, cx),
        };
        let read_only = is_catalog_stage(id);
        fields
            .output
            .editor
            .update(cx, |editor, _| editor.set_read_only(read_only));
        fields
    }

    pub(super) fn sync(&self, steps: &[Step], window: &mut Window, cx: &mut App) {
        for field in [
            &self.name,
            &self.args,
            &self.prompt,
            &self.command,
            &self.check,
            &self.output,
            &self.sections,
            &self.done_when,
        ] {
            field.sync(steps, window, cx);
        }
        // The placeholder is the sentence the stage gets without its own text.
        if let Some(stage) = find_stage(steps, &self.id) {
            let sentence = finished_when(&Stage {
                done_when: None,
                ..stage.clone()
            });
            self.done_when.editor.update(cx, |editor, cx| {
                editor.set_placeholder_text(&sentence, window, cx)
            });
        }
    }
}
