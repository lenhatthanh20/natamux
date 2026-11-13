//! What's new (T-0301): the release notes of the running version, and when they open by
//! themselves. Pure: `whats_new.rs` draws the dialog.

use natamux_update::check::{is_newer, parse_version};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Built in, so the notes need no network and ship inside the signed build. A version without
/// notes fails the build on purpose: `scripts/release.test.sh` demands them for every release.
pub const NOTES_FILE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/releases/v",
    env!("CARGO_PKG_VERSION"),
    ".md"
));
pub const CHANGELOG_URL: &str = "https://natamux.github.io/docs/help/changelog/";
/// The notes end here for someone who updated: the rest is for a fresh download.
const DOWNLOAD_HEADING: &str = "## Download";

/// The notes up to the `## Download` heading, without trailing blank lines.
pub fn notes_body(text: &str) -> &str {
    let mut end = text.len();
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        if line.trim_end() == DOWNLOAD_HEADING {
            end = at;
            break;
        }
        at += line.len();
    }
    text[..end].trim_end()
}

/// The dialog's markdown: the notes, then a link to the notes of every version.
pub fn dialog_text(notes: &str) -> String {
    format!(
        "{}\n\nOlder versions: [{CHANGELOG_URL}]({CHANGELOG_URL})",
        notes_body(notes)
    )
}

/// `seen`: the version whose notes the app last showed or skipped (`whatsNewSeen`).
/// `updated_here`: the updater's download folder of `running` exists, so this start follows an
/// update, also from a version that saved no `seen`.
pub fn show_on_start(seen: Option<&str>, running: &str, updated_here: bool) -> bool {
    match seen {
        Some(seen) if seen == running => false,
        Some(seen) if parse_version(seen).is_some() => is_newer(running, seen),
        Some(_) => true,
        None => updated_here,
    }
}

#[cfg(test)]
#[path = "whats_new_rules_tests.rs"]
mod tests;
