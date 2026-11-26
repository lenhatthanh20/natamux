//! Parts of `view.json` with rules of their own: the layout of main and the switches of the diff.
//! `app/view_state.rs` reads and writes the whole file.

use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Layout {
    #[default]
    Table,
    Board,
    Progress,
}

impl Layout {
    pub fn name(self) -> &'static str {
        match self {
            Layout::Table => "table",
            Layout::Board => "board",
            Layout::Progress => "progress",
        }
    }
}

/// The switches of the diff, with the defaults of the web.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiffFlags {
    pub side_by_side: bool,
    pub collapse_unchanged: bool,
    pub ignore_whitespace: bool,
    pub show_moves: bool,
    pub wrap_prose: bool,
    pub wrap_code: bool,
    /// Zed's `show_whitespaces: "all"` in every editor (T-0316).
    pub show_whitespace: bool,
}

impl Default for DiffFlags {
    fn default() -> Self {
        DiffFlags {
            side_by_side: true,
            collapse_unchanged: true,
            ignore_whitespace: true,
            show_moves: false,
            wrap_prose: true,
            wrap_code: false,
            show_whitespace: false,
        }
    }
}

impl DiffFlags {
    pub(crate) fn parse(value: Option<&Value>) -> Self {
        let default = DiffFlags::default();
        let get = |name: &str, fallback: bool| flag(value.and_then(|map| map.get(name)), fallback);
        DiffFlags {
            side_by_side: get("sideBySide", default.side_by_side),
            collapse_unchanged: get("collapseUnchanged", default.collapse_unchanged),
            ignore_whitespace: get("ignoreWhitespace", default.ignore_whitespace),
            show_moves: get("showMoves", default.show_moves),
            wrap_prose: get("wrapProse", default.wrap_prose),
            wrap_code: get("wrapCode", default.wrap_code),
            show_whitespace: get("showWhitespace", default.show_whitespace),
        }
    }

    pub(crate) fn to_json(self) -> Value {
        json!({
            "sideBySide": self.side_by_side,
            "collapseUnchanged": self.collapse_unchanged,
            "ignoreWhitespace": self.ignore_whitespace,
            "showMoves": self.show_moves,
            "wrapProse": self.wrap_prose,
            "wrapCode": self.wrap_code,
            "showWhitespace": self.show_whitespace,
        })
    }
}

pub(crate) fn flag(value: Option<&Value>, fallback: bool) -> bool {
    value.and_then(Value::as_bool).unwrap_or(fallback)
}
