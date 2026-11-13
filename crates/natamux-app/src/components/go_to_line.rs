//! Go to line (Ctrl+G, T-0316): a small dialog with one box. `120` or `120:5` (line, column, both
//! 1-based) goes there in the diff's working tree or the editor tab; Enter goes, Esc closes.

use std::rc::Rc;

use editor::Editor;
use gpui::{DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, KeyDownEvent};
use text::Point;
use ui::prelude::*;

use super::dialog::{
    DialogFrame, DialogLayer, DialogSize, DialogView, cancel_button, primary_button,
};
use super::form::{input_box, text_editor};

type OnGo = Rc<dyn Fn(u32, Option<u32>, &mut Window, &mut App)>;

/// Where `line` and `column` (1-based, the column in characters) are in `text`: the row clamped
/// to the last one, and the column as the byte column of that row, clamped to its end. A
/// `Point`'s column is in bytes, so a character column must not be used as one (T-0316).
pub fn line_target(text: &text::BufferSnapshot, line: u32, column: Option<u32>) -> Point {
    let row = line.saturating_sub(1).min(text.max_point().row);
    let end = Point::new(row, text.line_len(row));
    let line_text: String = text.text_for_range(Point::new(row, 0)..end).collect();
    let chars = column.map_or(0, |column| column.saturating_sub(1)) as usize;
    Point::new(row, byte_column(&line_text, chars))
}

/// The byte offset of character `chars` of `line`, or the line's length past its end.
fn byte_column(line: &str, chars: usize) -> u32 {
    let bytes = line
        .char_indices()
        .nth(chars)
        .map_or(line.len(), |(at, _)| at);
    u32::try_from(bytes).unwrap_or(u32::MAX)
}

/// `120` or `120:5`, spaces allowed around the parts. None for anything else, and for 0.
pub fn parse_line_target(text: &str) -> Option<(u32, Option<u32>)> {
    let mut parts = text.trim().split(':');
    let line = parts
        .next()?
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|n| *n > 0)?;
    let column = match parts.next() {
        None => None,
        Some(part) => Some(part.trim().parse::<u32>().ok().filter(|n| *n > 0)?),
    };
    if parts.next().is_some() {
        return None;
    }
    Some((line, column))
}

/// Opens the dialog; `on_go` gets the line and the column once a valid target is entered.
pub fn open_go_to_line(
    on_go: impl Fn(u32, Option<u32>, &mut Window, &mut App) + 'static,
    window: &mut Window,
    cx: &mut App,
) {
    let dialog = cx.new(|cx| GoToLine {
        input: text_editor("Line, or line:column", None, window, cx),
        on_go: Rc::new(on_go),
        focus: cx.focus_handle(),
        cancel_focus: cx.focus_handle(),
        submit_focus: cx.focus_handle(),
    });
    let first = dialog.read(cx).input.focus_handle(cx);
    DialogLayer::open(dialog, window, cx);
    window.focus(&first, cx);
}

pub struct GoToLine {
    input: Entity<Editor>,
    on_go: OnGo,
    focus: FocusHandle,
    cancel_focus: FocusHandle,
    submit_focus: FocusHandle,
}

impl EventEmitter<DismissEvent> for GoToLine {}

impl Focusable for GoToLine {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl DialogView for GoToLine {
    fn size(&self) -> DialogSize {
        DialogSize::Small
    }

    fn tab_ring(&self, cx: &App) -> Vec<FocusHandle> {
        vec![
            self.input.focus_handle(cx),
            self.cancel_focus.clone(),
            self.submit_focus.clone(),
        ]
    }

    /// Bad input does nothing: the dialog stays for a fix. The dialog closes first, so the focus
    /// that the jump gives the editor is not taken back to the opener.
    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((line, column)) = parse_line_target(&self.input.read(cx).text(cx)) else {
            return;
        };
        cx.emit(DismissEvent);
        let on_go = self.on_go.clone();
        window.defer(cx, move |window, cx| on_go(line, column, window, cx));
    }
}

impl Render for GoToLine {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let frame = DialogFrame::new(
            "Go to line",
            cx.listener(|_, _, _, cx| cx.emit(DismissEvent)),
        )
        .body_padded()
        .footer(cancel_button(
            "Cancel",
            &self.cancel_focus,
            cx.listener(|_, _, _, cx| cx.emit(DismissEvent)),
        ))
        .footer(primary_button(
            "Go",
            &self.submit_focus,
            false,
            cx.listener(|this, _, window, cx| this.submit(window, cx)),
        ))
        .child(input_box(&self.input, cx));
        // A one-line box leaves Enter to the form around it.
        div()
            .key_context("Dialog")
            .track_focus(&self.focus)
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let keys = &event.keystroke;
                if keys.key == "enter" && !keys.modifiers.modified() {
                    cx.stop_propagation();
                    this.submit(window, cx);
                }
            }))
            .child(frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_line_and_an_optional_column() {
        assert_eq!(parse_line_target("120"), Some((120, None)));
        assert_eq!(parse_line_target("120:5"), Some((120, Some(5))));
        assert_eq!(parse_line_target(" 120 : 5 "), Some((120, Some(5))));
    }

    #[test]
    fn counts_the_column_in_characters() {
        assert_eq!(byte_column("tiếng", 0), 0);
        assert_eq!(byte_column("tiếng", 2), 2);
        assert_eq!(byte_column("tiếng", 3), 5);
        assert_eq!(byte_column("tiếng", 99), 7);
    }

    #[test]
    fn refuses_anything_else() {
        for text in [
            "",
            " ",
            "0",
            "abc",
            ":5",
            "12:",
            "1:2:3",
            "-3",
            "4:0",
            "99999999999",
        ] {
            assert_eq!(parse_line_target(text), None, "{text:?}");
        }
    }
}
