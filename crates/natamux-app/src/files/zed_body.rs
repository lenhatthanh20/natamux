//! The editor tab as it was on Zed's editor before S4.3c, for the tests that write and check the
//! golden files of the old code (T-HL-1, T-CODE-1, T-OUT-1). The editor tab itself is on the own
//! code view now; this goes with the editor stack (S7).

use editor::scroll::Autoscroll;
use editor::{CurrentLineHighlight, Editor, MultiBuffer, SelectionEffects};
use gpui::{Entity, Focusable};
use language::{Buffer, ParseStatus};
use ui::prelude::*;

use crate::components::go_to_line::line_target;
use crate::git::change_rules::wraps_by_default;
use crate::git::diff_editors::plain_switches;
use crate::git::languages::language_for;

pub struct ZedBody {
    editor: Entity<Editor>,
    buffer: Entity<Buffer>,
}

impl ZedBody {
    pub fn new(path: &str, text: &str, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let buffer = cx.new(|cx| Buffer::local(text, cx));
        if let Some(language) = language_for(path, cx) {
            let buffer = buffer.clone();
            cx.spawn(async move |_, cx| {
                if let Some(language) = language.await {
                    buffer.update(cx, |buffer, cx| buffer.set_language(Some(language), cx));
                }
            })
            .detach();
        }
        let multi = cx.new(|cx| MultiBuffer::singleton(buffer.clone(), cx));
        let wrap = wraps_by_default(path);
        let editor = cx.new(|cx| {
            let mut editor = Editor::for_multibuffer(multi, None, window, cx);
            plain_switches(&mut editor, wrap, window, cx);
            editor.set_show_line_numbers(true, cx);
            editor.set_current_line_highlight(Some(CurrentLineHighlight::None));
            editor
        });
        ZedBody { editor, buffer }
    }

    pub fn editor(&self) -> &Entity<Editor> {
        &self.editor
    }

    pub fn buffer(&self) -> &Entity<Buffer> {
        &self.buffer
    }

    /// The file's language is loaded and parsed.
    pub fn parsed(&self, cx: &App) -> bool {
        let buffer = self.buffer.read(cx);
        let mut status = buffer.parse_status();
        buffer.language().is_some() && *status.borrow() == ParseStatus::Idle
    }

    /// The cursor goes to `line` and `column` (1-based), clamped, and the row is centred.
    pub fn go_to(&self, line: u32, column: Option<u32>, window: &mut Window, cx: &mut App) {
        let point = line_target(&self.buffer.read(cx).text_snapshot(), line, column);
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

impl Render for ZedBody {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.editor.clone())
    }
}

#[path = "editor_golden_tests.rs"]
mod golden_tests;
