//! Ignore whitespace in the diff (T-0316), as Monaco's `ignoreTrimWhitespace`: the leading and
//! trailing whitespace of each line does not count. `buffer_diff` diffs the real working-tree text
//! against a base, so the base it gets is the last commit's text with each line that equals its
//! working-tree line after trimming taken from the working tree. Such a line then makes no hunk.
//! The base keeps the last commit's line count, so the rows of the hunks are real rows of both
//! texts: revert and review comments need no row map.

use std::sync::Arc;

/// Over this many lines on a side, whitespace counts again: the line diff would be slow.
const MAX_LINES: usize = 20_000;

/// The base for `buffer_diff`: the last commit's text, or the effective base when `ignore`.
pub fn diff_base(original: Option<&str>, modified: &str, ignore: bool) -> Option<Arc<str>> {
    let original = original?;
    Some(if ignore {
        Arc::from(whitespace_base(original, modified))
    } else {
        Arc::from(original)
    })
}

/// `original` with each line that matches a line of `modified` after trimming replaced by that
/// line. Every line keeps its own ending.
pub fn whitespace_base(original: &str, modified: &str) -> String {
    let old: Vec<&str> = original.split_inclusive('\n').collect();
    let new: Vec<&str> = modified.split_inclusive('\n').collect();
    if old.len() > MAX_LINES || new.len() > MAX_LINES {
        return original.to_owned();
    }
    let trimmed =
        |lines: &[&str]| -> String { lines.iter().flat_map(|line| [line.trim(), "\n"]).collect() };
    let hunks = language::line_diff(&trimmed(&old), &trimmed(&new));
    let end = (
        old.len() as u32..old.len() as u32,
        new.len() as u32..new.len() as u32,
    );
    let mut out = String::with_capacity(original.len());
    let (mut o, mut n) = (0, 0);
    for (old_rows, new_rows) in hunks.into_iter().chain([end]) {
        let (start, stop) = (old_rows.start as usize, old_rows.end as usize);
        while o < start.min(old.len()) {
            out.push_str(&same_line(old[o], new.get(n).copied()));
            o += 1;
            n += 1;
        }
        let stop = stop.min(old.len()).max(o);
        out.extend(old[o..stop].iter().copied());
        o = stop;
        n = new_rows.end as usize;
    }
    out
}

/// The working tree's line with the last commit's line ending.
fn same_line(old: &str, new: Option<&str>) -> String {
    let Some(new) = new else {
        return old.to_owned();
    };
    let ending = &old[old.trim_end_matches(['\n', '\r']).len()..];
    format!("{}{ending}", new.trim_end_matches(['\n', '\r']))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_whitespace_only_change_gives_the_working_tree_text() {
        let modified = "fn a() {\n    b();  \n}\n";
        assert_eq!(whitespace_base("fn a() {\n  b();\n}\n", modified), modified);
    }

    #[test]
    fn a_real_change_keeps_its_old_lines() {
        let original = "x\n  y\nz\n";
        let modified = "x\ny\nZ\n";
        assert_eq!(whitespace_base(original, modified), "x\ny\nz\n");
    }

    #[test]
    fn a_line_with_a_real_and_a_whitespace_change_is_a_change() {
        assert_eq!(whitespace_base("a \nb\n", "c\nb\n"), "a \nb\n");
    }

    #[test]
    fn added_and_removed_lines_stay_and_the_line_count_is_the_old_one() {
        let original = "a\nold\n b\n";
        let modified = "new\na\nb \nmore\n";
        let base = whitespace_base(original, modified);
        assert_eq!(base, "a\nold\nb \n");
        assert_eq!(base.lines().count(), original.lines().count());
    }

    #[test]
    fn keeps_crlf_and_a_missing_final_line_break() {
        assert_eq!(whitespace_base("a \r\nb", "a\r\nb  "), "a\r\nb  ");
        assert_eq!(whitespace_base("a\r\n", " a\n"), " a\r\n");
    }

    #[test]
    fn an_empty_side_changes_nothing() {
        assert_eq!(whitespace_base("", "a\n"), "");
        assert_eq!(whitespace_base("a\n", ""), "a\n");
    }

    #[test]
    fn a_huge_file_counts_whitespace_again() {
        let original = " x\n".repeat(MAX_LINES + 1);
        let modified = "x\n".repeat(MAX_LINES + 1);
        assert_eq!(whitespace_base(&original, &modified), original);
    }

    #[test]
    fn the_base_is_the_real_text_unless_whitespace_is_ignored() {
        assert_eq!(diff_base(None, "a", true), None);
        assert_eq!(diff_base(Some(" a"), "a", false).as_deref(), Some(" a"));
        assert_eq!(diff_base(Some(" a"), "a", true).as_deref(), Some("a"));
    }
}
