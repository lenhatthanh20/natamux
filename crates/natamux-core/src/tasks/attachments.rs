//! What `natamux-proto` lacks of `packages/core/src/tasks/attachments.ts`: safe names, preview
//! types and sizes in words.

use natamux_proto::tasks::attachments::{
    MAX_ATTACHMENT_COUNT, MAX_ATTACHMENT_SIZE, MAX_TASK_ATTACHMENT_SIZE,
};
use natamux_proto::workspaces::changes::image_type_of;

/// The folder of a task's attachments, next to its stage files.
pub const ATTACHMENTS_DIR: &str = "attachments";
const MAX_NAME_LENGTH: usize = 100;
// Only a short extension is kept when a long name is cut.
const MAX_EXTENSION_LENGTH: usize = 20;
const MB: u64 = 1_000_000;

/// The length of the base64 text of that many bytes.
pub fn base64_length(bytes: u64) -> u64 {
    bytes.div_ceil(3) * 4
}

/// The image type the dashboard may show; None for a file that only downloads. SVG is left out:
/// it can hold scripts.
pub fn preview_type_of(name: &str) -> Option<&'static str> {
    image_type_of(name).filter(|kind| *kind != "image/svg+xml")
}

/// Names are cut in UTF-16 units, as JS slices them.
fn cut(name: &str) -> String {
    let units: Vec<u16> = name.encode_utf16().collect();
    if units.len() <= MAX_NAME_LENGTH {
        return name.to_string();
    }
    let dot = units.iter().rposition(|unit| *unit == u16::from(b'.'));
    let extension: &[u16] = match dot {
        Some(dot) if dot > 0 && units.len() - dot <= MAX_EXTENSION_LENGTH => &units[dot..],
        _ => &[],
    };
    let mut kept = units[..MAX_NAME_LENGTH - extension.len()].to_vec();
    kept.extend_from_slice(extension);
    String::from_utf16_lossy(&kept)
}

/// A file name that cannot leave the attachments folder or start a hidden file: only letters,
/// digits, dots, dashes and underscores (`\w` is ASCII in JS).
pub fn safe_attachment_name(name: &str) -> String {
    let base = name.rsplit(['\\', '/']).next().unwrap_or("");
    // `replace(/[^\w.-]/g, '-')` works on UTF-16 units: a character outside the BMP gives two.
    let replaced: String = base
        .chars()
        .flat_map(|c| {
            let keep = c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-');
            let dashes = if keep { 0 } else { c.len_utf16() };
            std::iter::once(c)
                .filter(move |_| keep)
                .chain(std::iter::repeat_n('-', dashes))
        })
        .collect();
    let safe = cut(replaced.trim_start_matches(['.', '-']));
    if safe.is_empty() { "file".into() } else { safe }
}

fn numbered(name: &str, number: usize) -> String {
    match name.rfind('.') {
        Some(dot) if dot > 0 => format!("{}-{number}{}", &name[..dot], &name[dot..]),
        _ => format!("{name}-{number}"),
    }
}

/// Safe names for files in this order; a name already used gets a number: a.png, a-2.png. Names
/// are compared without case, as the disks of macOS and Windows do.
pub fn attachment_names(names: &[String]) -> Vec<String> {
    let mut taken = std::collections::HashSet::new();
    names
        .iter()
        .map(|name| {
            let safe = safe_attachment_name(name);
            let mut unique = safe.clone();
            let mut number = 2;
            while taken.contains(&unique.to_lowercase()) {
                unique = numbered(&safe, number);
                number += 1;
            }
            taken.insert(unique.to_lowercase());
            unique
        })
        .collect()
}

/// A file refused by `fit_attachments`, with the reason for a person.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refused {
    pub name: String,
    pub reason: String,
}

/// Splits added files (name, size) into those that fit next to the ones a task already has and
/// those over a limit.
pub fn fit_attachments(
    current: &[(String, u64)],
    added: &[(String, u64)],
) -> (Vec<(String, u64)>, Vec<Refused>) {
    let mut count = current.len();
    let mut total: u64 = current.iter().map(|(_, size)| size).sum();
    let mut kept = Vec::new();
    let mut refused = Vec::new();
    for (name, size) in added {
        let reason = if *size > MAX_ATTACHMENT_SIZE {
            Some(format!(
                "{name} is larger than {} MB, the most for one file",
                MAX_ATTACHMENT_SIZE / MB
            ))
        } else if count >= MAX_ATTACHMENT_COUNT {
            Some(format!(
                "{name} is not attached: a task takes at most {MAX_ATTACHMENT_COUNT} files"
            ))
        } else if total + size > MAX_TASK_ATTACHMENT_SIZE {
            Some(format!(
                "{name} is not attached: the files of a task take at most {} MB together",
                MAX_TASK_ATTACHMENT_SIZE / MB
            ))
        } else {
            None
        };
        match reason {
            None => {
                kept.push((name.clone(), *size));
                count += 1;
                total += size;
            }
            Some(reason) => refused.push(Refused {
                name: name.clone(),
                reason,
            }),
        }
    }
    (kept, refused)
}

/// `formatBytes`: `toFixed(1)` rounds as JS does, half away from zero on the decimal text.
pub fn format_bytes(bytes: u64) -> String {
    if bytes < 1000 {
        return format!("{bytes} B");
    }
    if bytes < MB {
        return format!("{} KB", fixed1(bytes as f64 / 1000.0));
    }
    format!("{} MB", fixed1(bytes as f64 / MB as f64))
}

/// `Number#toFixed(1)` of a positive double: its exact decimal value, rounded half up.
fn fixed1(value: f64) -> String {
    // 60 places hold the exact value of any double at or above 1.
    let exact = format!("{value:.60}");
    let (whole, fraction) = exact.split_once('.').unwrap_or((&exact, "0"));
    let mut whole: u64 = whole.parse().unwrap_or(0);
    let mut tenth = u64::from(fraction.as_bytes()[0] - b'0');
    if fraction.as_bytes()[1] >= b'5' {
        tenth += 1;
        if tenth == 10 {
            tenth = 0;
            whole += 1;
        }
    }
    format!("{whole}.{tenth}")
}

#[cfg(test)]
#[path = "attachments_tests.rs"]
mod tests;
