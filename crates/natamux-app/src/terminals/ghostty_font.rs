//! Ghostty's font, for the terminal when the user picked none
//! (`packages/desktop/src/app/ghostty-config.ts`).

use std::path::{Path, PathBuf};

use super::terminal_font::{TerminalFont, parse_terminal_font};

/// Where Ghostty reads its config, in its own order: a later file wins.
pub fn ghostty_config_paths(xdg: Option<&Path>, home: &Path, mac: bool) -> Vec<PathBuf> {
    let base = match xdg {
        Some(path) if !path.to_string_lossy().trim().is_empty() => path.to_path_buf(),
        _ => home.join(".config"),
    };
    let mut paths = vec![
        base.join("ghostty").join("config"),
        base.join("ghostty").join("config.ghostty"),
    ];
    if mac {
        let support = home
            .join("Library")
            .join("Application Support")
            .join("com.mitchellh.ghostty");
        paths.push(support.join("config"));
        paths.push(support.join("config.ghostty"));
    }
    paths
}

fn unquote(value: &str) -> &str {
    let trimmed = value.trim();
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    }
}

// `key = value` of a line, or None for a comment or another shape.
fn setting_of(line: &str) -> Option<(&str, &str)> {
    let line = line.trim_start();
    let key_end = line
        .find(|c: char| !(c.is_ascii_lowercase() || c == '-'))
        .unwrap_or(line.len());
    let (key, rest) = line.split_at(key_end);
    let value = rest.trim_start().strip_prefix('=')?;
    (!key.is_empty()).then_some((key, value))
}

/// The font of one Ghostty config on top of `font`: the first `font-family` (an empty one clears
/// the list, as in Ghostty) and the last `font-size`.
fn parse_text(text: &str, mut font: TerminalFont) -> TerminalFont {
    let mut family_in_this_file = false;
    for line in text.lines() {
        let Some((key, raw)) = setting_of(line) else {
            continue;
        };
        let value = unquote(raw);
        if key == "font-family" {
            if value.is_empty() {
                font.family = None;
                family_in_this_file = false;
            } else if !family_in_this_file {
                // A later file sets the list again; later lines in one file only add fallbacks.
                font.family = Some(value.to_owned());
                family_in_this_file = true;
            }
        } else if key == "font-size"
            && let Ok(size) = value.parse::<f32>()
            && size.is_finite()
            && size > 0.0
        {
            font.size = Some(size);
        }
    }
    font
}

/// The font of the config files, read in order; None when they set no font.
pub fn parse_ghostty_font(texts: &[&str]) -> Option<TerminalFont> {
    let font = texts
        .iter()
        .fold(TerminalFont::default(), |font, text| parse_text(text, font));
    (font.family.is_some() || font.size.is_some()).then_some(font)
}

/// Like `read_ghostty_font`, with the file reader given (a missing file is skipped, as Ghostty does).
pub fn read_ghostty_font_with(
    xdg: Option<&Path>,
    home: &Path,
    mac: bool,
    read: impl Fn(&Path) -> Option<String>,
) -> Option<TerminalFont> {
    let texts: Vec<String> = ghostty_config_paths(xdg, home, mac)
        .iter()
        .filter_map(|path| read(path))
        .collect();
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    parse_ghostty_font(&refs)
}

/// Ghostty's font on this machine, kept to the sizes the terminal takes (8 to 32), as the web
/// reads what the Electron app stored.
pub fn read_ghostty_font() -> Option<TerminalFont> {
    let home = PathBuf::from(std::env::var_os("HOME")?);
    let xdg = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from);
    let font = read_ghostty_font_with(xdg.as_deref(), &home, cfg!(target_os = "macos"), |path| {
        std::fs::read_to_string(path).ok()
    })?;
    parse_terminal_font(&font.to_json())
}

#[cfg(test)]
#[path = "ghostty_font_tests.rs"]
mod tests;
