//! Project settings (docs/ui-spec.md 4.31, `ProjectSettingsDialog.tsx`): `.natamux.json` of a
//! project, as a screen of its own. Screen 5 brings the Stages tab only; Issues and JSON come with
//! screen 7. The tab edits one draft text; Save writes it through `PUT /repos/:id/settings` with
//! the version that was read, so a file another client changed shows Reload and Overwrite.

use gpui::{
    DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, ScrollHandle, Subscription,
};
use natamux_proto::projects::repo::{LoadedSettings, SkillList};
use ui::{Tab, TabBar, TabPosition, Tooltip, prelude::*};

use super::settings_text::{Base, base_of, blocked_reason, count_label, saves_unchanged};
use crate::app::dashboard::Dashboard;
use crate::app::source::reason;
use crate::components::confirm::Confirm;
use crate::components::dialog::{
    DialogFrame, DialogLayer, DialogSize, DialogView, cancel_button, primary_button,
};
use crate::stages::editor_draft::same_settings;
use crate::stages::kind_icon::problem_icon;
use crate::stages::pipeline_editor::{DraftEdited, PipelineEditor};
use crate::stages::pipeline_preview::pipeline_preview;
use crate::stages::stage_details::DetailsPane;
use crate::stages::stage_list::StageList;

/// The list and the details sit side by side from this window width (the web's `lg`).
const SIDE_BY_SIDE_FROM: f32 = 1024.;

/// Opens Project settings of `repo_id` on its Stages tab.
pub fn open(
    dashboard: &Entity<Dashboard>,
    repo_id: &str,
    window: &mut Window,
    cx: &mut App,
) -> Entity<ProjectSettingsDialog> {
    let name = dashboard
        .read(cx)
        .data
        .repos
        .iter()
        .find(|repo| repo.id == repo_id)
        .map_or_else(|| repo_id.to_owned(), |repo| repo.name.clone());
    let (dashboard, repo_id) = (dashboard.clone(), repo_id.to_owned());
    let view = cx.new(|cx| ProjectSettingsDialog::new(dashboard, repo_id, name, window, cx));
    DialogLayer::open(view.clone(), window, cx);
    view
}

/// The parts of the Stages tab, made once the settings are read.
pub(super) struct StagesTab {
    pub(super) editor: Entity<PipelineEditor>,
    list: Entity<StageList>,
    pub(super) details: Entity<DetailsPane>,
    /// The picker showed at the last change, so focus moves only when it opens or closes.
    was_picking: bool,
}

