//! The editor tab of a workspace (`FileEditor.tsx`): the header, the banner for a file that
//! changed on disk, and the body. One editor per task and file; the unsaved text is a draft in
//! the `DraftStore`, so it stays when this view is dropped.

use gpui::{ClickEvent, Entity, FocusHandle, Focusable, Subscription};
use natamux_proto::workspaces::files::WorkspaceFile;
use ui::{Tooltip, prelude::*};

use super::draft_store::DraftStore;
use super::file_drafts::{
    FileDraft, body_notice, can_save, close_question, edit_draft, is_stale, keep_mine,
    revert_question,
};
use super::file_editor_body::{Edited, FileEditorBody};
use super::file_icon_view::file_icon;
use crate::app::dashboard::Dashboard;
use crate::app::shortcuts::SaveFile;
use crate::app::source::reason;
use crate::components::button_style::{action_button, key_hint, quiet_button};
use crate::components::colors::signal;
use crate::components::confirm::Confirm;
use crate::components::skeleton::SkeletonList;
use crate::git::change_rules::split_path;
use crate::terminals::pane_header::icon_button;

pub struct FileEditor {
    pub(super) dashboard: Entity<Dashboard>,
    pub(super) task_id: String,
    pub(super) path: String,
    pub(super) store: Entity<DraftStore>,
    /// The last good read of the file; a failed refresh keeps it.
    pub(super) disk: Option<WorkspaceFile>,
    /// The error of the first read, shown while there is no text.
    error: Option<String>,
    body: Option<Entity<FileEditorBody>>,
    pub(super) saving: bool,
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl FileEditor {
    pub fn new(
        dashboard: Entity<Dashboard>,
        task_id: String,
        path: String,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let store = DraftStore::global(cx);
        let subscriptions = vec![cx.observe(&store, |_, _, cx| cx.notify())];
        let mut editor = FileEditor {
            dashboard,
            task_id,
            path,
            store,
            disk: None,
            error: None,
            body: None,
            saving: false,
            focus: cx.focus_handle(),
            _subscriptions: subscriptions,
        };
        editor.refresh(cx);
        editor
    }

    pub fn task_id(&self) -> &str {
        &self.task_id
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    fn name(&self) -> &str {
        split_path(&self.path).1
    }

    pub(super) fn draft(&self, cx: &App) -> Option<FileDraft> {
        self.store.read(cx).file(&self.task_id, &self.path).cloned()
    }

    /// Reads the file again; a new text goes into the editor when there is no draft.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        let Some(source) = self.dashboard.read(cx).source().cloned() else {
            return;
        };
        let answer = source.read_workspace_file(&self.task_id, &self.path);
        cx.spawn(async move |this, cx| {
            let answer = answer.await;
            this.update(cx, |this, cx| match answer {
                Ok(file) => this.got(file, cx),
                Err(error) if this.disk.is_none() => {
                    this.error = Some(reason(&error));
                    cx.notify();
                }
                Err(_) => {}
            })
            .ok();
        })
        .detach();
    }

    fn got(&mut self, file: WorkspaceFile, cx: &mut Context<Self>) {
        self.error = None;
        let shown = file.text.clone().filter(|_| notice_of(&file).is_none());
        match shown {
            // A draft stays; the banner shows when the version moved.
            Some(text) if self.draft(cx).is_none() => {
                if let Some(body) = &self.body {
                    body.update(cx, |body, cx| body.set_disk_text(&text, cx));
                }
            }
            Some(_) => {}
            None => self.body = None,
        }
        self.disk = Some(file);
        cx.notify();
    }

    /// The body is made when the first text is there (it needs the window).
    fn ensure_body(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.body.is_some() {
            return;
        }
        let Some(file) = self.disk.as_ref().filter(|file| notice_of(file).is_none()) else {
            return;
        };
        let text = self
            .draft(cx)
            .map(|draft| draft.text)
            .or_else(|| file.text.clone())
            .unwrap_or_default();
        let path = self.path.clone();
        let body = cx.new(|cx| FileEditorBody::new(&path, &text, window, cx));
        self._subscriptions
            .push(cx.subscribe(&body, |this, _, edited: &Edited, cx| {
                this.edited(&edited.0, cx)
            }));
        self.body = Some(body);
    }

    /// The text after an edit. A text that came from disk equals the disk text: no draft.
    fn edited(&mut self, text: &str, cx: &mut Context<Self>) {
        let Some(disk) = self.disk.as_ref() else {
            return;
        };
        let draft = edit_draft(
            disk.text.as_deref().unwrap_or_default(),
            &disk.version,
            self.draft(cx).as_ref(),
            text,
        );
        let (task, path) = (self.task_id.clone(), self.path.clone());
        self.store
            .update(cx, |store, cx| store.set_file(&task, &path, draft, cx));
    }

    pub(super) fn stale_now(&self, cx: &App) -> bool {
        is_stale(
            self.draft(cx).as_ref(),
            self.disk.as_ref().map(|file| file.version.as_str()),
        )
    }

    fn set_draft(&mut self, draft: Option<FileDraft>, cx: &mut Context<Self>) {
        let (task, path) = (self.task_id.clone(), self.path.clone());
        self.store
            .update(cx, |store, cx| store.set_file(&task, &path, draft, cx));
    }

