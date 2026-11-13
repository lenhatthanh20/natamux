use std::path::{Path, PathBuf};

use super::*;

#[test]
fn the_input_style_uses_the_short_input_line() {
    // Zed's 1.618 buffer line drew a cursor that filled the 24 px box (T-0302).
    let style = input_style(None, None);
    assert_eq!(style.line_height, Some(relative(INPUT_LINE_HEIGHT)));
    assert_eq!(style.font_family, None);
    assert_eq!(style.font_size, None);
}

#[test]
fn a_mono_field_keeps_its_font_and_size_and_the_input_line() {
    let style = input_style(Some("Mono".into()), Some(12.0));
    assert_eq!(style.font_family, Some(SharedString::from("Mono")));
    assert_eq!(style.font_size, Some(rems_from_px(12.0_f32).into()));
    assert_eq!(style.line_height, Some(relative(INPUT_LINE_HEIGHT)));
}

fn rust_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
}

#[test]
fn every_input_editor_comes_from_input_editor() {
    // Split so this file does not match itself.
    let banned = [
        format!("{}::{}(", "Editor", "single_line"),
        format!("{}::{}(", "Editor", "auto_height"),
    ];
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files(&src, &mut files);
    assert!(!files.is_empty());
    let mut found = Vec::new();
    for file in files {
        let name = file
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if name == "form.rs" || name.ends_with("_tests.rs") {
            continue;
        }
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        // Test modules may make plain editors to check keys.
        let code = text.split("#[cfg(test)]").next().unwrap_or("");
        for (line_no, line) in code.lines().enumerate() {
            if banned.iter().any(|call| line.contains(call.as_str())) {
                found.push(format!("{}:{}", file.display(), line_no + 1));
            }
        }
    }
    assert!(found.is_empty(), "use form::input_editor: {found:?}");
}
