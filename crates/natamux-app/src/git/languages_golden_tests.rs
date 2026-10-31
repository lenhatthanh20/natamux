//! T-HL-1 (plan 6.4, 7.6): the syntax colours of the editor tab, as runs of `(byte range, theme
//! key, colour, font weight, font style)` under Natamux Dark, for the `code-<lang>` shot files,
//! three files of the reviewed task's shots and the fixtures in `tests/fixtures/code/`. The own
//! highlighter must give the same runs.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::ops::Range;

use gpui::{Hsla, Rgba, TestAppContext};
use language::{HighlightId, LanguageAwareStyling};
use serde_json::{Value, json};
use theme::ActiveTheme;

use crate::app::golden_files;
use crate::app::options::Shot;
use crate::app::shots_text::code_sample;
use crate::app::test_views;
use crate::files::file_editor_body::FileEditorBody;

const SET: &str = "highlight";

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

/// The rows of one file: a head with its language, then one row per run of one theme key.
fn rows(path: &str, text: &str, cx: &mut TestAppContext) -> Vec<Value> {
    let (body, cx) = cx.add_window_view(|window, cx| FileEditorBody::new(path, text, window, cx));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let body = body.read(cx);
        let buffer = body.editor().read(cx).buffer().read(cx).as_singleton();
        let buffer = buffer.expect("the editor tab shows one buffer");
        let buffer = buffer.read(cx);
        let language = buffer
            .language()
            .map(|language| language.name().to_string());
        let mut rows = vec![json!({
            "path": path,
            "language": language,
            "parsed": body.parsed(cx),
        })];
        let snapshot = buffer.snapshot();
        let styling = LanguageAwareStyling {
            tree_sitter: true,
            diagnostics: false,
        };
        let mut runs: Vec<(Range<usize>, HighlightId)> = Vec::new();
        let mut at = 0;
        for chunk in snapshot.chunks(0..snapshot.len(), styling) {
            let end = at + chunk.text.len();
            if let Some(id) = chunk.syntax_highlight_id {
                match runs.last_mut() {
                    Some((range, last)) if *last == id && range.end == at => range.end = end,
                    _ => runs.push((at..end, id)),
                }
            }
            at = end;
        }
        let syntax = cx.theme().syntax().clone();
        for (range, id) in runs {
            let style = syntax.get(id).copied().unwrap_or_default();
            rows.push(json!([
                range.start,
                range.end,
                syntax.get_capture_name(id),
                style.color.map(hex),
                style.font_weight.map(|weight| weight.0),
                style.font_style.map(|style| format!("{style:?}")),
            ]));
        }
        rows
    })
}

fn start(cx: &mut TestAppContext) {
    test_views::init(cx);
    cx.update(super::init);
}

#[gpui::test]
fn the_colours_match_the_golden_files(cx: &mut TestAppContext) {
    start(cx);
    for (name, path, text) in fixtures() {
        golden_files::check(SET, &name, &rows(&path, &text, cx));
    }
}

#[gpui::test]
fn every_fixture_has_a_language_and_colours(cx: &mut TestAppContext) {
    start(cx);
    for (name, path, text) in fixtures() {
        let rows = rows(&path, &text, cx);
        assert!(rows[0]["parsed"].as_bool().unwrap(), "{name}: not parsed");
        assert!(rows.len() > 1, "{name}: no colours");
    }
}

#[gpui::test]
#[ignore]
fn golden_highlight(cx: &mut TestAppContext) {
    start(cx);
    for (name, path, text) in fixtures() {
        golden_files::write(SET, &name, &rows(&path, &text, cx));
    }
}
