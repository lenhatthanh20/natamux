//! Words and small rules of the Project settings dialog (`features/projects/settings-text.ts`,
//! `ProjectSettingsDialog.tsx`): what the screen starts from, the import question, and why Save is
//! off.

// The settings dialog uses these; they were ported first, with their tests.
#![cfg_attr(not(test), allow(dead_code))]

use natamux_proto::projects::repo::LoadedSettings;
use natamux_proto::stages::capabilities::stage_name_of_id;

use crate::stages::editor_draft::{RawSettings, format_settings, parse_draft, with_steps};
use crate::stages::editor_fields::{ShellField, ShellKind};

/// `CREATE_ISSUE_OPTIONS`: the value and the words of each choice.
pub const CREATE_ISSUE_OPTIONS: [(&str, &str); 4] = [
    (
        "manual",
        "Only when you click Create issue in the task's Progress view",
    ),
    (
        "on-spec-approved",
        "Automatically, when you approve the Spec stage. Without a Spec stage, when the pull request opens",
    ),
    ("on-pr", "Automatically, just before the pull request opens"),
    ("never", "Never"),
];

/// A settings file is a few KiB. A bigger pick is the wrong file, and would slow the editor.
pub const MAX_IMPORT_BYTES: usize = 1024 * 1024;

/// What the screen started from: the text of the file and its version. A broken file starts from
/// the defaults.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Base {
    pub text: String,
    pub version: Option<String>,
    pub broken: Option<String>,
    pub path: String,
    /// The daemon read the file, but the Stages tab cannot (a field of a wrong type, or a value
    /// of a newer core). `broken` says why.
    pub unreadable: bool,
}

pub fn base_of(loaded: &LoadedSettings) -> Base {
    let text = if loaded.error.is_some() {
        "{}".to_string()
    } else if let Some(text) = &loaded.text {
        text.clone()
    } else if loaded.found {
        // The daemon sends `text` for a file it read; this is the fallback from the typed
        // settings, which keep only the stages (and not the other keys of the file).
        format_settings(&with_steps(&RawSettings::new(), &loaded.settings.stages))
    } else {
        "{}".to_string()
    };
    let mut base = Base {
        text,
        version: loaded.version.clone(),
        broken: loaded.error.clone(),
        path: loaded.path.clone(),
        unreadable: false,
    };
    // Starting from {} without a word would let one Save replace the whole file with {}.
    if base.broken.is_none()
        && let Err(reason) = parse_draft(&base.text)
    {
        base.text = "{}".to_string();
        base.broken = Some(format!("The app cannot read it: {reason}"));
        base.unreadable = true;
    }
    base
}

/// Save can replace the file without an edit: only a file the daemon could not read, which the
/// screen shows as the defaults. A file only the app cannot read needs an edit first.
pub fn saves_unchanged(base: &Base) -> bool {
    base.broken.is_some() && !base.unreadable
}

fn shell_field_label(field: ShellKind) -> &'static str {
    match field {
        ShellKind::Check => "Check command",
        ShellKind::Command => "Command",
        ShellKind::Args => "Arguments",
        ShellKind::Run => "Condition command",
    }
}

/// The import question. A file with commands lists each one in full, since they run on this
/// computer once a task reaches the stage. `None` when there is nothing to ask.
pub fn import_question(name: &str, dirty: bool, fields: &[ShellField]) -> Option<String> {
    let replace = format!("Replace your changes with {name}?");
    let later = "Nothing is saved until you click Save.";
    if fields.is_empty() {
        return dirty.then(|| format!("{replace} {later}"));
    }
    let list: Vec<String> = fields
        .iter()
        .map(|field| {
            let shown = field
                .name
                .clone()
                .unwrap_or_else(|| stage_name_of_id(&field.id));
            format!(
                "{shown} ({}) \u{b7} {}: {}",
                field.id,
                shell_field_label(field.field),
                field.value
            )
        })
        .collect();
    let intro = if dirty {
        replace
    } else {
        format!("Import {name}?")
    };
    Some(format!(
        "{intro} It has commands that run in a shell on this computer:\n\n{}\n\n{later}",
        list.join("\n")
    ))
}

pub fn count_label(count: usize, word: &str) -> String {
    format!("{count} {word}{}", if count == 1 { "" } else { "s" })
}

/// Why Save is off, or `None` when it is on. A broken file can be replaced without an edit;
/// otherwise there is nothing to save.
pub fn blocked_reason(
    draft_readable: bool,
    errors: usize,
    dirty: bool,
    file_broken: bool,
) -> Option<&'static str> {
    if !draft_readable {
        Some("Fix the JSON first")
    } else if errors > 0 {
        Some("Fix the problems first")
    } else if unchanged(dirty, file_broken) {
        Some("Nothing changed yet")
    } else {
        None
    }
}

/// `unchanged` of the dialog.
pub fn unchanged(dirty: bool, file_broken: bool) -> bool {
    !dirty && !file_broken
}

#[cfg(test)]
#[path = "settings_text_tests.rs"]
mod tests;
