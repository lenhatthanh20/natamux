//! What a code view does with typed text (plan 6.4): pairs that close themselves, a selection
//! that gets surrounded, Backspace inside an empty pair, and the indent of a new line.

use gpui::Context;
use tree_sitter::Tree;

use super::code_view::Code;
use super::text_input::{TAB_SIZE, TextInput};
use crate::git::languages::Lang;
use crate::text::model::EditKind;
use crate::text::motion;

/// An auto-closed pair: the offsets of its opening and closing chars. Typing the closing char
/// at it steps over it, and Backspace right after the opening char deletes both.
#[derive(Clone, Copy, Debug)]
pub(super) struct Region {
    open: usize,
    close: usize,
}

fn parse(lang: &Lang, text: &str) -> Option<Tree> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&(lang.grammar)()).ok()?;
    parser.parse(text, None)
}

impl TextInput {
    /// The auto-closed pairs that still hold the caret, taken out of the view: none after an
    /// edit that was not typed through them.
    fn take_regions(&mut self) -> Vec<Region> {
        let (version, head) = (self.model.version(), self.model.head());
        let Some(code) = self.code.as_mut() else {
            return Vec::new();
        };
        let mut regions = std::mem::take(&mut code.regions);
        if code.regions_version != version {
            regions.clear();
        }
        regions.retain(|region| region.open < head && head <= region.close);
        regions
    }

    /// Keeps `regions` for the text as it is now.
    fn put_regions(&mut self, regions: Vec<Region>) {
        let version = self.model.version();
        if let Some(code) = self.code.as_mut() {
            code.regions = regions;
            code.regions_version = version;
        }
    }

    /// One char typed at the selection of a code view with typing facts: a pair closes itself,
    /// a selection gets surrounded, and the closing char of an auto-closed pair is stepped over.
    /// False when the view leaves the text to the plain input.
    pub(super) fn type_code(&mut self, text: &str, cx: &mut Context<Self>) -> bool {
        let Some(typing) = self.typing() else {
            return false;
        };
        let mut chars = text.chars();
        let (Some(c), None) = (chars.next(), chars.next()) else {
            return false;
        };
        let selection = self.model.selection();
        let now = self.now(cx);
        if !selection.is_empty() {
            let Some(close) = typing.surround(c) else {
                return false;
            };
            let range = selection.range();
            let inner = self.model.text()[range.clone()].to_owned();
            let wrapped = format!("{c}{inner}{close}");
            let edited = self
                .model
                .edit(range.clone(), &wrapped, EditKind::Typing, now);
            let start = range.start + c.len_utf8();
            self.model.select(start, start + inner.len());
            self.put_regions(Vec::new());
            self.changed(edited, cx);
            return true;
        }
        let at = selection.head;
        let over = self.model.text()[at..].starts_with(c);
        let mut regions = self.take_regions();
        if over && let Some(index) = regions.iter().position(|region| region.close == at) {
            regions.remove(index);
            self.model.move_to(at + c.len_utf8());
            self.model.goal_x = None;
            self.put_regions(regions);
            self.changed(false, cx);
            return true;
        }
        let in_string = self
            .code
            .as_ref()
            .and_then(Code::tree)
            .is_some_and(|tree| typing.in_string(tree, at));
        let closer = typing.closer(self.model.text(), at, c, in_string);
        let insert = closer.map_or_else(|| c.to_string(), |close| format!("{c}{close}"));
        let edited = self.model.edit(at..at, &insert, EditKind::Typing, now);
        let after = at + c.len_utf8();
        self.model.move_to(after);
        for region in &mut regions {
            region.close += c.len_utf8();
        }
        if closer.is_some() {
            regions.push(Region {
                open: at,
                close: after,
            });
        }
        self.put_regions(regions);
        self.changed(edited, cx);
        true
    }

    /// Backspace right after the opening char of an auto-closed pair deletes both chars.
    pub(super) fn delete_pair(&mut self, cx: &mut Context<Self>) -> bool {
        if self.typing().is_none() || !self.model.selection().is_empty() || self.read_only {
            return false;
        }
        let at = self.model.head();
        let mut regions = self.take_regions();
        let Some(index) = regions
            .iter()
            .position(|region| region.close == at && region.open + 1 == at)
        else {
            self.put_regions(regions);
            return false;
        };
        let region = regions.remove(index);
        let now = self.now(cx);
        let edited = self
            .model
            .edit(region.open..region.close + 1, "", EditKind::Deleting, now);
        self.changed(edited, cx);
        true
    }

    /// Enter in a code view with typing facts: between the chars of a pair that opens a block,
    /// the caret goes on an indented line of its own and the closing char on the next; else the
    /// new line takes the indent the language gives it.
    pub(super) fn newline_code(&mut self, cx: &mut Context<Self>) -> bool {
        let (Some(typing), Some(lang)) = (self.typing(), self.code.as_ref().and_then(|c| c.lang))
        else {
            return false;
        };
        let range = self.model.selection().range();
        let text = self.model.text();
        let line_start = motion::line_start(text, range.start);
        let lead: String = text[line_start..]
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect();
        let (insert, caret) = if range.is_empty() && typing.expands(text, range.start) {
            let middle = format!("\n{lead}{}", " ".repeat(TAB_SIZE));
            let caret = range.start + middle.len();
            (format!("{middle}\n{lead}"), caret)
        } else {
            let mut after = text.to_owned();
            after.replace_range(range.clone(), "\n");
            let row = after[..=range.start].matches('\n').count();
            let indent =
                parse(lang, &after).map_or(lead, |tree| typing.indent_for(&after, &tree, row));
            let insert = format!("\n{indent}");
            let caret = range.start + insert.len();
            (insert, caret)
        };
        let now = self.now(cx);
        let edited = self.model.edit(range, &insert, EditKind::Typing, now);
        self.model.move_to(caret);
        self.put_regions(Vec::new());
        self.changed(edited, cx);
        true
    }
}
