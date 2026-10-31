//! The own syntax highlighter (plan 6.4). tree-sitter parses the text, the query files of the
//! language (`assets/queries/<lang>/`) name its parts, and the theme gives each name its style.
//! It colours the way the Zed editor did, so the code view looks the same:
//! - a capture name takes the longest theme key that is the name or a dot-prefix of it
//!   ("string.special.key" takes "string.special" when the theme has no full match); a name the
//!   theme does not know colours nothing and hides nothing;
//! - captures come in text order; each one colours its node until the node ends, on top of the
//!   ones before it (a stack), so on the same node a later pattern wins;
//! - a capture named `none` colours nothing but hides the captures under it, as in nvim queries;
//! - injections (fenced code, JSDoc, regex) are not run: the old editor never coloured them.

use std::ops::{ControlFlow, Range};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use tree_sitter::{
    InputEdit, ParseOptions, ParseState, Parser, Query, QueryCursor, StreamingIterator, Tree,
};

use crate::git::languages::{LANG_COUNT, LANGS, Lang};

/// The time a parse may take on the main thread before it moves to a background thread.
pub const PARSE_BUDGET: Duration = Duration::from_millis(1);

/// The capture name that hides what is under it and colours nothing.
const BLANK: &str = "none";

/// The editor's limit of matches in progress per query cursor.
const MATCH_LIMIT: u32 = 64;

static COMPILED: [OnceLock<Result<Highlights, String>>; LANG_COUNT] =
    [const { OnceLock::new() }; LANG_COUNT];

/// The query files of one language, compiled once.
pub struct Highlights {
    query: Query,
    /// Per capture: its name after the language's renames, `None` when it is dropped.
    names: Vec<Option<String>>,
    /// Per pattern: the query file it comes from, when that file uses the first-wins rule.
    first_wins_file: Vec<Option<usize>>,
}

impl Highlights {
    /// The compiled queries of `lang`. An error names the language and the query problem.
    pub fn of(lang: &'static Lang) -> Result<&'static Highlights, &'static str> {
        let index = LANGS
            .iter()
            .position(|other| std::ptr::eq(other, lang))
            .unwrap_or_default();
        COMPILED[index]
            .get_or_init(|| compile(lang))
            .as_ref()
            .map_err(String::as_str)
    }

    /// Per capture, the theme key that styles it (see `theme_key`).
    pub fn theme_keys(&self, has_key: impl Fn(&str) -> bool) -> Vec<Option<String>> {
        self.names
            .iter()
            .map(|name| name.as_deref().and_then(|name| theme_key(name, &has_key)))
            .collect()
    }

    /// The coloured runs inside `range` of a text parsed as `tree`. `keys` holds a key per
    /// capture (from `theme_keys`); a capture with none is skipped. Next runs of one key join.
    pub fn runs<K: Clone + PartialEq>(
        &self,
        tree: &Tree,
        text: &str,
        range: Range<usize>,
        keys: &[Option<K>],
    ) -> Vec<(Range<usize>, K)> {
        let mut cursor = QueryCursor::new();
        cursor.set_match_limit(MATCH_LIMIT);
        cursor.set_byte_range(range.clone());
        let mut captures = cursor.captures(&self.query, tree.root_node(), text.as_bytes());
        let mut paint = Paint {
            at: range.start,
            stack: Vec::new(),
            runs: Vec::new(),
        };
        // Nodes a first-wins pattern took at the current start byte, with their file.
        let mut taken: Vec<(usize, usize)> = Vec::new();
        let mut taken_at = usize::MAX;
        while let Some((found, index)) = captures.next() {
            let capture = &found.captures[*index];
            let node = capture.node;
            if let Some(file) = self.first_wins_file[found.pattern_index] {
                if node.start_byte() != taken_at {
                    taken.clear();
                    taken_at = node.start_byte();
                }
                if taken.contains(&(node.id(), file)) {
                    continue;
                }
                taken.push((node.id(), file));
            }
            let blank = self.names[capture.index as usize].as_deref() == Some(BLANK);
            let key = if blank {
                None
            } else {
                keys.get(capture.index as usize).cloned().flatten()
            };
            if key.is_none() && !blank {
                continue;
            }
            paint.until(node.start_byte().min(range.end));
            paint.stack.push((node.end_byte(), key));
        }
        paint.until(range.end);
        paint.runs
    }
}

fn compile(lang: &Lang) -> Result<Highlights, String> {
    let mut source = String::new();
    let mut starts = Vec::new();
    for file in lang.queries {
        starts.push(source.len());
        source.push_str(file.text);
        source.push('\n');
    }
    let query = Query::new(&(lang.grammar)(), &source)
        .map_err(|error| format!("{}: {error}", lang.name))?;
    let first_wins_file = (0..query.pattern_count())
        .map(|pattern| {
            let at = query.start_byte_for_pattern(pattern);
            let file = starts
                .iter()
                .rposition(|start| *start <= at)
                .unwrap_or_default();
            lang.queries[file].first_wins.then_some(file)
        })
        .collect();
    let names = query
        .capture_names()
        .iter()
        .map(
            |name| match lang.renames.iter().find(|(from, _)| from == name) {
                Some(&(_, to)) => to.map(str::to_owned),
                None => Some((*name).to_owned()),
            },
        )
        .collect();
    Ok(Highlights {
        query,
        names,
        first_wins_file,
    })
}

