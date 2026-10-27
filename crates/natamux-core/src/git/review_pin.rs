//! Where a review comment sits now: its quoted lines are looked up in the current text of its
//! side, so the comment follows its code when the agent edits the file. Pure.

/// The lines of a text as the diff numbers them: a last newline adds no line, and a `\r` before
/// a newline is not part of the line.
pub fn file_lines(text: &str) -> Vec<&str> {
    let text = text.strip_suffix('\n').unwrap_or(text);
    if text.is_empty() {
        return Vec::new();
    }
    text.split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect()
}

/// The text of lines `start` to `end` (1-based), joined by `\n`. None when they are not all in
/// the text. Both 0 means the whole file: an empty quote.
pub fn quote_lines(text: &str, start: u32, end: u32) -> Option<String> {
    if start == 0 && end == 0 {
        return Some(String::new());
    }
    if start == 0 || end < start {
        return None;
    }
    let lines = file_lines(text);
    let (from, to) = (start as usize - 1, end as usize);
    (to <= lines.len()).then(|| lines[from..to].join("\n"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pinned {
    pub start: u32,
    pub end: u32,
    pub outdated: bool,
}

/// Where the quote is now in `text` (None: the file is gone). Of several matches, the one nearest
/// the stored `start` wins, the earlier one on a tie. Not found: outdated, at the stored lines.
/// A whole-file comment (`start` 0) only needs the file.
pub fn pin(start: u32, end: u32, quote: &str, text: Option<&str>) -> Pinned {
    let stay = |outdated| Pinned {
        start,
        end,
        outdated,
    };
    let Some(text) = text else {
        return stay(true);
    };
    if start == 0 {
        return stay(false);
    }
    // A quote has no last newline: "" is one blank line.
    let wanted: Vec<&str> = quote
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect();
    let lines = file_lines(text);
    if wanted.len() > lines.len() {
        return stay(true);
    }
    let stored = i64::from(start) - 1;
    let best = (0..=lines.len() - wanted.len())
        .filter(|&at| lines[at..at + wanted.len()] == wanted[..])
        .min_by_key(|&at| (at as i64 - stored).abs());
    match best {
        Some(at) => Pinned {
            start: at as u32 + 1,
            end: (at + wanted.len()) as u32,
            outdated: false,
        },
        None => stay(true),
    }
}

#[cfg(test)]
#[path = "review_pin_tests.rs"]
mod tests;
