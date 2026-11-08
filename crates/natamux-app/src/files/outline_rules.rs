//! The rows of the outline picker (Go to symbol, T-0316): the file's outline items in order,
//! indented by depth; a query keeps the items it fuzzy-matches, best first.

use crate::palette::fuzzy::fuzzy_match;

/// One item of the file's outline: its text, its depth, and the 0-based row and column it starts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutlineRow {
    pub text: String,
    pub depth: usize,
    pub row: u32,
    pub column: u32,
}

pub const NO_SYMBOLS: &str = "No symbols in this file.";
pub const NO_MATCH: &str = "No symbol matches.";

/// The rows to show for `query`: all in file order when it is empty, else the matching ones, the
/// best score first and file order between equal scores.
pub fn outline_rows(items: &[OutlineRow], query: &str) -> Vec<OutlineRow> {
    let query = query.trim();
    if query.is_empty() {
        return items.to_vec();
    }
    let mut found: Vec<(f64, usize)> = items
        .iter()
        .enumerate()
        .filter_map(|(at, item)| Some((fuzzy_match(query, &item.text)?.score, at)))
        .collect();
    found.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    found.into_iter().map(|(_, at)| items[at].clone()).collect()
}

/// What the list says with no row: the file has no outline, or nothing matches.
pub fn empty_text(items: &[OutlineRow]) -> &'static str {
    if items.is_empty() {
        NO_SYMBOLS
    } else {
        NO_MATCH
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(text: &str, depth: usize, line: u32) -> OutlineRow {
        OutlineRow {
            text: text.to_owned(),
            depth,
            row: line,
            column: 0,
        }
    }

    fn items() -> Vec<OutlineRow> {
        vec![
            row("struct Diff", 0, 1),
            row("fn show", 1, 5),
            row("fn build", 1, 20),
            row("fn shown", 0, 40),
        ]
    }

    #[test]
    fn an_empty_query_lists_every_item_in_file_order() {
        assert_eq!(outline_rows(&items(), "  "), items());
    }

    #[test]
    fn a_query_keeps_matches_best_first() {
        let found = outline_rows(&items(), "show");
        let texts: Vec<&str> = found.iter().map(|row| row.text.as_str()).collect();
        assert_eq!(texts.first(), Some(&"fn show"));
        assert!(texts.contains(&"fn shown"));
        assert!(!texts.contains(&"fn build"));
    }

    #[test]
    fn says_why_the_list_is_empty() {
        assert!(outline_rows(&items(), "zzz").is_empty());
        assert_eq!(empty_text(&items()), NO_MATCH);
        assert_eq!(empty_text(&[]), NO_SYMBOLS);
    }
}
