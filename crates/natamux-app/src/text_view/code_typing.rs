//! What a code view does with typed text (plan 6.4): pairs that close themselves, a selection
//! that gets surrounded, Backspace inside an empty pair, the closing tag of JSX, and Enter
//! (`code_enter.rs`).

use gpui::Context;
use tree_sitter::Tree;

use super::code_enter::{enter, parse};
use super::code_view::Code;
use super::text_input::TextInput;
use crate::git::languages::Lang;
use crate::text::model::EditKind;

/// An auto-closed pair: the offsets of its opening and closing chars. Typing the closing char
/// at it steps over it, and Backspace right after the opening char deletes both.
#[derive(Clone, Copy, Debug)]
pub(super) struct Region {
    open: usize,
    close: usize,
}

/// The closing tag of the JSX opening tag whose `>` ends at `at` of `text`, when its element has
/// none yet: `</name>`, or `</>` for a fragment. `old` is the text before the `>` with its tree,
/// when there is one.
fn jsx_closing_tag(
    lang: &Lang,
    text: &str,
    at: usize,
    old: Option<(&str, &Tree)>,
) -> Option<String> {
    let tree = parse(lang, text, old)?;
    let mut node = tree
        .root_node()
        .descendant_for_byte_range(at.saturating_sub(1), at);
    while let Some(found) = node {
        if found.kind() == "jsx_opening_element" && found.end_byte() == at {
            let element = found
                .parent()
                .filter(|parent| parent.kind() == "jsx_element");
            let closed = element.is_some_and(|element| {
                let mut cursor = element.walk();
                element.children(&mut cursor).any(|child| {
                    child.kind() == "jsx_closing_element"
                        && !child.is_missing()
                        && !child.byte_range().is_empty()
                })
            });
            if closed {
                return None;
            }
            let name = found
                .child_by_field_name("name")
                .and_then(|name| text.get(name.byte_range()))
                .unwrap_or("");
            return Some(format!("</{name}>"));
        }
        node = found.parent();
    }
    None
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
    /// a selection gets surrounded, the closing char of an auto-closed pair is stepped over, and
    /// the `>` of a JSX opening tag adds its closing tag. False when the view leaves the text to
    /// the plain input.
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
        let version = self.model.version();
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
        let scope = self
            .code
            .as_ref()
            .and_then(Code::tree)
            .map(|tree| typing.scope_at(tree, self.model.text(), at))
            .unwrap_or_default();
        let closer = typing.closer(self.model.text(), at, c, scope);
        let lang = self.code.as_ref().and_then(|code| code.lang);
        let tag = match lang {
            Some(lang)
                if c == '>'
                    && typing.jsx
                    && closer.is_none()
                    && !self.model.text()[..at].ends_with(['=', '-']) =>
            {
                let before = self.model.text();
                let mut typed = before.to_owned();
                typed.insert(at, c);
                let tree = self.code.as_ref().and_then(Code::tree);
                jsx_closing_tag(lang, &typed, at + 1, tree.map(|tree| (before, tree)))
            }
            _ => None,
        };
        let insert = format!("{c}{}{}", closer.unwrap_or(""), tag.unwrap_or_default());
        let edited = self.model.edit(at..at, &insert, EditKind::Typing, now);
        let after = at + c.len_utf8();
        self.model.move_to(after);
        // Every kept region holds `at`, so its close moves by all the inserted text.
        for region in &mut regions {
            region.close += insert.len();
        }
        // Only a one-char closer is stepped over or deleted with its opening.
        if let Some(close) = closer
            && close.chars().count() == 1
        {
            regions.push(Region {
                open: at,
                close: after,
            });
        }
        self.put_regions(regions);
        self.dedent_closer(typing, now, version);
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

    /// Enter in a code view with typing facts (`code_enter.rs`).
    pub(super) fn newline_code(&mut self, cx: &mut Context<Self>) -> bool {
        let (Some(typing), Some(lang)) = (self.typing(), self.code.as_ref().and_then(|c| c.lang))
        else {
            return false;
        };
        let range = self.model.selection().range();
        let tree = self.code.as_ref().and_then(Code::tree);
        let edit = enter(typing, lang, self.model.text(), range, tree);
        let now = self.now(cx);
        let edited = self
            .model
            .edit(edit.range, &edit.text, EditKind::Typing, now);
        self.model.move_to(edit.caret);
        self.put_regions(Vec::new());
        self.changed(edited, cx);
        true
    }
}
