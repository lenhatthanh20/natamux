//! The Docs tab of a workspace (`DocsTab.tsx`): the stage files and attachments of the task, the
//! picked file as Markdown or source, and a plain edit with a draft that outlives the tab.
//! While the details panel shows Docs, the panel draws the list and main only the picked file.

use editor::{Editor, EditorEvent};
use gpui::{Div, Entity, FocusHandle, Focusable, KeyContext, Subscription};
use natamux_proto::stages::actions::StageFile;
use natamux_proto::tasks::attachments::Attachment;
use ui::prelude::*;

use super::docs_markdown::MarkdownDoc;
use super::docs_rules::{DocDraft, current_doc};
use super::docs_view::plain;
use crate::app::dashboard::Dashboard;
use crate::app::source::reason;
use crate::app::view_rules::DetailsView;
use crate::files::draft_store::DraftStore;

/// The text of a file as read, with the `modified_at` of the list it was read for.
pub(super) struct ShownText {
    pub name: String,
    pub modified_at: String,
    pub text: String,
}

/// The edit box of the file that has a draft.
pub(super) struct EditBox {
    pub name: String,
    pub editor: Entity<Editor>,
    _edits: Subscription,
}

pub struct DocsTab {
    pub(super) dashboard: Entity<Dashboard>,
    pub(super) task_id: String,
    focus: FocusHandle,
    pub(super) view_focus: FocusHandle,
    pub(super) picked: Option<String>,
    pub(super) start_new: bool,
    pub(super) list: Option<Vec<StageFile>>,
    list_error: Option<String>,
    pub(super) attachments: Vec<Attachment>,
    pub(super) shown: Option<ShownText>,
    pub(super) text_error: Option<(String, String)>,
    reading: Option<(String, String)>,
    pub(super) image: Option<super::docs_list::ShownAttachment>,
    pub(super) markdown: MarkdownDoc,
    pub(super) source: Option<(Entity<Editor>, String)>,
    pub(super) edit: Option<EditBox>,
    pub(super) focus_edit: bool,
    pub(super) saving: bool,
    pub(super) store: Entity<DraftStore>,
    _store: Subscription,
}

impl DocsTab {
    pub fn new(
        dashboard: Entity<Dashboard>,
        task_id: String,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let store = DraftStore::global(cx);
        let observe = cx.observe(&store, |_, _, cx| cx.notify());
        let tab = Self {
            dashboard,
            task_id,
            focus: cx.focus_handle(),
            view_focus: cx.focus_handle().tab_stop(true),
            picked: None,
            start_new: false,
            list: None,
            list_error: None,
            attachments: Vec::new(),
            shown: None,
            text_error: None,
            reading: None,
            image: None,
            markdown: MarkdownDoc::new(cx),
            source: None,
            edit: None,
            focus_edit: false,
            saving: false,
            store,
            _store: observe,
        };
        tab.read_list(cx);
        tab.read_attachments(cx);
        tab
    }

    pub fn task_id(&self) -> &str {
        &self.task_id
    }

    /// The box of the Source view, once it showed, for the `--shot` states.
    pub fn source_editor(&self) -> Option<Entity<Editor>> {
        self.source.as_ref().map(|(editor, _)| editor.clone())
    }

    /// A stage card chip or the list picks a file; an attachment view gives way to it.
    pub fn pick(&mut self, name: String, cx: &mut Context<Self>) {
        self.picked = Some(name);
        self.image = None;
        self.ensure_text(cx);
        cx.notify();
    }

