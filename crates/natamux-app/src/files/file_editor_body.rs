//! The writable Zed editor of the editor tab (`MonacoFile.tsx`, kind `edit`): a `Buffer` with no
//! project, shown through a singleton `MultiBuffer`. It tells its owner the text after each edit;
//! the owner turns that into a draft.

use editor::scroll::Autoscroll;
use editor::{CurrentLineHighlight, Editor, EditorEvent, MultiBuffer, SelectionEffects};
use gpui::{Entity, EventEmitter, Focusable, Subscription};
use language::language_settings::SoftWrap;
use language::{Buffer, ParseStatus};
use text::Point;
use ui::prelude::*;

use crate::components::go_to_line::line_target;
use crate::git::change_rules::wraps_by_default;
use crate::git::diff_editors::plain_switches;
use crate::git::languages::language_for;

/// The text of the editor after a change. A text set by `set_disk_text` comes too; the owner
/// compares it with the disk text, so it makes no draft.
pub struct Edited(pub String);

pub struct FileEditorBody {
    editor: Entity<Editor>,
    buffer: Entity<Buffer>,
    /// Wrap long lines: the default of the file's kind until the header's toggle flips it.
    wrap: bool,
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
            wrap,
            _subscription: subscription,
            _set: None,
        }
    }

    pub fn editor(&self) -> &Entity<Editor> {
        &self.editor
    }

    /// The file's language is loaded and parsed, so its colours show; a `--shot` waits for it.
    pub fn parsed(&self, cx: &App) -> bool {
        let buffer = self.buffer.read(cx);
        let mut status = buffer.parse_status();
        buffer.language().is_some() && *status.borrow() == ParseStatus::Idle
    }

    pub fn text(&self, cx: &App) -> String {
        self.editor.read(cx).text(cx)
    }

    pub fn wraps(&self) -> bool {
        self.wrap
    }

    /// The header's Wrap long lines toggle. Not stored: a reopened file has its default again.
    pub fn toggle_wrap(&mut self, cx: &mut Context<Self>) {
        self.wrap = !self.wrap;
        let mode = if self.wrap {
            SoftWrap::EditorWidth
        } else {
            SoftWrap::None
        };
        self.editor
            .update(cx, |editor, cx| editor.set_soft_wrap_mode(mode, cx));
        cx.notify();
    }

    pub fn buffer(&self) -> &Entity<Buffer> {
        &self.buffer
    }

    /// A new text from disk goes into the same buffer: only the changed parts are edited, so the
    /// cursor and the scroll stay. The undo history goes with it (T-0316): an undo would edit the
    /// disk text back toward the old one.
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
                forget_history(buffer);
            });
        }));
    }

    /// Clean on save: no trailing whitespace, and one line break at the end. One undo step.
    pub fn clean(&self, cx: &mut Context<Self>) -> gpui::Task<()> {
        let buffer = self.buffer.clone();
        let trim = buffer.read(cx).remove_trailing_whitespace(None, cx);
        cx.spawn(async move |_, cx| {
            let trim = trim.await;
            buffer.update(cx, |buffer, cx| {
                buffer.start_transaction();
                buffer.apply_diff(trim, cx);
                let newline = buffer.ensure_final_newline(None);
                buffer.apply_diff(newline, cx);
                buffer.end_transaction(cx);
            });
        })
    }

    /// Go to line: the cursor goes to `line` and `column` (1-based), clamped to the text, and
    /// the row is centred.
    pub fn go_to(&self, line: u32, column: Option<u32>, window: &mut Window, cx: &mut App) {
        let point = line_target(&self.buffer.read(cx).text_snapshot(), line, column);
        self.go_to_point(point, window, cx);
    }

    pub fn go_to_point(&self, point: Point, window: &mut Window, cx: &mut App) {
        let buffer = self.buffer.clone();
        self.editor.update(cx, |editor, cx| {
            let anchor = editor
                .buffer()
                .read(cx)
                .buffer_point_to_anchor(&buffer, point, cx);
            if let Some(anchor) = anchor {
                let effects = SelectionEffects::scroll(Autoscroll::center());
                editor.change_selections(effects, window, cx, |selections| {
                    selections.select_anchor_ranges([anchor..anchor])
                });
            }
        });
        window.focus(&self.editor.read(cx).focus_handle(cx), cx);
    }
}

/// Drops every undo and redo step of `buffer`.
fn forget_history(buffer: &mut Buffer) {
    loop {
        let top = buffer
            .peek_undo_stack()
            .or_else(|| buffer.peek_redo_stack())
            .map(|entry| entry.transaction_id());
        let Some(id) = top else {
            break;
        };
        if buffer.forget_transaction(id).is_none() {
            break;
        }
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

#[cfg(test)]
#[path = "file_editor_body_tests.rs"]
mod tests;

impl Render for FileEditorBody {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.editor.clone())
    }
}
