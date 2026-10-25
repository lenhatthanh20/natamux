//! The Vietnamese input gap (`write` and `flush` in `session-input.ts`). A Vietnamese input method
//! types "a", then erases it with DEL and types "á". Claude Code drops the text that it reads in
//! one piece with a DEL, and pieces sent at the same moment are read together. So DEL and other
//! input go to the shell apart, INPUT_GAP_MS from each other. Typing without DEL, or holding
//! Backspace, is not slowed down.

/// Time between a DEL and other input that a program must read apart.
pub const INPUT_GAP_MS: u64 = 25;
/// Time between a paste and its Enter: the Cursor CLI ignores an Enter that it reads in one piece
/// with the end of a paste.
pub const ENTER_AFTER_PASTE_MS: u64 = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputKind {
    Text,
    Erase,
    Submit,
}

/// `data` cut into runs of DEL and runs of anything else (`/\x7f+|[^\x7f]+/g`).
pub fn pieces(data: &str) -> Vec<(String, InputKind)> {
    let mut out: Vec<(String, InputKind)> = Vec::new();
    for char in data.chars() {
        let kind = if char == '\x7f' {
            InputKind::Erase
        } else {
            InputKind::Text
        };
        match out.last_mut() {
            Some((text, last)) if *last == kind => text.push(char),
            _ => out.push((char.to_string(), kind)),
        }
    }
    out
}

/// How long a piece of `kind` must wait, at `now`, after the last write. Pieces of the same kind
/// as the one before go at once.
pub fn wait_before(kind: InputKind, last: Option<(u64, InputKind)>, now: u64) -> u64 {
    let gap = if kind == InputKind::Submit {
        ENTER_AFTER_PASTE_MS
    } else {
        INPUT_GAP_MS
    };
    match last {
        Some((at, last_kind)) if last_kind != kind => (at + gap).saturating_sub(now),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cuts_input_into_runs_of_del_and_runs_of_text() {
        assert!(pieces("").is_empty());
        assert_eq!(
            pieces("a\x7f\x7fáb\x7f"),
            [
                ("a".to_string(), InputKind::Text),
                ("\x7f\x7f".to_string(), InputKind::Erase),
                ("áb".to_string(), InputKind::Text),
                ("\x7f".to_string(), InputKind::Erase),
            ]
        );
    }

    #[test]
    fn waits_only_between_pieces_of_another_kind() {
        assert_eq!(wait_before(InputKind::Text, None, 50), 0);
        assert_eq!(
            wait_before(InputKind::Text, Some((40, InputKind::Text)), 50),
            0
        );
        assert_eq!(
            wait_before(InputKind::Erase, Some((40, InputKind::Text)), 50),
            15
        );
        assert_eq!(
            wait_before(InputKind::Text, Some((10, InputKind::Erase)), 50),
            0
        );
        assert_eq!(
            wait_before(InputKind::Submit, Some((40, InputKind::Text)), 50),
            90
        );
    }
}
