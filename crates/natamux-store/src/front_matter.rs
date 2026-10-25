//! Markdown with YAML front matter, as gray-matter 4.0.3 reads (`matter(text)`) and writes
//! (`matter.stringify(content, data)`) it with its defaults.

use serde_json::{Map, Value};

use crate::yaml_dump::dump;
use crate::yaml_load::load;

const OPEN: &str = "---";
const CLOSE: &str = "\n---";

/// The front matter and the text after it. A text without front matter has none (`{}`). The
/// front matter is what YAML holds, a mapping as a rule.
pub fn parse(text: &str) -> Result<(Value, String), String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    if !text.starts_with(OPEN) || text[OPEN.len()..].starts_with('-') {
        return Ok((Value::Object(Map::new()), text.to_string()));
    }
    let mut rest = &text[OPEN.len()..];
    // A language may follow the first delimiter, such as `---yaml`.
    // `str.search(/\r?\n/)`; without a line break, slice(0, -1) leaves out the last character.
    let line_end = match rest.find('\n') {
        Some(at) if at > 0 && rest.as_bytes()[at - 1] == b'\r' => at - 1,
        Some(at) => at,
        None => rest.char_indices().last().map_or(0, |(at, _)| at),
    };
    let language = &rest[..line_end];
    if !language.trim().is_empty() {
        // gray-matter's engine lookup also knows `yml` as an alias of `yaml`.
        if !matches!(language.trim().to_lowercase().as_str(), "yaml" | "yml") {
            return Err(format!(
                "gray-matter engine \"{}\" is not registered",
                language.trim()
            ));
        }
        rest = &rest[line_end..];
    }
    let close = rest.find(CLOSE);
    let matter = &rest[..close.unwrap_or(rest.len())];
    let data = if without_comments(matter).trim().is_empty() {
        Value::Object(Map::new())
    } else {
        load(matter)?
    };
    let content = match close {
        None => String::new(),
        Some(at) => {
            let after = &rest[at + CLOSE.len()..];
            let after = after.strip_prefix('\r').unwrap_or(after);
            after.strip_prefix('\n').unwrap_or(after).to_string()
        }
    };
    Ok((data, content))
}

/// `/^\s*#[^\n]+/gm` taken out: lines of comments only.
fn without_comments(matter: &str) -> String {
    matter
        .split('\n')
        .filter(|line| {
            let trimmed = line.trim_start_matches(natamux_core::js::is_js_space);
            !(trimmed.starts_with('#') && trimmed.len() > 1)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `matter.stringify(content, data)` for a content that holds no front matter of its own.
pub fn stringify(content: &str, data: &Value) -> String {
    let yaml = dump(data);
    let matter = natamux_core::js::js_trim(&yaml);
    let mut out = String::new();
    if matter != "{}" {
        out.push_str(&newline(OPEN));
        out.push_str(&newline(matter));
        out.push_str(&newline(OPEN));
    }
    out.push_str(&newline(content));
    out
}

fn newline(text: &str) -> String {
    if text.ends_with('\n') {
        text.to_string()
    } else {
        format!("{text}\n")
    }
}

#[cfg(test)]
#[path = "front_matter_tests.rs"]
mod tests;
