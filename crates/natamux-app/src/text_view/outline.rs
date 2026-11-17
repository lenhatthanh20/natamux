//! The outline of a text (plan 6.4, S4.3c): the items Go to symbol lists and sticky scroll pins.
//! The own `assets/queries/<lang>/outline.scm` files name them with these captures:
//! - `@entry`: the node of the item;
//! - `@title`: a part of the shown text that is also the name; `@word`: a part that is only
//!   shown (a keyword, a type);
//! - `@body_start` and `@body_end`: the tokens around the item's body.
//!
//! The shown text joins its parts in source order with one space where the source has a gap. An
//! item inside another item's range is one level deeper.

use std::cmp::Reverse;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::OnceLock;

use tree_sitter::{Query, QueryCursor, StreamingIterator, Tree};

use crate::git::languages::{LANG_COUNT, LANGS, Lang};

/// One item of the outline. Every range is in bytes of the text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutlineItem {
    pub depth: usize,
    /// The shown text: keywords, the name and the like.
    pub text: String,
    pub range: Range<usize>,
    /// From the start of the first name to the end of the last one.
    pub selection: Range<usize>,
    /// From the start of the first shown part to the end of the last one.
    pub source: Range<usize>,
    /// Between the `@body_start` and the `@body_end` token.
    pub body: Option<Range<usize>>,
    /// The names inside `text`.
    pub names: Vec<Range<usize>>,
}

struct Compiled {
    query: Query,
    item: Option<u32>,
    name: Option<u32>,
    context: Option<u32>,
    open: Option<u32>,
    close: Option<u32>,
}

static COMPILED: [OnceLock<Option<Compiled>>; LANG_COUNT] = [const { OnceLock::new() }; LANG_COUNT];

/// The outline query file of a language; the languages with none have no outline.
fn source(lang: &Lang) -> Option<&'static str> {
    Some(match lang.name {
        "C" => include_str!("../../assets/queries/c/outline.scm"),
        "C++" => include_str!("../../assets/queries/cpp/outline.scm"),
        "CSS" => include_str!("../../assets/queries/css/outline.scm"),
        "Go" => include_str!("../../assets/queries/go/outline.scm"),
        "JavaScript" => include_str!("../../assets/queries/javascript/outline.scm"),
        "JSON" | "JSONC" => include_str!("../../assets/queries/json/outline.scm"),
        "Markdown" => include_str!("../../assets/queries/markdown/outline.scm"),
        "Python" => include_str!("../../assets/queries/python/outline.scm"),
        "Rust" => include_str!("../../assets/queries/rust/outline.scm"),
        "TSX" | "TypeScript" => include_str!("../../assets/queries/typescript/outline.scm"),
        "YAML" => include_str!("../../assets/queries/yaml/outline.scm"),
        _ => return None,
    })
}

fn compile(lang: &Lang) -> Result<Option<Compiled>, String> {
    let Some(text) = source(lang) else {
        return Ok(None);
    };
    let query = Query::new(&(lang.grammar)(), text)
        .map_err(|error| format!("{} outline: {error}", lang.name))?;
    let at = |name: &str| query.capture_index_for_name(name);
    Ok(Some(Compiled {
        item: at("entry"),
        name: at("title"),
        context: at("word"),
        open: at("body_start"),
        close: at("body_end"),
        query,
    }))
}