pub struct ProjectSettingsDialog {
    pub(super) dashboard: Entity<Dashboard>,
    pub(super) repo_id: String,
    pub(super) repo_name: String,
    /// The file as first read: a later read does not move the version a save checks.
    pub(super) base: Option<Base>,
    read_error: Option<String>,
    pub(super) stages: Option<StagesTab>,
    /// Kept until the tab is made: the skills may be read before the settings.
    skills: Option<Result<SkillList, String>>,
    pub(super) saving: bool,
    pub(super) conflict: bool,
    pub(super) problem: Option<String>,
    body_scroll: ScrollHandle,
    focus: FocusHandle,
    cancel: FocusHandle,
    submit: FocusHandle,
    _reads: Vec<gpui::Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl ProjectSettingsDialog {
    fn new(
        dashboard: Entity<Dashboard>,
        repo_id: String,
        repo_name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut dialog = ProjectSettingsDialog {
            dashboard,
            repo_id,
            repo_name,
            base: None,
            read_error: None,
            stages: None,
            skills: None,
            saving: false,
            conflict: false,
            problem: None,
            body_scroll: ScrollHandle::new(),
            focus: cx.focus_handle(),
            cancel: cx.focus_handle(),
            submit: cx.focus_handle(),
            _reads: Vec::new(),
            _subscriptions: Vec::new(),
        };
        dialog.read(window, cx);
        dialog
    }

    /// Reads the settings and the skills once.
    fn read(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(source) = self.dashboard.read(cx).source().cloned() else {
            return;
        };
        let (settings, skills) = (
            source.load_settings(&self.repo_id),
            source.list_skills(&self.repo_id),
        );
        let settings = cx.spawn_in(window, async move |this, cx| {
            let answer = settings.await;
            this.update_in(cx, |this, window, cx| {
                this.took_settings(answer, window, cx)
            })
            .ok();
        });
        let skills = cx.spawn(async move |this, cx| {
            let answer = skills.await.map_err(|error| reason(&error));
            this.update(cx, |this, cx| match &this.stages {
                Some(stages) => stages
                    .details
                    .update(cx, |details, cx| details.set_skills(answer, cx)),
                None => this.skills = Some(answer),
            })
            .ok();
        });
        self._reads = vec![settings, skills];
    }

    fn took_settings(
        &mut self,
        answer: Result<LoadedSettings, natamux_client::ClientError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match answer {
            Ok(loaded) => {
                let base = base_of(&loaded);
                self.start_tab(&base.text, window, cx);
                self.base = Some(base);
            }
            Err(error) => self.read_error = Some(reason(&error)),
        }
        cx.notify();
    }

    fn start_tab(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let editor = cx.new(|_| PipelineEditor::new(text));
        let list = cx.new(|cx| StageList::new(editor.clone(), cx));
        let details = cx.new(|cx| DetailsPane::new(editor.clone(), window, cx));
        if let Some(skills) = self.skills.take() {
            details.update(cx, |details, cx| details.set_skills(skills, cx));
        }
        self._subscriptions = vec![
            cx.subscribe(&editor, |this, _, _: &DraftEdited, cx| {
                this.problem = None;
                cx.notify();
            }),
            cx.observe(&editor, |_, _, cx| cx.notify()),
            cx.observe_in(&editor, window, Self::follow_picker),
        ];
        self.stages = Some(StagesTab {
            editor,
            list,
            details,
            was_picking: false,
        });
    }

    /// The picker takes the focus to its search when it opens, and gives it back to Add stage
    /// when it closes. Done after the details pane made or dropped the picker.
    fn follow_picker(
        &mut self,
        editor: Entity<PipelineEditor>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let picking = editor.read(cx).picking;
        let Some(stages) = self.stages.as_mut() else {
            return;
        };
        if picking == stages.was_picking {
            return;
        }
        stages.was_picking = picking;
        let (details, list) = (stages.details.clone(), stages.list.clone());
        window.defer(cx, move |window, cx| {
            let focus = if picking {
                details.read(cx).search_focus(cx)
            } else {
                Some(list.read(cx).add_stage_focus())
            };
            if let Some(focus) = focus {
                window.focus(&focus, cx);
            }
        });
    }

    /// The draft of the Stages tab, once the settings are read.
    pub fn pipeline(&self) -> Option<Entity<PipelineEditor>> {
        self.stages.as_ref().map(|stages| stages.editor.clone())
    }

    /// The draft differs from the file as read.
    pub(super) fn dirty(&self, cx: &App) -> bool {
        match (&self.base, &self.stages) {
            (Some(base), Some(stages)) => !same_settings(stages.editor.read(cx).text(), &base.text),
            _ => false,
        }
    }

    /// Save may replace the file without an edit (a file the daemon could not read).
    fn file_broken(&self) -> bool {
        self.base.as_ref().is_some_and(saves_unchanged)
    }

    fn errors(&self, cx: &App) -> usize {
        self.stages
            .as_ref()
            .map_or(0, |stages| stages.editor.read(cx).errors())
    }

    /// Why Save is off; None while it can save.
    pub(super) fn save_blocked(&self, cx: &App) -> Option<&'static str> {
        self.base.as_ref()?;
        blocked_reason(true, self.errors(cx), self.dirty(cx), self.file_broken())
    }

