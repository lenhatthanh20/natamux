//! Project settings (docs/ui-spec.md 4.31, `ProjectSettingsDialog.tsx`): `.natamux.json` of a
//! project, as a page of Settings (T-0174). Its General, Stages and JSON tabs edit one draft text,
//! which may not parse (then the JSON tab shows); Save writes it through `PUT /repos/:id/settings`
//! with the version that was read, so a file another client changed shows Reload and Overwrite.
//! The draft stays while the person looks at other pages; Discard drops it. The page draws its own
//! header (`settings_header.rs`) and bottom bar (`settings_bar.rs`), so the tabs and Save stay
//! while the body scrolls (layout-b 5).

use gpui::{Entity, EventEmitter, FocusHandle, Focusable, ScrollHandle, Subscription};
use natamux_proto::projects::repo::{LoadedSettings, SkillList};
use ui::{ScrollAxes, prelude::*};

use super::branch_name_field::BranchNameField;
use super::idle_pause_field::IdlePauseField;
use super::repo_trust_dialog::row::RepoTrustRow;
use super::send_backs_field::SendBacksField;
use super::settings_tab_rules::SettingsTab;
use super::settings_tabs::SettingsTabs;
use super::settings_text::{Base, base_of, blocked_reason};
use crate::app::dashboard::Dashboard;
use crate::app::source::reason;
use crate::components::confirm::Confirm;
use crate::components::scrollbars::scroll_area;
use crate::stages::editor_draft::same_settings;
use crate::stages::pipeline_editor::{DraftEdited, PipelineEditor};
use crate::stages::stage_details::DetailsPane;
use crate::stages::stage_list::StageList;
use crate::stages::stages_tab::stages_tab;

/// The content width of the General tab; Stages and JSON are wider.
const CONTENT_MAX: f32 = 760.;
const WIDE_CONTENT_MAX: f32 = 860.;

/// Discard was confirmed: Settings makes the page again from the file.
pub struct Discarded;

/// The parts of the Stages tab, made once the settings are read.
pub(super) struct StagesTab {
    pub(super) editor: Entity<PipelineEditor>,
    list: Entity<StageList>,
    pub(super) details: Entity<DetailsPane>,
    pub(super) tabs: SettingsTabs,
    pub(super) send_backs: Entity<SendBacksField>,
    pub(super) branch_name: Entity<BranchNameField>,
    pub(super) idle_pause: Entity<IdlePauseField>,
}

pub struct ProjectSettingsPage {
    pub(super) dashboard: Entity<Dashboard>,
    pub(super) repo_id: String,
    pub(super) repo_name: String,
    /// The file as first read: a later read does not move the version a save checks.
    pub(super) base: Option<Base>,
    pub(super) read_error: Option<String>,
    pub(super) stages: Option<StagesTab>,
    /// Kept until the tab is made: the skills may be read before the settings.
    skills: Option<Result<SkillList, String>>,
    pub(super) saving: bool,
    pub(super) conflict: bool,
    pub(super) problem: Option<String>,
    pub(super) trust: Entity<RepoTrustRow>,
    body_scroll: ScrollHandle,
    preview_scroll: ScrollHandle,
    focus: FocusHandle,
    pub(super) discard: FocusHandle,
    pub(super) submit: FocusHandle,
    _reads: Vec<gpui::Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl ProjectSettingsPage {
    /// Reads the settings of `repo_id` and its skills; the Stages tab shows first.
    pub fn new(
        dashboard: Entity<Dashboard>,
        repo_id: String,
        repo_name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let trust = cx.new(|cx| RepoTrustRow::new(&dashboard, &repo_id, &repo_name, cx));
        cx.observe(&trust, |_, _, cx| cx.notify()).detach();
        let mut page = ProjectSettingsPage {
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
            trust,
            body_scroll: ScrollHandle::new(),
            preview_scroll: ScrollHandle::new(),
            focus: cx.focus_handle(),
            discard: cx.focus_handle(),
            submit: cx.focus_handle(),
            _reads: Vec::new(),
            _subscriptions: Vec::new(),
        };
        page.read(window, cx);
        page
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
        ];
        let tabs = SettingsTabs::new(&editor, window, cx);
        let send_backs = cx.new(|cx| SendBacksField::new(editor.clone(), window, cx));
        let branch_name = cx.new(|cx| BranchNameField::new(editor.clone(), window, cx));
        let idle_pause = cx.new(|cx| IdlePauseField::new(editor.clone(), window, cx));
        self.stages = Some(StagesTab {
            editor,
            list,
            details,
            tabs,
            send_backs,
            branch_name,
            idle_pause,
        });
    }

    /// The draft of the Stages tab, once the settings are read.
    pub fn pipeline(&self) -> Option<Entity<PipelineEditor>> {
        self.stages.as_ref().map(|stages| stages.editor.clone())
    }

    /// The draft differs from the file as read.
    pub fn dirty(&self, cx: &App) -> bool {
        match (&self.base, &self.stages) {
            (Some(base), Some(stages)) => !same_settings(stages.editor.read(cx).text(), &base.text),
            _ => false,
        }
    }

    /// Save may replace the file without an edit (a file the daemon could not read).
    pub(super) fn file_broken(&self) -> bool {
        self.base.as_ref().is_some_and(|base| base.broken.is_some())
    }

