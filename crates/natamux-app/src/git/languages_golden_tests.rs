//! T-HL-1 (plan 6.4, 7.6): the files of the syntax colour golden (`golden/highlight/`): the
//! `code-<lang>` shot files, three files of the reviewed task's shots and the fixtures in
//! `tests/fixtures/code/`. The golden rows were recorded on Zed's editor before S7; the own
//! highlighter must give the same runs (`text_view/highlight_tests.rs`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gpui::{Hsla, Rgba};
use serde_json::Value;

use crate::app::options::Shot;
use crate::app::shots_text::code_sample;

/// The reviewed task of the shots, and the files of it the shots show in the editor tab.
const REVIEWED: &str = "T-0143";
const SHOT_FILES: [(&str, &str); 3] = [
    ("file-checkout", "src/checkout.ts"),
    ("file-natamux-json", ".natamux.json"),
    ("file-readme", "README.md"),
];

/// Each fixture: its golden name, its path (which picks the language) and its text.
pub(crate) fn fixtures() -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for shot in Shot::ALL {
        if let Shot::Code(lang) = shot {
            let (path, text) = code_sample(lang);
            out.push((lang.state().to_owned(), path.to_owned(), text.to_owned()));
        }
    }
    let seed: Value = serde_json::from_str(include_str!("../app/fake_files.json")).unwrap();
    let files = &seed["worktrees"][REVIEWED]["files"];
    for (name, path) in SHOT_FILES {
        let text = files[path].as_str().unwrap();
        out.push((name.to_owned(), path.to_owned(), text.to_owned()));
    }
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/code");
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    for name in names {
        let text = std::fs::read_to_string(format!("{dir}/{name}")).unwrap();
        out.push((format!("fixture-{name}"), format!("fixtures/{name}"), text));
    }
    out
}

pub(crate) fn hex(color: Hsla) -> String {
    let Rgba { r, g, b, a } = Rgba::from(color);
    let byte = |value: f32| (value * 255.).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}{:02x}",
        byte(r),
        byte(g),
        byte(b),
        byte(a)
    )
}
