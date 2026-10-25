//! The rules of the Docs tab that need no window (`features/workspaces/DocsTab.tsx`): which file
//! shows, the edit draft, the questions, the view mode, which links open.

use super::new_workspace_rules::format_bytes;

/// The file "Write context.md" starts.
pub const FIRST_FILE: &str = "context.md";

pub const STALE_LINE: &str =
    "This file changed after you began editing. Saving replaces the new text.";

pub const EMPTY_TEXT: &str = "No stage files yet. The Context stage writes context.md, and each \
                              stage after it adds its own.";

/// Which file shows: the picked one when the list has it, else the first, else `context.md` after
/// "Write context.md", else none.
pub fn current_doc(picked: Option<&str>, names: &[String], start_new: bool) -> Option<String> {
    if let Some(name) = picked.filter(|p| names.iter().any(|n| n == p)) {
        return Some(name.to_string());
    }
    match names.first() {
        Some(first) => Some(first.clone()),
        None => start_new.then(|| FIRST_FILE.to_string()),
    }
}

/// An unsaved edit of a stage file. `base` is the `modifiedAt` of the file when the edit began.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocDraft {
    pub text: String,
    pub original: String,
    pub base: Option<String>,
}

impl DocDraft {
    pub fn is_dirty(&self) -> bool {
        self.text != self.original
    }
}

/// The file changed since the edit began.
pub fn is_stale(draft: &DocDraft, modified_at: Option<&str>) -> bool {
    draft.base.as_deref() != modified_at
}

/// Asked before Save, only when the file changed under the edit.
pub fn save_question(name: &str, changed: bool) -> Option<String> {
    changed.then(|| format!("{name} changed after you began editing. Replace it with your text?"))
}

/// Asked before Cancel drops a changed text; an unchanged edit closes at once.
pub fn cancel_question(name: &str, dirty: bool) -> Option<String> {
    dirty.then(|| format!("Drop your changes to {name}?"))
}

/// The web's stored `natamux.docs-source`; the app keeps a flag (`ViewState::docs_source`), so
/// this twin only carries the web's reading rule into the tests.
#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ViewMode {
    #[default]
    Rendered,
    Source,
}

#[cfg(test)]
impl ViewMode {
    /// A value that is not known reads as Rendered.
    pub fn parse(value: &str) -> Self {
        match value {
            "source" => Self::Source,
            _ => Self::Rendered,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Rendered => "rendered",
            Self::Source => "source",
        }
    }
}

/// Only web and mail addresses open, so a file cannot start a program through another scheme.
pub fn is_safe_link(href: &str) -> bool {
    let Some((scheme, _)) = href.split_once(':') else {
        return false;
    };
    let mut chars = scheme.chars();
    let valid = chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c));
    valid && ["http", "https", "mailto"].contains(&scheme.to_ascii_lowercase().as_str())
}

/// The line under an attachment's name. A file that is not an image only downloads.
pub fn attachment_meta(size: u64, download: bool) -> String {
    let size = format_bytes(size);
    if download {
        format!("{size} \u{b7} download")
    } else {
        size
    }
}

#[cfg(test)]
#[path = "docs_rules_tests.rs"]
mod tests;