    /// `workspace.dirty` and a logged change: the list again, then the text if its time changed.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.read_list(cx);
    }

    pub(super) fn current(&self) -> Option<String> {
        let names: Vec<String> = self
            .list
            .iter()
            .flatten()
            .map(|file| file.name.clone())
            .collect();
        current_doc(self.picked.as_deref(), &names, self.start_new)
    }

    pub(super) fn modified_of(&self, name: &str) -> Option<String> {
        self.list
            .iter()
            .flatten()
            .find(|file| file.name == name)
            .map(|file| file.modified_at.clone())
    }

    fn read_list(&self, cx: &mut Context<Self>) {
        let Some(source) = self.dashboard.read(cx).source().cloned() else {
            return;
        };
        let answer = source.list_stage_files(&self.task_id);
        cx.spawn(async move |this, cx| {
            let read = answer.await.map_err(|error| reason(&error));
            this.update(cx, |this, cx| this.apply_list(read, cx)).ok();
        })
        .detach();
    }

    pub(super) fn apply_list(
        &mut self,
        read: Result<Vec<StageFile>, String>,
        cx: &mut Context<Self>,
    ) {
        match read {
            Ok(list) => {
                self.list = Some(list);
                self.list_error = None;
            }
            // A failed refresh keeps what is shown.
            Err(message) if self.list.is_none() => self.list_error = Some(message),
            Err(_) => {}
        }
        self.ensure_text(cx);
        cx.notify();
    }

    /// Once per tab: the attachments of a task do not change.
    fn read_attachments(&self, cx: &mut Context<Self>) {
        let Some(source) = self.dashboard.read(cx).source().cloned() else {
            return;
        };
        let answer = source.list_attachments(&self.task_id);
        cx.spawn(async move |this, cx| {
            let read = answer.await.unwrap_or_default();
            this.update(cx, |this, cx| {
                this.attachments = read;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Reads the shown file when it is new, or when the list shows a new time for it.
    pub(super) fn ensure_text(&mut self, cx: &mut Context<Self>) {
        let Some(name) = self.current() else {
            return;
        };
        let Some(modified) = self.modified_of(&name) else {
            return;
        };
        let have = self
            .shown
            .as_ref()
            .is_some_and(|shown| shown.name == name && shown.modified_at == modified);
        let key = (name.clone(), modified.clone());
        if have || self.reading.as_ref() == Some(&key) {
            return;
        }
        let Some(source) = self.dashboard.read(cx).source().cloned() else {
            return;
        };
        self.reading = Some(key.clone());
        let answer = source.read_stage_file(&self.task_id, &name);
        cx.spawn(async move |this, cx| {
            let read = answer.await.map(|file| file.text).map_err(|e| reason(&e));
            this.update(cx, |this, cx| this.apply_text(key, read, cx))
                .ok();
        })
        .detach();
    }

    pub(super) fn apply_text(
        &mut self,
        key: (String, String),
        read: Result<String, String>,
        cx: &mut Context<Self>,
    ) {
        if self.reading.as_ref() == Some(&key) {
            self.reading = None;
        }
        // A file picked meanwhile has its own read.
        if self.current().as_deref() != Some(key.0.as_str()) {
            return;
        }
        match read {
            Ok(text) => {
                self.shown = Some(ShownText {
                    name: key.0,
                    modified_at: key.1,
                    text,
                });
                self.text_error = None;
            }
            Err(message) => {
                // The old text of the same file stays.
                let kept = self.shown.as_ref().is_some_and(|shown| shown.name == key.0);
                if !kept {
                    self.text_error = Some((key.0, message));
                }
            }
        }
        cx.notify();
    }

    pub(super) fn text_of(&self, name: &str) -> Option<&str> {
        self.shown
            .as_ref()
            .filter(|shown| shown.name == name)
            .map(|shown| shown.text.as_str())
    }

    /// "Write context.md" and Edit: a draft that begins on the text and time now shown.
    pub(super) fn start_edit(&mut self, name: &str, cx: &mut Context<Self>) {
        let text = self.text_of(name).unwrap_or_default().to_owned();
        let draft = DocDraft {
            original: text.clone(),
            text,
            base: self.modified_of(name),
        };
        let task = self.task_id.clone();
        self.focus_edit = true;
        self.store
            .update(cx, |store, cx| store.set_doc(&task, name, Some(draft), cx));
    }

    /// Makes the edit box of the draft of the shown file, and drops it when the draft is gone.
    fn sync_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let draft = self
            .current()
            .and_then(|name| self.store.read(cx).doc(&self.task_id, &name).cloned());
        let Some(draft) = draft else {
            self.edit = None;
            return;
        };
        let name = self.current().unwrap_or_default();
        if self.edit.as_ref().is_some_and(|edit| edit.name == name) {
            return;
        }
        let editor = cx.new(|cx| {
            let mut editor = Editor::multi_line(window, cx);
            plain(&mut editor, window, cx);
            editor.set_text(draft.text.clone(), window, cx);
            editor
        });
        let own = name.clone();
        let edits = cx.subscribe(&editor, move |this, editor, event: &EditorEvent, cx| {
            if matches!(event, EditorEvent::BufferEdited) {
                let text = editor.read(cx).text(cx);
                this.set_draft_text(&own, text, cx);
            }
        });
        if std::mem::take(&mut self.focus_edit) {
            window.focus(&editor.focus_handle(cx), cx);
        }
        self.edit = Some(EditBox {
            name,
            editor,
            _edits: edits,
        });
    }

    fn set_draft_text(&mut self, name: &str, text: String, cx: &mut Context<Self>) {
        let task = self.task_id.clone();
        let Some(mut draft) = self.store.read(cx).doc(&task, name).cloned() else {
            return;
        };
        draft.text = text;
        self.store
            .update(cx, |store, cx| store.set_doc(&task, name, Some(draft), cx));
    }
}

impl Focusable for DocsTab {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl DocsTab {
    /// The key context of the Docs screen; the panel and main share it.
    fn root(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        self.sync_edit(window, cx);
        let mut context = KeyContext::default();
        context.add("NatamuxDocs");
        let typing = self
            .edit
            .as_ref()
            .is_some_and(|edit| edit.editor.focus_handle(cx).contains_focused(window, cx));
        if !typing {
            context.add("not_editing");
        }
        v_flex().key_context(context).size_full().min_h_0()
    }

    fn reading_state(&self) -> AnyElement {
        let text = match &self.list_error {
            Some(message) => Label::new(message.clone()).color(Color::Error),
            None => Label::new("Reading the stage files\u{2026}").color(Color::Muted),
        };
        div().p_4().child(text).into_any_element()
    }

    /// The list part, drawn in the details panel. No focus tracking: main owns the focus handle.
    pub fn render_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let root = self.root(window, cx);
        let Some(entries) = self.list.clone() else {
            return root.child(self.reading_state()).into_any_element();
        };
        let Some(current) = self.current() else {
            return root
                .child(div().p_2().child(self.attachment_rows(true, cx)))
                .into_any_element();
        };
        root.child(self.list_column(&entries, &current, true, cx))
            .into_any_element()
    }
}

impl Render for DocsTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let in_panel = self.dashboard.read(cx).panel_shows(DetailsView::Docs);
        let root = self.root(window, cx).track_focus(&self.focus);
        let Some(entries) = self.list.clone() else {
            return root.child(self.reading_state()).into_any_element();
        };
        let Some(current) = self.current() else {
            return root.child(self.empty(cx)).into_any_element();
        };
        let main = match self.image.is_some() {
            true => self.image_view(cx),
            false => self.section(&current, window, cx),
        };
        // Both parts fill the height: the header of the file sits at the top.
        let row = h_flex().flex_1().min_h_0().items_stretch();
        let page = match in_panel {
            true => root.child(row.child(main)),
            false => root.child(
                row.child(self.list_column(&entries, &current, false, cx))
                    .child(main),
            ),
        };
        page.into_any_element()
    }
}