    /// Cancel, the close button and Esc: asks first when there are changes.
    pub(super) fn request_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.saving {
            return;
        }
        if !self.dirty(cx) {
            cx.emit(DismissEvent);
            return;
        }
        let this = cx.weak_entity();
        // Asked after this event is handled: the dialog layer may be busy with it.
        window.defer(cx, move |window, cx| {
            Confirm::new(
                "Discard your changes?",
                "The settings you changed here are not saved.",
                "Discard",
                move |_, cx| {
                    this.update(cx, |_, cx| cx.emit(DismissEvent)).ok();
                },
            )
            .cancel_label("Keep editing")
            .open_over(window, cx);
        });
    }

    fn render_alerts(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let line = |text: String| {
            h_flex()
                .gap_1p5()
                .items_start()
                .child(problem_icon().size(IconSize::XSmall).color(Color::Error))
                .child(Label::new(text).size(LabelSize::Small).color(Color::Error))
        };
        let reading = self.base.is_none() && self.read_error.is_none();
        v_flex()
            .gap_2()
            .when(reading, |this| {
                this.child(Label::new("Reading the settings\u{2026}").color(Color::Muted))
            })
            .children(self.read_error.clone().map(line))
            .children(
                self.base
                    .as_ref()
                    .and_then(|base| base.broken.clone())
                    .map(|broken| {
                        line(format!(
                            "The file has a problem, so the defaults apply: {broken}"
                        ))
                    }),
            )
            .when(self.conflict, |this| this.child(self.conflict_box(cx)))
            .children(self.problem.clone().map(line))
    }

    fn conflict_box(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .flex_wrap()
            .gap_2()
            .px_3()
            .py_2()
            .rounded_lg()
            .border_1()
            .border_color(cx.theme().status().warning_border)
            .bg(cx.theme().status().warning_background)
            .child(
                div().min_w_0().flex_1().child(
                    Label::new(
                        "The file changed on disk since you opened it. Reload drops your changes \
                         here; Overwrite replaces the file with them.",
                    )
                    .color(Color::Default),
                ),
            )
            .child(
                Button::new("settings-reload", "Reload")
                    .style(ButtonStyle::Outlined)
                    .on_click(cx.listener(|this, _, _, cx| this.reload(cx))),
            )
            .child(
                Button::new("settings-overwrite", "Overwrite")
                    .style(ButtonStyle::Tinted(ui::TintColor::Error))
                    .on_click(cx.listener(|this, _, _, cx| this.overwrite(cx))),
            )
    }

    /// The preview, then the list and the details: two parts of the body, so a shot can scroll
    /// to the second.
    fn render_stages(&self, stages: &StagesTab, window: &Window, cx: &mut App) -> [AnyElement; 2] {
        let wide = window.viewport_size().width >= px(SIDE_BY_SIDE_FROM);
        let parts = if wide {
            h_flex()
                .items_start()
                .gap_4()
                .child(div().w(px(352.)).flex_none().child(stages.list.clone()))
                .child(div().min_w_0().flex_1().child(stages.details.clone()))
        } else {
            v_flex()
                .gap_4()
                .child(stages.list.clone())
                .child(stages.details.clone())
        };
        [
            pipeline_preview(&stages.editor, cx),
            parts.into_any_element(),
        ]
    }

    /// Scrolls the body to the stage list and the details (the fourth part, after the alerts,
    /// the tab row and the preview).
    pub fn show_details(&self, cx: &mut Context<Self>) {
        self.body_scroll.scroll_to_top_of_item(3);
        cx.notify();
    }

    fn footer_status(&self, cx: &App) -> impl IntoElement {
        let (errors, warnings) = self.stages.as_ref().map_or((0, 0), |stages| {
            let editor = stages.editor.read(cx);
            (editor.errors(), editor.warnings())
        });
        h_flex()
            .gap_2()
            .when(errors > 0, |this| {
                this.child(
                    Label::new(count_label(errors, "problem"))
                        .size(LabelSize::Small)
                        .color(Color::Error),
                )
            })
            .when(warnings > 0, |this| {
                this.child(
                    Label::new(count_label(warnings, "warning"))
                        .size(LabelSize::Small)
                        .color(Color::Warning),
                )
            })
    }

    fn ghost(
        &self,
        id: &'static str,
        label: &'static str,
        on_click: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> Button {
        Button::new(id, label)
            .style(ButtonStyle::Subtle)
            .disabled(self.base.is_none())
            .on_click(cx.listener(move |this, _, window, cx| on_click(this, window, cx)))
    }
}

