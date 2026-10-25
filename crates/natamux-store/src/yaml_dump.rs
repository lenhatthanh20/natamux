//! YAML as js-yaml 3.15.2 `safeDump` writes it with its defaults (indent 2, line width 80), for
//! the JSON values of task and template files: a port of the part of `lib/js-yaml/dumper.js` such
//! values reach. js-yaml looks at UTF-16 units, so a character outside the BMP is not printable
//! for it and makes a string double-quoted.

use natamux_core::json::{js_keys, number_to_js};
use serde_json::{Number, Value};

use crate::yaml_types::is_ambiguous;

const INDENT: usize = 2;
const LINE_WIDTH: usize = 80;

const DEPRECATED_BOOLEANS: [&str; 16] = [
    "y", "Y", "yes", "Yes", "YES", "on", "On", "ON", "n", "N", "no", "No", "NO", "off", "Off",
    "OFF",
];

/// `yaml.safeDump(value)`.
pub fn dump(value: &Value) -> String {
    format!("{}\n", write_node(0, value, true, true, false))
}

fn next_line(level: usize) -> String {
    format!("\n{}", " ".repeat(INDENT * level))
}

fn write_node(level: usize, value: &Value, block: bool, compact: bool, is_key: bool) -> String {
    match value {
        Value::Null => "null".into(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number_text(number),
        Value::String(text) => write_scalar(text, level, is_key),
        Value::Array(items) if block && !items.is_empty() => block_sequence(level, items, compact),
        Value::Array(items) => {
            let parts: Vec<String> = items
                .iter()
                .map(|item| write_node(level, item, false, false, false))
                .collect();
            format!("[{}]", parts.join(", "))
        }
        Value::Object(map) if block && !map.is_empty() => block_mapping(level, map, compact),
        Value::Object(map) => {
            let parts: Vec<String> = js_keys(map)
                .into_iter()
                .map(|key| {
                    let key_text =
                        write_node(level, &Value::String(key.clone()), false, false, false);
                    let explicit = if key_text.chars().count() > 1024 {
                        "? "
                    } else {
                        ""
                    };
                    let value_text = write_node(level, &map[key.as_str()], false, false, false);
                    format!("{explicit}{key_text}: {value_text}")
                })
                .collect();
            format!("{{{}}}", parts.join(", "))
        }
    }
}

/// The int type writes an integer, the float type the rest (`representYamlFloat`).
fn number_text(number: &Number) -> String {
    let value = number.as_f64().unwrap_or(0.0);
    if value.fract() == 0.0 && !(value == 0.0 && value.is_sign_negative()) {
        return number_to_js(value);
    }
    if value == 0.0 {
        return "-0.0".into();
    }
    let text = number_to_js(value);
    // JS can write `5e-100`; YAML wants a dot: `5.e-100`.
    let digits = text.trim_start_matches(['-', '+']);
    let no_dot = digits
        .find('e')
        .is_some_and(|at| digits[..at].bytes().all(|b| b.is_ascii_digit()));
    if no_dot {
        text.replacen('e', ".e", 1)
    } else {
        text
    }
}

fn block_sequence(level: usize, items: &[Value], compact: bool) -> String {
    let mut out = String::new();
    for (index, item) in items.iter().enumerate() {
        let dump = write_node(level + 1, item, true, true, false);
        if !compact || index != 0 {
            out.push_str(&next_line(level));
        }
        out.push_str(if dump.starts_with('\n') { "-" } else { "- " });
        out.push_str(&dump);
    }
    out
}

fn block_mapping(level: usize, map: &serde_json::Map<String, Value>, compact: bool) -> String {
    let mut out = String::new();
    for (index, key) in js_keys(map).into_iter().enumerate() {
        let mut pair = String::new();
        if !compact || index != 0 {
            pair.push_str(&next_line(level));
        }
        let key_dump = write_node(level + 1, &Value::String(key.clone()), true, true, true);
        let explicit = key_dump.encode_utf16().count() > 1024;
        if explicit {
            pair.push_str(if key_dump.starts_with('\n') {
                "?"
            } else {
                "? "
            });
        }
        pair.push_str(&key_dump);
        if explicit {
            pair.push_str(&next_line(level));
        }
        let value_dump = write_node(level + 1, &map[key.as_str()], true, explicit, false);
        pair.push_str(if value_dump.starts_with('\n') {
            ":"
        } else {
            ": "
        });
        pair.push_str(&value_dump);
        out.push_str(&pair);
    }
    out
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Style {
    Plain,
    Single,
    Literal,
    Folded,
    Double,
}

fn write_scalar(text: &str, level: usize, is_key: bool) -> String {
    if text.is_empty() {
        return "''".into();
    }
    if DEPRECATED_BOOLEANS.contains(&text) {
        return format!("'{text}'");
    }
    let indent = INDENT * level.max(1);
    let width = LINE_WIDTH.min(40).max(LINE_WIDTH.saturating_sub(indent));
    match choose_style(text, is_key, width) {
        Style::Plain => text.into(),
        Style::Single => format!("'{}'", text.replace('\'', "''")),
        Style::Literal => format!(
            "|{}{}",
            block_header(text),
            drop_ending_newline(&indent_string(text, indent))
        ),
        Style::Folded => format!(
            ">{}{}",
            block_header(text),
            drop_ending_newline(&indent_string(&fold_string(text, width), indent))
        ),
        Style::Double => format!("\"{}\"", escape_string(text)),
    }
}

fn is_printable(c: u32) -> bool {
    (0x20..=0x7e).contains(&c)
        || ((0xa1..=0xd7ff).contains(&c) && c != 0x2028 && c != 0x2029)
        || ((0xe000..=0xfffd).contains(&c) && c != 0xfeff)
        || (0x10000..=0x10ffff).contains(&c)
}

fn is_white(c: u32) -> bool {
    c == 0x20 || c == 0x09
}

fn is_ns_char(c: u32) -> bool {
    is_printable(c) && !is_white(c) && c != 0xfeff && c != 0x0d && c != 0x0a
}

fn is_plain_safe(c: u32, prev: Option<u32>) -> bool {
    is_printable(c)
        && c != 0xfeff
        && !matches!(c, 0x2c | 0x5b | 0x5d | 0x7b | 0x7d | 0x3a)
        && (c != 0x23 || prev.is_some_and(is_ns_char))
}

fn is_plain_safe_first(c: u32) -> bool {
    is_printable(c)
        && c != 0xfeff
        && !is_white(c)
        && !b"-?:,[]{}#&*!|=>'\"%@`"
            .iter()
            .any(|indicator| u32::from(*indicator) == c)
}

/// `chooseScalarStyle` with the line width of this level.
fn choose_style(text: &str, single_line_only: bool, width: usize) -> Style {
    let units: Vec<u32> = text.encode_utf16().map(u32::from).collect();
    let mut plain = units.first().is_some_and(|c| is_plain_safe_first(*c))
        && !units.last().is_some_and(|c| is_white(*c));
    let mut line_break = false;
    let mut foldable = false;
    let mut previous_break: isize = -1;
    let starts_with_space = |at: isize| units.get((at + 1) as usize) == Some(&0x20);
    for (i, c) in units.iter().copied().enumerate() {
        if !single_line_only && c == 0x0a {
            line_break = true;
            let long = i as isize - previous_break - 1 > width as isize;
            foldable = foldable || (long && !starts_with_space(previous_break));
            previous_break = i as isize;
        } else if !is_printable(c) {
            return Style::Double;
        }
        let prev = i.checked_sub(1).map(|at| units[at]);
        plain = plain && is_plain_safe(c, prev);
    }
    if !single_line_only {
        let long = units.len() as isize - previous_break - 1 > width as isize;
        foldable = foldable || (long && !starts_with_space(previous_break));
    }
    if !line_break && !foldable {
        return if plain && !is_ambiguous(text) {
            Style::Plain
        } else {
            Style::Single
        };
    }
    if foldable {
        Style::Folded
    } else {
        Style::Literal
    }
}

fn needs_indent_indicator(text: &str) -> bool {
    text.trim_start_matches('\n').starts_with(' ')
}

fn block_header(text: &str) -> String {
    let indicator = if needs_indent_indicator(text) {
        INDENT.to_string()
    } else {
        String::new()
    };
    let clip = text.ends_with('\n');
    let keep = clip && (text.ends_with("\n\n") || text == "\n");
    let chomp = if keep {
        "+"
    } else if clip {
        ""
    } else {
        "-"
    };
    format!("{indicator}{chomp}\n")
}

fn drop_ending_newline(text: &str) -> &str {
    text.strip_suffix('\n').unwrap_or(text)
}

/// Indents every line; empty lines (`\n` only) are not indented.
fn indent_string(text: &str, spaces: usize) -> String {
    let pad = " ".repeat(spaces);
    text.split_inclusive('\n')
        .map(|line| {
            if line == "\n" {
                line.to_string()
            } else {
                format!("{pad}{line}")
            }
        })
        .collect()
}

/// `foldString`. Folded text is printable, so it holds no surrogate: chars are UTF-16 units.
fn fold_string(text: &str, width: usize) -> String {
    let first_end = text.find('\n').unwrap_or(text.len());
    let mut result = fold_line(&text[..first_end], width);
    let mut prev_more_indented = text.starts_with('\n') || text.starts_with(' ');
    let mut rest = &text[first_end..];
    while !rest.is_empty() {
        let breaks = rest.len() - rest.trim_start_matches('\n').len();
        let prefix = &rest[..breaks];
        let after = &rest[breaks..];
        let line_end = after.find('\n').unwrap_or(after.len());
        let line = &after[..line_end];
        let more_indented = line.starts_with(' ');
        result.push_str(prefix);
        if !prev_more_indented && !more_indented && !line.is_empty() {
            result.push('\n');
        }
        result.push_str(&fold_line(line, width));
        prev_more_indented = more_indented;
        rest = &after[line_end..];
    }
    result
}

/// `foldLine`: greedy breaks at a space that is followed by a non-space.
fn fold_line(line: &str, width: usize) -> String {
    if line.is_empty() || line.starts_with(' ') {
        return line.to_string();
    }
    let chars: Vec<char> = line.chars().collect();
    let slice = |from: usize, to: usize| chars[from..to].iter().collect::<String>();
    let (mut start, mut curr) = (0usize, 0usize);
    let mut result = String::new();
    for next in
        (0..chars.len().saturating_sub(1)).filter(|at| chars[*at] == ' ' && chars[at + 1] != ' ')
    {
        if next - start > width {
            let end = if curr > start { curr } else { next };
            result.push('\n');
            result.push_str(&slice(start, end));
            start = end + 1;
        }
        curr = next;
    }
    result.push('\n');
    if chars.len() - start > width && curr > start {
        result.push_str(&slice(start, curr));
        result.push('\n');
        result.push_str(&slice(curr + 1, chars.len()));
    } else {
        result.push_str(&slice(start, chars.len()));
    }
    result[1..].to_string()
}

fn escape_string(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        let code = c as u32;
        let escape = match code {
            0x00 => Some("\\0"),
            0x07 => Some("\\a"),
            0x08 => Some("\\b"),
            0x09 => Some("\\t"),
            0x0a => Some("\\n"),
            0x0b => Some("\\v"),
            0x0c => Some("\\f"),
            0x0d => Some("\\r"),
            0x1b => Some("\\e"),
            0x22 => Some("\\\""),
            0x5c => Some("\\\\"),
            0x85 => Some("\\N"),
            0xa0 => Some("\\_"),
            0x2028 => Some("\\L"),
            0x2029 => Some("\\P"),
            _ => None,
        };
        match escape {
            Some(escape) => out.push_str(escape),
            // A pair of surrogates is one code point, written as `\U` and 8 digits.
            None if code > 0xffff => out.push_str(&format!("\\U{code:08X}")),
            None if is_printable(code) => out.push(c),
            None if code <= 0xff => out.push_str(&format!("\\x{code:02X}")),
            None => out.push_str(&format!("\\u{code:04X}")),
        }
    }
    out
}

#[cfg(test)]
#[path = "yaml_dump_tests.rs"]
mod tests;
