//! The fuzzy matcher of the palettes (`packages/web/src/features/palette/fuzzy.ts`). Every
//! character of the query must appear in the text, in order. Starts of words and runs of
//! neighbours score higher. Ranges are byte offsets on char boundaries, so a caller can slice.

pub type Range = (usize, usize);

#[derive(Clone, Debug, PartialEq)]
pub struct FuzzyMatch {
    pub score: f64,
    pub ranges: Vec<Range>,
}

const WORD_START_BONUS: f64 = 8.0;
const FIRST_CHAR_BONUS: f64 = 4.0;
const NEIGHBOUR_BONUS: f64 = 6.0;
const GAP_PENALTY: f64 = 0.3;
const LONG_QUERY: usize = 4;
const MAX_SPREAD: usize = 2;
// A title that long is not matched past its start.
const MAX_TEXT: usize = 160;

fn lower_of(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

fn is_lower_like(c: char) -> bool {
    c.to_lowercase().eq(std::iter::once(c))
}

fn is_word_start(chars: &[(usize, char)], index: usize) -> bool {
    if index == 0 {
        return true;
    }
    let before = chars[index - 1].1;
    let here = chars[index].1;
    !before.is_ascii_alphanumeric() || (is_lower_like(before) && !is_lower_like(here))
}

fn match_word(word: &str, original: &str) -> Option<FuzzyMatch> {
    let chars: Vec<(usize, char)> = original.char_indices().take(MAX_TEXT).collect();
    let lower: Vec<char> = chars.iter().map(|(_, c)| lower_of(*c)).collect();
    let query: Vec<char> = word.chars().map(lower_of).collect();
    let n = lower.len();
    let m = query.len();

    // A quick test first: most texts do not contain the query as a subsequence.
    let mut probe = 0;
    for c in &lower {
        if probe < m && *c == query[probe] {
            probe += 1;
        }
    }
    if probe < m {
        return None;
    }

    let none = f64::NEG_INFINITY;
    // best[i * n + j]: the best score with query[i] matched at text[j].
    let mut best = vec![none; m * n];
    let mut from = vec![usize::MAX; m * n];
    for i in 0..m {
        for j in i..n {
            if lower[j] != query[i] {
                continue;
            }
            let own =
                1.0 + if is_word_start(&chars, j) {
                    WORD_START_BONUS
                } else {
                    0.0
                } + if j == 0 { FIRST_CHAR_BONUS } else { 0.0 };
            if i == 0 {
                best[j] = own - j.min(8) as f64 * GAP_PENALTY;
                continue;
            }
            let mut top = none;
            let mut top_from = usize::MAX;
            for k in (i - 1)..j {
                let before = best[(i - 1) * n + k];
                if before == none {
                    continue;
                }
                let score = if k + 1 == j {
                    before + NEIGHBOUR_BONUS
                } else {
                    before - (j - k - 1) as f64 * GAP_PENALTY
                };
                if score > top {
                    top = score;
                    top_from = k;
                }
            }
            if top_from != usize::MAX {
                best[i * n + j] = top + own;
                from[i * n + j] = top_from;
            }
        }
    }

    let last = m - 1;
    let mut end = usize::MAX;
    let mut score = none;
    for j in 0..n {
        if best[last * n + j] > score {
            score = best[last * n + j];
            end = j;
        }
    }
    if end == usize::MAX {
        return None;
    }
    let mut positions = vec![0; m];
    let mut at = end;
    for i in (0..m).rev() {
        positions[i] = at;
        if i > 0 {
            at = from[i * n + at];
        }
    }
    // A long query that spreads over the text is noise: "stage" must not find "Show the tab,
    // toggle sidebar" by scattered letters.
    if m >= LONG_QUERY && end - positions[0] + 1 > m * MAX_SPREAD {
        return None;
    }
    Some(FuzzyMatch {
        score,
        ranges: to_ranges(&chars, original.len(), &positions),
    })
}

fn to_ranges(chars: &[(usize, char)], len: usize, positions: &[usize]) -> Vec<Range> {
    let mut ranges: Vec<Range> = Vec::new();
    for &position in positions {
        let start = chars[position].0;
        let end = chars.get(position + 1).map_or(len, |next| next.0);
        match ranges.last_mut() {
            Some(previous) if previous.1 == start => previous.1 = end,
            _ => ranges.push((start, end)),
        }
    }
    ranges
}

fn merge_ranges(ranges: &[Range]) -> Vec<Range> {
    let mut sorted = ranges.to_vec();
    sorted.sort_by_key(|range| range.0);
    let mut merged: Vec<Range> = Vec::new();
    for (start, end) in sorted {
        match merged.last_mut() {
            Some(previous) if start <= previous.1 => previous.1 = previous.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    merged
}

/// None when the text does not match. A query of several words matches when each word does.
pub fn fuzzy_match(query: &str, text: &str) -> Option<FuzzyMatch> {
    let mut words = query.split_whitespace().peekable();
    if words.peek().is_none() {
        return Some(FuzzyMatch {
            score: 0.0,
            ranges: Vec::new(),
        });
    }
    let mut score = 0.0;
    let mut ranges = Vec::new();
    for word in words {
        let found = match_word(word, text)?;
        score += found.score;
        ranges.extend(found.ranges);
    }
    Some(FuzzyMatch {
        score,
        ranges: merge_ranges(&ranges),
    })
}

// The rows draw the ranges as highlights; the cut is kept with its ported test.
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Piece {
    pub text: String,
    pub matched: bool,
}

/// The text cut into pieces, so matched characters can be drawn bold.
#[cfg_attr(not(test), allow(dead_code))]
pub fn split_by_ranges(text: &str, ranges: &[Range]) -> Vec<Piece> {
    let piece = |from: usize, to: usize, matched: bool| Piece {
        text: text[from..to].to_owned(),
        matched,
    };
    let mut pieces = Vec::new();
    let mut at = 0;
    for &(start, end) in ranges {
        if start > at {
            pieces.push(piece(at, start, false));
        }
        pieces.push(piece(start, end, true));
        at = end;
    }
    if at < text.len() {
        pieces.push(piece(at, text.len(), false));
    }
    pieces
}

#[cfg(test)]
#[path = "fuzzy_tests.rs"]
mod tests;
