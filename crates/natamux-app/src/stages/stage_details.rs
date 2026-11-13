//! Opens the sheet of what is picked to edit (layout-b 5.3, 5.6): a stage row, a flowchart node
//! or a new stage opens the stage sheet, a branch opens the branch sheet. It keeps the project's
//! skills for the stage sheet. It draws nothing: the sheets are dialogs.

use editor::Editor;
use gpui::{Entity, FocusHandle, Subscription, WeakEntity};
use natamux_proto::projects::repo::SkillList;
use ui::prelude::*;

use super::branch_dialog;
use super::editor_find::{find_branch, find_stage};
use super::pipeline_editor::{OpenSheet, PipelineEditor};
use super::stage_sheet::{self, Skills, StageSheet};

pub struct DetailsPane {
    editor: Entity<PipelineEditor>,
    skills: Skills,
    /// The open stage sheet, which takes the skills when they come late.
    sheet: Option<WeakEntity<StageSheet>>,
    _open: Subscription,
}

impl DetailsPane {
    pub fn new(
        editor: Entity<PipelineEditor>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let open = cx.subscribe_in(&editor, window, |this, _, event: &OpenSheet, window, cx| {
            this.open(&event.0, window, cx);
        });
        DetailsPane {
            editor,
            skills: None,
            sheet: None,
            _open: open,
        }
    }

    /// The prompt box of the open stage sheet, for the whole-app flows.
    pub fn prompt_box(&self, cx: &App) -> Option<Entity<Editor>> {
        let sheet = self.sheet.as_ref()?.upgrade()?;
        Some(sheet.read(cx).fields.prompt.editor.clone())
    }

    /// The open stage sheet, for the flow tests.
    #[cfg(test)]
    pub fn sheet(&self) -> Option<Entity<StageSheet>> {
        self.sheet.as_ref()?.upgrade()
    }

    /// The sheets are dialogs with their own Tab ring: the page has no stops here.
    pub fn tab_stops(&self, _: &App) -> Vec<FocusHandle> {
        Vec::new()
    }

    /// The skill menu is in the stage sheet, which closes it on Esc itself.
    pub fn closes_layer(&mut self, _: &mut Window, _: &mut Context<Self>) -> bool {
        false
    }

    pub fn set_skills(&mut self, skills: Result<SkillList, String>, cx: &mut Context<Self>) {
        self.skills = Some(skills.clone());
        if let Some(sheet) = self.sheet.as_ref().and_then(WeakEntity::upgrade) {
            sheet.update(cx, |sheet, cx| sheet.set_skills(skills, cx));
        }
    }

    fn open(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let (editor, id) = (self.editor.clone(), id.to_owned());
        let steps = editor.read(cx).steps();
        let (branch, stage) = (
            find_branch(steps, &id).is_some(),
            find_stage(steps, &id).is_some(),
        );
        if branch {
            // Opened after the event: the click or the dialog that sent it is still running.
            window.defer(cx, move |window, cx| {
                branch_dialog::open_edit(&editor, &id, window, cx)
            });
            return;
        }
        if !stage {
            return;
        }
        let (skills, this) = (self.skills.clone(), cx.weak_entity());
        window.defer(cx, move |window, cx| {
            let sheet = stage_sheet::open(&editor, &id, skills, window, cx);
            this.update(cx, |this, _| this.sheet = Some(sheet.downgrade()))
                .ok();
        });
    }
}

impl Render for DetailsPane {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}
