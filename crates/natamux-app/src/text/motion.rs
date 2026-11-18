//! Where the caret goes by char, word and line in a text that stores only `\n` (plan 6.3.2).
//! Offsets are bytes. Plain Rust, no gpui.

use unicode_segmentation::GraphemeCursor;

/// The kinds of chars a word move tells apart. A newline is its own kind: it stops a word move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Class {
    Word,
    Space,
    Punctuation,
    Newline,
}

/// The class of `c`; the chars of `extra` count as word chars (a code language's own, as `#` in
/// JSON).
fn class(c: char, extra: &str) -> Class {
    if c == '\n' {
        Class::Newline
    } else if c.is_alphanumeric() || c == '_' || extra.contains(c) {
        Class::Word
    } else if c.is_whitespace() {
        Class::Space
    } else {
        Class::Punctuation
    }
}

/// The start of the grapheme before `offset`, so a letter and its combining marks go together.
pub fn previous_grapheme(text: &str, offset: usize) -> usize {
    let mut cursor = GraphemeCursor::new(offset.min(text.len()), text.len(), true);
    match cursor.prev_boundary(text, 0) {
        Ok(Some(at)) => at,
        _ => 0,
    }
}

/// The end of the grapheme at `offset`.
pub fn next_grapheme(text: &str, offset: usize) -> usize {
    let mut cursor = GraphemeCursor::new(offset.min(text.len()), text.len(), true);
    match cursor.next_boundary(text, 0) {
        Ok(Some(at)) => at,
        _ => text.len(),
    }
}

/// Skips the spaces before `offset`, then one run of word or punctuation chars. At the start of a
/// line it steps over the newline only.
pub fn previous_word_start(text: &str, offset: usize) -> usize {
    previous_word_start_in(text, offset, "")
}

/// `previous_word_start`, with the chars of `extra` as word chars.
pub fn previous_word_start_in(text: &str, offset: usize, extra: &str) -> usize {
    let class = |c| class(c, extra);
    let mut chars = text[..offset].char_indices().rev().peekable();
    let mut at = offset;
    while let Some(&(index, c)) = chars.peek() {
        if class(c) != Class::Space {
            break;
        }
        at = index;
        chars.next();
    }
    let Some(&(index, first)) = chars.peek() else {
        return at;
    };
    let run = class(first);
    if run == Class::Newline {
        return if at == offset { index } else { at };
    }
    for (index, c) in chars {
        if class(c) != run {
            break;
        }
        at = index;
    }
    at
}

/// The mirror of `previous_word_start`: the spaces after `offset`, then one run.
pub fn next_word_end(text: &str, offset: usize) -> usize {
    next_word_end_in(text, offset, "")
}

/// `next_word_end`, with the chars of `extra` as word chars.
pub fn next_word_end_in(text: &str, offset: usize, extra: &str) -> usize {
    let class = |c| class(c, extra);
    let mut chars = text[offset..]
        .char_indices()
        .map(|(index, c)| (offset + index, c))
        .peekable();
    let mut at = offset;
    while let Some(&(index, c)) = chars.peek() {
        if class(c) != Class::Space {
            break;
        }
        at = index + c.len_utf8();
        chars.next();
    }
    let Some(&(index, first)) = chars.peek() else {
        return at;
    };
    let run = class(first);
    if run == Class::Newline {
        return if at == offset { index + 1 } else { at };
    }
    for (index, c) in chars {
        if class(c) != run {
            break;
        }
        at = index + c.len_utf8();
    }
    at
}

/// The word or the run of spaces or punctuation around `offset`, as a double click selects it.
pub fn word_around(text: &str, offset: usize) -> (usize, usize) {
    word_around_in(text, offset, "")
}

/// `word_around`, with the chars of `extra` as word chars.
pub fn word_around_in(text: &str, offset: usize, extra: &str) -> (usize, usize) {
    let class = |c| class(c, extra);
    let after = text[offset..].chars().next();
    let before = text[..offset].chars().next_back();
    let run = match (before, after) {
        (_, Some(c)) if class(c) != Class::Newline => class(c),
        (Some(c), _) if class(c) != Class::Newline => class(c),
        _ => return (offset, offset),
    };
    let start = text[..offset]
        .char_indices()
        .rev()
        .take_while(|(_, c)| class(*c) == run)
        .last()
        .map_or(offset, |(index, _)| index);
    let end = text[offset..]
        .char_indices()
        .take_while(|(_, c)| class(*c) == run)
        .last()
        .map_or(offset, |(index, c)| offset + index + c.len_utf8());
    (start, end)
}

/// The start of the line that holds `offset`.
pub fn line_start(text: &str, offset: usize) -> usize {
    text[..offset].rfind('\n').map_or(0, |at| at + 1)
}

/// The end of the line that holds `offset`, before its newline.
pub fn line_end(text: &str, offset: usize) -> usize {
    text[offset..]
        .find('\n')
        .map_or(text.len(), |at| offset + at)
}

/// `offset` in UTF-16 units, as the platform's input method counts.
pub fn to_utf16(text: &str, offset: usize) -> usize {
    text[..offset.min(text.len())]
        .chars()
        .map(char::len_utf16)
        .sum()
}

/// The byte offset of `units` UTF-16 units. A unit inside a char (half of an emoji) goes back to
/// the start of that char.
pub fn from_utf16(text: &str, units: usize) -> usize {
    let mut counted = 0;
    for (index, c) in text.char_indices() {
        if counted + c.len_utf16() > units {
            return index;
        }
        counted += c.len_utf16();
    }
    text.len()
}

/// The largest char boundary at or below `offset`.
pub fn floor_boundary(text: &str, offset: usize) -> usize {
    let mut at = offset.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

#[cfg(test)]
#[path = "motion_tests.rs"]
mod tests;