fn compiled(lang: &'static Lang) -> Option<&'static Compiled> {
    let index = LANGS.iter().position(|other| std::ptr::eq(other, lang))?;
    COMPILED[index]
        .get_or_init(|| compile(lang).unwrap_or(None))
        .as_ref()
}

/// Why the outline query of `lang` does not compile; None when it does or there is none. For the
/// tests: `outline` shows no item for a broken query.
#[cfg(test)]
pub fn query_problem(lang: &'static Lang) -> Option<String> {
    compile(lang).err()
}

/// What one match of the query found.
struct Found {
    pattern: usize,
    item: Range<usize>,
    /// The shown parts, with whether each is a name.
    parts: Vec<(Range<usize>, bool)>,
    open: Option<usize>,
    close: Option<usize>,
}

/// The outline of `text`, parsed as `tree`, in text order.
pub fn outline(lang: &'static Lang, tree: &Tree, text: &str) -> Vec<OutlineItem> {
    outline_in(lang, tree, text, None)
}

/// The outline items whose nodes meet `range` (all of them for None), in text order. An item
/// that meets it keeps the parts outside it: the query gives every match it meets.
pub fn outline_in(
    lang: &'static Lang,
    tree: &Tree,
    text: &str,
    range: Option<Range<usize>>,
) -> Vec<OutlineItem> {
    let Some(compiled) = compiled(lang) else {
        return Vec::new();
    };
    let mut found: Vec<Found> = Vec::new();
    // Each item's place in `found`, by its pattern and range.
    let mut places: HashMap<(usize, usize, usize), usize> = HashMap::new();
    let mut cursor = QueryCursor::new();
    if let Some(range) = range {
        cursor.set_byte_range(range);
    }
    let mut matches = cursor.matches(&compiled.query, tree.root_node(), text.as_bytes());
    while let Some(matched) = matches.next() {
        let mut this = Found {
            pattern: matched.pattern_index,
            item: 0..0,
            parts: Vec::new(),
            open: None,
            close: None,
        };
        let mut has_item = false;
        for capture in matched.captures {
            let range = capture.node.byte_range();
            let index = Some(capture.index);
            if index == compiled.item {
                this.item = range;
                has_item = true;
            } else if index == compiled.name {
                this.parts.push((range, true));
            } else if index == compiled.context {
                this.parts.push((range, false));
            } else if index == compiled.open {
                this.open = Some(range.end);
            } else if index == compiled.close {
                this.close = Some(range.start);
            }
        }
        if !has_item {
            continue;
        }
        // A pattern that matches one child at a time (a selector list) gives one match per
        // child: they are one item.
        let place = (this.pattern, this.item.start, this.item.end);
        match places.get(&place).and_then(|&index| found.get_mut(index)) {
            Some(old) => {
                for part in this.parts {
                    if !old.parts.contains(&part) {
                        old.parts.push(part);
                    }
                }
                old.open = old.open.or(this.open);
                old.close = old.close.or(this.close);
            }
            None => {
                places.insert(place, found.len());
                found.push(this);
            }
        }
    }
    let mut items: Vec<OutlineItem> = found
        .into_iter()
        .filter_map(|found| item_of(found, text))
        .collect();
    items.sort_by_key(|item| (item.range.start, Reverse(item.range.end)));
    items.dedup_by(|later, earlier| later.range == earlier.range);
    set_depths(&mut items);
    items
}

/// The items of `items` (in text order) whose range meets `range`, each as deep as the found
/// items around it: what sticky scroll looks at for the rows from its top row.
pub fn containing(items: &[OutlineItem], range: Range<usize>) -> Vec<OutlineItem> {
    let mut found: Vec<OutlineItem> = items
        .iter()
        .filter(|item| item.range.start < range.end && item.range.end > range.start)
        .cloned()
        .collect();
    set_depths(&mut found);
    found
}

/// An item inside another item's range is one level deeper.
fn set_depths(items: &mut [OutlineItem]) {
    let mut ends: Vec<usize> = Vec::new();
    for item in items {
        while ends.last().is_some_and(|end| *end <= item.range.start) {
            ends.pop();
        }
        item.depth = ends.len();
        ends.push(item.range.end);
    }
}

/// The item with its text, or None when it shows no name.
fn item_of(mut found: Found, text: &str) -> Option<OutlineItem> {
    found
        .parts
        .sort_by_key(|(range, _)| (range.start, Reverse(range.end)));
    let mut shown = String::new();
    let mut names: Vec<Range<usize>> = Vec::new();
    let mut selection: Option<Range<usize>> = None;
    let mut last_end: Option<usize> = None;
    let mut first_start = 0;
    for (range, is_name) in found.parts {
        if last_end.is_some_and(|end| range.start < end) {
            continue;
        }
        let before = shown.len();
        match last_end {
            Some(end) if range.start > end => shown.push(' '),
            Some(_) => {}
            None => first_start = range.start,
        }
        // A name after another name starts where the text before its space ended.
        let start = if is_name && !names.is_empty() {
            before
        } else {
            shown.len()
        };
        shown.push_str(text.get(range.clone())?);
        if is_name {
            names.push(start..shown.len());
            selection = Some(match selection {
                Some(old) => old.start..range.end,
                None => range.start..range.end,
            });
        }
        last_end = Some(range.end);
    }
    let selection = selection?;
    Some(OutlineItem {
        depth: 0,
        text: shown,
        range: found.item,
        selection,
        source: first_start..last_end?,
        body: found.open.zip(found.close).map(|(open, close)| open..close),
        names,
    })
}

#[cfg(test)]
#[path = "outline_tests.rs"]
mod tests;