    /// The text of the draft can be read as settings.
    pub(super) fn draft_ok(&self, cx: &App) -> bool {
        self.stages
            .as_ref()
            .is_none_or(|stages| stages.editor.read(cx).parse_error().is_none())
    }

    /// Why Save is off; None while it can save.
    pub(super) fn save_blocked(&self, cx: &App) -> Option<&'static str> {
        self.base.as_ref()?;
        blocked_reason(
            self.draft_ok(cx),
            self.errors(cx),
            self.dirty(cx),
            self.file_broken(),
        )
    }

    /// Discard: asks first, then Settings drops the draft.
    pub(super) fn request_discard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.saving || !self.dirty(cx) {
            return;
        }
        let this = cx.weak_entity();
        // Asked after this event is handled: the click is still being dispatched.
        window.defer(cx, move |window, cx| {
            Confirm::new(
                "Discard your changes?",
                "The settings you changed here are not saved.",
                "Discard",
                move |_, cx| {
                    this.update(cx, |_, cx| cx.emit(Discarded)).ok();
                },
            )
            .cancel_label("Keep editing")
            .sheet()
            .open_over(window, cx);
        });
    }

    /// The Pipeline group, then the Stages group: two parts of the body, so a shot can scroll to
    /// the second.
    pub(super) fn render_stages(
        &self,
        stages: &StagesTab,
        window: &mut Window,
        cx: &mut App,
    ) -> [AnyElement; 2] {
        stages_tab(
            &stages.editor,
            &stages.list,
            &self.preview_scroll,
            window,
            cx,
        )
    }

    /// The sideways scroll of the pipeline preview; `--smoke` reads its range.
    pub fn preview_scroll(&self) -> &ScrollHandle {
        &self.preview_scroll
    }

    /// Scrolls the body to the Stages group (the third part, after the alerts and the Pipeline
    /// group; the header is above the body).
    pub fn show_details(&self, cx: &mut Context<Self>) {
        self.body_scroll.scroll_to_top_of_item(2);
        cx.notify();
    }
}

impl EventEmitter<Discarded> for ProjectSettingsPage {}

impl Focusable for ProjectSettingsPage {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

// What the dialog layer did for the dialog, Settings now asks the page for.
impl ProjectSettingsPage {
    /// The tabs, the rows and fields on screen in the order they are drawn, then the bottom bar.
    pub fn tab_ring(&self, cx: &App) -> Vec<FocusHandle> {
        let mut ring = Vec::new();
        if let Some(stages) = &self.stages {
            let shown = self.shown_tab(cx);
            ring.extend(stages.tabs.stops(shown, cx));
            match shown {
                SettingsTab::General => {
                    ring.push(stages.send_backs.read(cx).stop(cx));
                    ring.push(stages.idle_pause.read(cx).stop(cx));
                    ring.push(stages.branch_name.read(cx).stop(cx));
                    ring.push(stages.tabs.issues.read(cx).stop());
                }
                SettingsTab::Stages => {
                    ring.extend(stages.list.read(cx).tab_stops());
                    ring.extend(stages.details.read(cx).tab_stops(cx));
                }
                SettingsTab::Json => {}
            }
        }
        if self.base.is_some() {
            ring.extend([self.discard.clone(), self.submit.clone()]);
        }
        ring
    }

    /// While saving, Esc and Mod+Enter do nothing.
    pub fn busy(&self) -> bool {
        self.saving
    }

    /// Esc closes the page's own layer first: the skill menu. True: it did.
    pub fn closes_layer(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(details) = self.stages.as_ref().map(|stages| stages.details.clone()) else {
            return false;
        };
        details.update(cx, |details, cx| details.closes_layer(window, cx))
    }
}

impl Render for ProjectSettingsPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let shown = self.shown_tab(cx);
        let header = self.render_header(shown, cx);
        let alerts = self.render_alerts(cx).into_any_element();
        let mut body = vec![alerts];
        if let Some(stages) = &self.stages {
            body.extend(self.render_tab_body(stages, shown, window, cx));
        }
        let wide = self.stages.is_some() && shown != SettingsTab::General;
        let width = rems_from_px(if wide { WIDE_CONTENT_MAX } else { CONTENT_MAX });
        // Each part is centred on its own, so a shot can still scroll to a part by its index.
        let body = body
            .into_iter()
            .map(|part| div().w_full().max_w(width).mx_auto().child(part));
        let bar = self.render_bar(cx);
        v_flex()
            .track_focus(&self.focus)
            .size_full()
            .min_h_0()
            .child(header)
            .child(
                scroll_area(
                    v_flex()
                        .id("project-settings-body")
                        .min_h_0()
                        .flex_1()
                        .overflow_y_scroll()
                        .track_scroll(&self.body_scroll)
                        .pt(rems_from_px(8.0_f32))
                        .px(rems_from_px(32.0_f32))
                        .pb(rems_from_px(32.0_f32))
                        .children(body),
                    ScrollAxes::Vertical,
                    &self.body_scroll,
                    window,
                    cx,
                )
                .flex()
                .flex_col()
                .min_h_0()
                .flex_1(),
            )
            .children(bar)
    }
}