/// The longest key the theme has that is `name` or a dot-prefix of it, as the Zed editor picks.
pub fn theme_key(name: &str, has_key: impl Fn(&str) -> bool) -> Option<String> {
    let mut key = name;
    loop {
        if has_key(key) {
            return Some(key.to_owned());
        }
        key = &key[..key.rfind('.')?];
    }
}

/// The colours of the text as the stack of captures stands: the top one wins until its node
/// ends, then the one under it.
struct Paint<K> {
    at: usize,
    stack: Vec<(usize, Option<K>)>,
    runs: Vec<(Range<usize>, K)>,
}

impl<K: Clone + PartialEq> Paint<K> {
    fn until(&mut self, to: usize) {
        while self.at < to {
            while self.stack.last().is_some_and(|(end, _)| *end <= self.at) {
                self.stack.pop();
            }
            let Some((end, key)) = self.stack.last() else {
                self.at = to;
                return;
            };
            let end = (*end).min(to);
            if let Some(key) = key {
                match self.runs.last_mut() {
                    Some((run, last)) if last == key && run.end == self.at => run.end = end,
                    _ => self.runs.push((self.at..end, key.clone())),
                }
            }
            self.at = end;
        }
    }
}

/// The parse of one text. An edit moves the old tree with the text, so its colours stay right
/// while a new parse runs; the edit count makes sure a late parse never replaces a newer one.
pub struct Syntax {
    lang: &'static Lang,
    parser: Parser,
    tree: Option<Tree>,
    edits: u64,
    /// True when the tree is of the current text.
    done: bool,
}

/// A parse to finish on a background thread: the text, the old tree (edited) and the edit count
/// when it started.
pub struct ParseJob {
    lang: &'static Lang,
    text: Arc<str>,
    old: Option<Tree>,
    edits: u64,
}

/// The tree a `ParseJob` made.
pub struct Parsed {
    tree: Option<Tree>,
    edits: u64,
}

impl Syntax {
    pub fn new(lang: &'static Lang) -> Result<Self, String> {
        Ok(Syntax {
            lang,
            parser: parser_for(lang)?,
            tree: None,
            edits: 0,
            done: false,
        })
    }

    pub fn lang(&self) -> &'static Lang {
        self.lang
    }

    /// The last tree: of the current text when `is_done`, else an older one moved by the edits.
    pub fn tree(&self) -> Option<&Tree> {
        self.tree.as_ref()
    }

    pub fn is_done(&self) -> bool {
        self.done
    }

    /// Parses `text` on this thread for at most `budget`. False when the time ran out: the old
    /// tree stays, and `job` finishes the parse elsewhere.
    pub fn parse(&mut self, text: &str, budget: Duration) -> bool {
        let started = Instant::now();
        let mut stop = |_: &ParseState| {
            if started.elapsed() >= budget {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        };
        let bytes = text.as_bytes();
        let tree = self.parser.parse_with_options(
            &mut |at, _| bytes.get(at..).unwrap_or_default(),
            self.tree.as_ref(),
            Some(ParseOptions::new().progress_callback(&mut stop)),
        );
        match tree {
            Some(tree) => {
                self.tree = Some(tree);
                self.done = true;
            }
            // A stopped parse leaves state in the parser.
            None => self.parser.reset(),
        }
        self.done
    }

    /// Moves the tree with an edit of the text; the next parse starts from it.
    pub fn edit(&mut self, edit: &InputEdit) {
        if let Some(tree) = &mut self.tree {
            tree.edit(edit);
        }
        self.edits += 1;
        self.done = false;
    }

    /// The rest of the parse of `text` (the current text), to run with `ParseJob::run`.
    pub fn job(&self, text: Arc<str>) -> ParseJob {
        ParseJob {
            lang: self.lang,
            text,
            old: self.tree.clone(),
            edits: self.edits,
        }
    }

    /// Takes the tree of a finished job. False, and nothing changes, when an edit came after
    /// the job started.
    pub fn finish(&mut self, parsed: Parsed) -> bool {
        if parsed.edits != self.edits || parsed.tree.is_none() {
            return false;
        }
        self.tree = parsed.tree;
        self.done = true;
        true
    }
}

impl ParseJob {
    /// Parses with no time limit, on its own parser.
    pub fn run(self) -> Parsed {
        let bytes = self.text.as_bytes();
        let tree = parser_for(self.lang).ok().and_then(|mut parser| {
            parser.parse_with_options(
                &mut |at, _| bytes.get(at..).unwrap_or_default(),
                self.old.as_ref(),
                None,
            )
        });
        Parsed {
            tree,
            edits: self.edits,
        }
    }
}

fn parser_for(lang: &Lang) -> Result<Parser, String> {
    let mut parser = Parser::new();
    parser
        .set_language(&(lang.grammar)())
        .map_err(|error| format!("{}: {error}", lang.name))?;
    Ok(parser)
}

#[cfg(test)]
#[path = "highlight_tests.rs"]
mod tests;