impl EventEmitter<DismissEvent> for ProjectSettingsDialog {}

impl Focusable for ProjectSettingsDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl DialogView for ProjectSettingsDialog {
    fn size(&self) -> DialogSize {
        DialogSize::ExtraLarge
    }

    /// The stage rows and the fields on screen, in the order they are drawn, then the footer.
    fn tab_ring(&self, cx: &App) -> Vec<FocusHandle> {
        let mut ring = Vec::new();
        if let Some(stages) = &self.stages {
            ring.extend(stages.list.read(cx).tab_stops());
            ring.extend(stages.details.read(cx).tab_stops(cx));
        }
        ring.extend([self.cancel.clone(), self.submit.clone()]);
        ring
    }

    fn busy(&self, _: &App) -> bool {
        self.saving
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.save(window, cx);
    }

    /// Esc closes the top layer first: the skill menu or the stage picker, then the dialog,
    /// which asks first when there are changes.
    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if let Some(stages) = &self.stages {
            let details = stages.details.clone();
            if details.update(cx, |details, cx| details.closes_layer(window, cx)) {
                return true;
            }
        }
        if self.dirty(cx) {
            self.request_close(window, cx);
            return true;
        }
        false
    }
}

impl Render for ProjectSettingsDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let description = match &self.base {
            Some(base) => format!(
                "Saved in {}. Commit it to share it with your team.",
                base.path
            ),
            None => "The stages, context sources, issue tracker and agent permissions of this \
                     project."
                .to_owned(),
        };
        let blocked = self.save_blocked(cx);
        let close = cx.listener(|this, _, window, cx| this.request_close(window, cx));
        let cancel = cx.listener(|this, _, window, cx| this.request_close(window, cx));
        let save = cx.listener(|this, _, window, cx| this.save(window, cx));
        let save_button = primary_button(
            "Save",
            &self.submit,
            false,
            self.base.is_none() || blocked.is_some() || self.saving,
            save,
        )
        .when_some(blocked, |button, reason| {
            button.tooltip(Tooltip::text(reason))
        });
        let tabs = TabBar::new("project-settings-tabs").child(
            Tab::new("project-settings-stages")
                .position(TabPosition::First)
                .toggle_state(true)
                .child(Label::new("Stages")),
        );
        let alerts = self.render_alerts(cx).into_any_element();
        let mut body = vec![alerts];
        if let Some(stages) = &self.stages {
            body.push(tabs.into_any_element());
            body.extend(self.render_stages(stages, window, cx));
        }
        let reset = self.ghost("settings-reset", "Reset to default", Self::reset, cx);
        let import = self.ghost("settings-import", "Import\u{2026}", Self::import, cx);
        let export = self.ghost("settings-export", "Export", Self::export, cx);
        div().track_focus(&self.focus).child(
            DialogFrame::new(format!("Project settings of {}", self.repo_name), close)
                .tall()
                .description(description)
                .footer_note(
                    "New tasks use this pipeline. Tasks on their way keep their stages, but use \
                     the new prompts and approvals.",
                )
                .track_scroll(&self.body_scroll)
                .children(body)
                .footer(reset)
                .footer(import)
                .footer(export)
                .footer(div().flex_1())
                .footer(self.footer_status(cx))
                .footer(cancel_button("Cancel", &self.cancel, cancel))
                .footer(save_button),
        )
    }
}
