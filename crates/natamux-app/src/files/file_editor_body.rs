//! The writable code view of the editor tab (`MonacoFile.tsx`, kind `edit`). It tells its owner
//! the text after each edit; the owner turns that into a draft.

use gpui::{EventEmitter, FocusHandle, Focusable, Subscription};
use ui::prelude::*;

use crate::text_view::code_view::CodeView;
use crate::text_view::text_box::TextEvent;

/// The text of the editor after a change. A text set by `set_disk_text` comes too; the owner
/// compares it with the disk text, so it makes no draft.
pub struct Edited(pub String);

pub struct FileEditorBody {
    view: CodeView,
    _subscription: Subscription,
}

impl EventEmitter<Edited> for FileEditorBody {}

impl FileEditorBody {
    /// `text` is what the editor shows first: the draft, or the file as read.
    pub fn new(path: &str, text: &str, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let view = CodeView::editor(path, text, window, cx);
        let subscription = view.text_box().subscribe(cx, |_, text_box, event, cx| {
            if event == TextEvent::Edited {
                cx.emit(Edited(text_box.text(cx)));
            }
        });
        FileEditorBody {
            view,
            _subscription: subscription,
        }
    }

    pub fn view(&self) -> &CodeView {
        &self.view
    }

    /// The file's language is parsed, so its colours show; a `--shot` waits for it.
    pub fn parsed(&self, cx: &App) -> bool {
        self.view.parsed(cx)
    }

    pub fn text(&self, cx: &App) -> String {
        self.view.text_box().text(cx)
    }

    pub fn wraps(&self, cx: &App) -> bool {
        self.view.wraps(cx)
    }

    /// The header's Wrap long lines toggle. Not stored: a reopened file has its default again.
    pub fn toggle_wrap(&mut self, cx: &mut Context<Self>) {
        let wrap = !self.view.wraps(cx);
        self.view.set_wrap(wrap, cx);
        cx.notify();
    }

    /// A new text from disk: only the changed parts are edited, so the cursor and the scroll
    /// stay. The undo history goes with it (T-0316).
    pub fn set_disk_text(&mut self, text: &str, cx: &mut Context<Self>) {
        if self.text(cx) == text {
            return;
        }
        self.view.set_disk_text(text, cx);
    }

    /// Clean on save: no trailing whitespace, and one line break at the end. One undo step.
    pub fn clean(&self, cx: &mut Context<Self>) -> gpui::Task<()> {
        let text = self.text(cx);
        let cleaned = cleaned(&text);
        if cleaned != text {
            self.view.replace_minimal(&cleaned, cx);
        }
        gpui::Task::ready(())
    }

    /// Go to line: the cursor goes to `line` and `column` (1-based), clamped to the text, and
    /// the row is centred.
    pub fn go_to(&self, line: u32, column: Option<u32>, window: &mut Window, cx: &mut App) {
        let at = line_offset(&self.text(cx), line, column);
        self.view.go_to_offset(at, window, cx);
    }
}

/// `text` as Zed's clean on save left it: spaces and tabs off each line end; then, unless the
/// text is empty, the whitespace at its end becomes one line break.
fn cleaned(text: &str) -> String {
    let trimmed: Vec<&str> = text
        .split('\n')
        .map(|line| line.trim_end_matches([' ', '\t']))
        .collect();
    let trimmed = trimmed.join("\n");
    if trimmed.is_empty() {
        return trimmed;
    }
    let end = trimmed.trim_end_matches(|c: char| c.is_ascii_whitespace());
    format!("{end}\n")
}

/// The byte offset of `line` and `column` (1-based, the column in characters) in `text`: the
/// line clamped to the last one, the column to the end of its line.
fn line_offset(text: &str, line: u32, column: Option<u32>) -> usize {
    let row = line.saturating_sub(1) as usize;
    let mut start = 0;
    for _ in 0..row {
        match text[start..].find('\n') {
            Some(at) => start += at + 1,
            None => break,
        }
    }
    let line_text = text[start..].split('\n').next().unwrap_or_default();
    let chars = column.map_or(0, |column| column.saturating_sub(1)) as usize;
    start
        + line_text
            .char_indices()
            .nth(chars)
            .map_or(line_text.len(), |(at, _)| at)
}

impl Focusable for FileEditorBody {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.view.text_box().focus_handle(cx)
    }
}

#[cfg(test)]
#[path = "file_editor_body_tests.rs"]
mod tests;

impl Render for FileEditorBody {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.view.text_box())
    }
}