    /// Drops the draft and shows the text on disk.
    fn reload(&mut self, cx: &mut Context<Self>) {
        self.set_draft(None, cx);
        let text = self.disk.as_ref().and_then(|file| file.text.clone());
        if let (Some(text), Some(body)) = (text, &self.body) {
            body.update(cx, |body, cx| body.set_disk_text(&text, cx));
        }
    }

    fn keep_mine(&mut self, cx: &mut Context<Self>) {
        let (Some(draft), Some(disk)) = (self.draft(cx), self.disk.as_ref()) else {
            return;
        };
        let kept = keep_mine(&draft, &disk.version);
        self.set_draft(Some(kept), cx);
    }

    fn revert(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let this = cx.entity();
        Confirm::new(
            "Revert changes",
            revert_question(self.name()),
            "Revert",
            move |_, cx| this.update(cx, |this, cx| this.reload(cx)),
        )
        .open(window, cx);
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.draft(cx).is_none() {
            self.close_now(cx);
            return;
        }
        let this = cx.entity();
        Confirm::new(
            "Close the file",
            close_question(self.name()),
            "Discard",
            move |_, cx| this.update(cx, |this, cx| this.close_now(cx)),
        )
        .open(window, cx);
    }

    fn close_now(&mut self, cx: &mut Context<Self>) {
        self.set_draft(None, cx);
        let id = self.task_id.clone();
        self.dashboard
            .update(cx, |dashboard, cx| dashboard.close_file(&id, cx));
    }
}

fn notice_of(file: &WorkspaceFile) -> Option<&'static str> {
    body_notice(file.binary, file.too_large, file.text.is_none())
}

impl Focusable for FileEditor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for FileEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.ensure_body(window, cx);
        let stale = self.stale_now(cx);
        v_flex()
            .id("file-editor")
            .key_context("NatamuxFileEditor")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &SaveFile, _, cx| this.save(cx)))
            .size_full()
            .child(self.render_header(stale, cx))
            .children(stale.then(|| self.render_banner(cx)))
            .child(self.render_body(cx))
    }
}

impl FileEditor {
    fn render_header(&self, stale: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let draft = self.draft(cx);
        let has_draft = draft.is_some();
        let can = can_save(draft.as_ref(), stale, self.saving);
        let colors = cx.theme().colors();
        h_flex()
            .min_h(rems_from_px(40.0_f32))
            .flex_none()
            .flex_wrap()
            .items_center()
            .gap_2()
            .px_3()
            .border_b_1()
            .border_color(colors.border)
            .child(file_icon(&self.path, cx))
            .child(
                div().flex_1().min_w_0().child(
                    Label::new(self.path.clone())
                        .size(LabelSize::Small)
                        .color(Color::Muted)
                        .buffer_font(cx)
                        .truncate(),
                ),
            )
            .children(has_draft.then(|| {
                Label::new("Modified")
                    .size(LabelSize::XSmall)
                    .color(Color::Custom(signal(cx).fg))
            }))
            .child(
                quiet_button("file-revert", "Revert")
                    .label_size(LabelSize::Small)
                    .disabled(!has_draft)
                    .on_click(
                        cx.listener(|this, _: &ClickEvent, window, cx| this.revert(window, cx)),
                    ),
            )
            .child(
                action_button("file-save", "Save")
                    .label_size(LabelSize::Small)
                    .key_binding(key_hint("mod-s", !can))
                    .disabled(!can)
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.save(cx))),
            )
            .child(
                icon_button("file-close", "x")
                    .tooltip(Tooltip::text("Close the file"))
                    .on_click(
                        cx.listener(|this, _: &ClickEvent, window, cx| this.close(window, cx)),
                    ),
            )
    }

    fn render_banner(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        h_flex()
            .flex_none()
            .flex_wrap()
            .items_center()
            .gap_2()
            .px_3()
            .py_1p5()
            .border_b_1()
            .border_color(colors.border)
            .child(
                div().flex_1().min_w_0().child(
                    Label::new("This file changed on disk.")
                        .size(LabelSize::Small)
                        .color(Color::Custom(signal(cx).fg)),
                ),
            )
            .child(
                action_button("file-reload", "Reload")
                    .label_size(LabelSize::Small)
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.reload(cx))),
            )
            .child(
                action_button("file-keep-mine", "Keep mine")
                    .label_size(LabelSize::Small)
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.keep_mine(cx))),
            )
    }

    fn render_body(&self, _: &mut Context<Self>) -> AnyElement {
        if let Some(message) = self.error.as_ref().filter(|_| self.disk.is_none()) {
            return notice(message.clone(), Color::Error);
        }
        let Some(file) = self.disk.as_ref() else {
            return div()
                .flex_1()
                .child(SkeletonList::new(6, 28.))
                .into_any_element();
        };
        if let Some(text) = notice_of(file) {
            return notice(text.to_owned(), Color::Muted);
        }
        match &self.body {
            Some(body) => div()
                .flex_1()
                .min_h_0()
                .w_full()
                .child(body.clone())
                .into_any_element(),
            None => div().flex_1().into_any_element(),
        }
    }
}

fn notice(text: String, color: Color) -> AnyElement {
    div()
        .flex_1()
        .p_4()
        .child(Label::new(text).color(color))
        .into_any_element()
}
