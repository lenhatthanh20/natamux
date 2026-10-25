//! The writable Zed editor of the editor tab (`MonacoFile.tsx`, kind `edit`): a `Buffer` with no
//! project, shown through a singleton `MultiBuffer`. It tells its owner the text after each edit;
//! the owner turns that into a draft.

use editor::{CurrentLineHighlight, Editor, EditorEvent, MultiBuffer};
use gpui::{Entity, EventEmitter, Subscription};
use language::Buffer;
use ui::prelude::*;

use crate::git::change_rules::wraps_by_default;
use crate::git::diff_editors::plain_switches;
use crate::git::languages::language_for;

/// The text of the editor after a change. A text set by `set_disk_text` comes too; the owner
/// compares it with the disk text, so it makes no draft.
pub struct Edited(pub String);

pub struct FileEditorBody {
    editor: Entity<Editor>,
    buffer: Entity<Buffer>,
    _subscription: Subscription,
    _set: Option<gpui::Task<()>>,
}

impl EventEmitter<Edited> for FileEditorBody {}

impl FileEditorBody {
    /// `text` is what the editor shows first: the draft, or the file as read.
    pub fn new(path: &str, text: &str, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let buffer = cx.new(|cx| Buffer::local(text, cx));
        color_by_language(path, &buffer, cx);
        let multi = cx.new(|cx| MultiBuffer::singleton(buffer.clone(), cx));
        let wrap = wraps_by_default(path);
        let editor = cx.new(|cx| {
            let mut editor = Editor::for_multibuffer(multi, None, window, cx);
            plain_switches(&mut editor, wrap, window, cx);
            editor.set_show_line_numbers(true, cx);
            editor.set_current_line_highlight(Some(CurrentLineHighlight::None));
            editor
        });
        let subscription = cx.subscribe(&editor, |_, editor, event: &EditorEvent, cx| {
            if matches!(event, EditorEvent::BufferEdited) {
                let text = editor.read(cx).text(cx);
                cx.emit(Edited(text));
            }
        });
        FileEditorBody {
            editor,
            buffer,
            _subscription: subscription,
            _set: None,
        }
    }

    pub fn text(&self, cx: &App) -> String {
        self.editor.read(cx).text(cx)
    }

    /// A new text from disk goes into the same buffer: only the changed parts are edited, so the
    /// cursor and the scroll stay.
    pub fn set_disk_text(&mut self, text: &str, cx: &mut Context<Self>) {
        if self.text(cx) == text {
            return;
        }
        let buffer = self.buffer.clone();
        let diff = buffer.read(cx).diff(text.to_owned(), cx);
        self._set = Some(cx.spawn(async move |_, cx| {
            let diff = diff.await;
            buffer.update(cx, |buffer, cx| {
                buffer.apply_diff(diff, cx);
            });
        }));
    }
}

/// The buffer takes the language of the file's extension once it is loaded, for colours.
fn color_by_language(path: &str, buffer: &Entity<Buffer>, cx: &mut Context<FileEditorBody>) {
    let Some(language) = language_for(path, cx) else {
        return;
    };
    let buffer = buffer.clone();
    cx.spawn(async move |_, cx| {
        if let Some(language) = language.await {
            buffer.update(cx, |buffer, cx| buffer.set_language(Some(language), cx));
        }
    })
    .detach();
}

impl Render for FileEditorBody {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.editor.clone())
    }
}
