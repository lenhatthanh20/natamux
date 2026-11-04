//! T-CODE-1 (plan 6.4, 7.6): typing in the editor tab and the JSON tab, per language: auto-close
//! of brackets and quotes (and none inside strings and comments), typing over a closing char,
//! Backspace in an empty pair, Enter between brackets, `/*` + Enter, comment and Markdown list
//! continuation, Tab, and word moves over `a#b $c d-e.f:g`. Each case records the text and the
//! selections after each step; the own code view must give the same.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use editor::Editor;
use gpui::{AnyView, Entity, Focusable as _, TestAppContext, VisualTestContext};
use serde_json::{Value, json};
use ui::prelude::*;

use crate::app::test_driver::{press, type_text};
use crate::app::{golden_files, shortcuts, test_views};
use crate::files::file_editor_body::FileEditorBody;
use crate::projects::json_tab::JsonTab;
use crate::stages::pipeline_editor::PipelineEditor;

const SET: &str = "code_typing";
/// Where a case's text has its caret.
const CARET: char = 'ˇ';
const OPENERS: [(&str, &str); 7] = [
    ("(", ")"),
    ("[", "]"),
    ("{", "}"),
    ("\"", "\""),
    ("'", "'"),
    ("`", "`"),
    ("<", ">"),
];

/// A file kind under test: its path picks the language. `base` is the text the generic cases
/// type into (inside an injected language, such as a regex in JavaScript); `string` and `comment`
/// are texts with the caret inside a string and a comment of the language.
pub(super) struct Host {
    pub name: &'static str,
    pub path: &'static str,
    pub json_tab: bool,
    pub base: Option<&'static str>,
    pub string: Option<&'static str>,
    pub comment: Option<&'static str>,
}

impl Host {
    pub(super) const fn new(name: &'static str, path: &'static str) -> Host {
        Host {
            name,
            path,
            json_tab: false,
            base: None,
            string: None,
            comment: None,
        }
    }

    const fn string(mut self, text: &'static str) -> Host {
        self.string = Some(text);
        self
    }

    const fn comment(mut self, text: &'static str) -> Host {
        self.comment = Some(text);
        self
    }

    const fn base(mut self, text: &'static str) -> Host {
        self.base = Some(text);
        self
    }
}

enum Step {
    Type(String),
    Press(String),
}

fn t(text: &str) -> Step {
    Step::Type(text.to_owned())
}

fn p(keys: &str) -> Step {
    Step::Press(keys.to_owned())
}

/// The word keys of the app: Alt on macOS, Ctrl elsewhere (`text_boxes.rs`).
fn word(key: &str) -> Step {
    let held = if cfg!(target_os = "macos") {
        "alt"
    } else {
        "ctrl"
    };
    p(&format!("{held}-{key}"))
}

struct Case {
    label: String,
    text: String,
    steps: Vec<Step>,
}

fn case(label: impl Into<String>, text: impl Into<String>, steps: Vec<Step>) -> Case {
    Case {
        label: label.into(),
        text: text.into(),
        steps,
    }
}

fn cases(host: &Host) -> Vec<Case> {
    let base = host.base.unwrap_or("ˇ");
    let at = |text: &str| base.replace(CARET, text);
    let mut out = Vec::new();
    for (open, close) in OPENERS {
        out.push(case(format!("type {open}"), base, vec![t(open), t(close)]));
        out.push(case(
            format!("backspace {open}"),
            base,
            vec![t(open), p("backspace")],
        ));
        out.push(case(
            format!("enter {open}"),
            base,
            vec![t(open), p("enter")],
        ));
        for (place, text) in [
            ("before a word", "ˇx"),
            ("before a comma", "ˇ,"),
            ("before a close", "ˇ}"),
            ("after a word", "xˇ"),
        ] {
            out.push(case(format!("{open} {place}"), at(text), vec![t(open)]));
        }
        for (place, text) in [("in a string", host.string), ("in a comment", host.comment)] {
            if let Some(text) = text {
                out.push(case(format!("{open} {place}"), text, vec![t(open)]));
            }
        }
    }
    for (place, text) in [("in a string", host.string), ("in a comment", host.comment)] {
        if let Some(text) = text {
            out.push(case(format!("enter {place}"), text, vec![p("enter")]));
        }
    }
    out.push(case("block comment", base, vec![t("/*"), p("enter")]));
    out.push(case("doc comment", base, vec![t("/**"), p("enter")]));
    out.push(case("tab", at("  aˇ"), vec![p("tab"), t("x")]));
    if host.base.is_none() {
        out.push(case("indent", "    aˇ", vec![p("enter"), t("b")]));
        out.push(case("indent in a pair", "    f(ˇ)", vec![p("enter")]));
        out.push(case("indent in a block", "    a {ˇ}", vec![p("enter")]));
    }
    let moves: Vec<Step> = (0..8)
        .map(|_| word("right"))
        .chain((0..8).map(|_| word("left")))
        .collect();
    out.push(case("word moves", at("ˇa#b $c d-e.f:g"), moves));
    out.push(case(
        "word deletes",
        at("a#b $c d-e.f:gˇ"),
        (0..4).map(|_| word("backspace")).collect(),
    ));
    if host.path.ends_with(".md") && host.base.is_none() {
        for text in [
            "- itemˇ",
            "* itemˇ",
            "+ itemˇ",
            "1. itemˇ",
            "9. itemˇ",
            "- [ ] taskˇ",
            "- [x] doneˇ",
            "  - nestedˇ",
            "> quoteˇ",
        ] {
            out.push(case(
                format!("list {}", text.trim_end_matches(CARET)),
                text,
                vec![p("enter"), t("b"), p("enter"), p("enter")],
            ));
        }
    }
    out
}

/// The window of the cases: it shows one editor at a time, so an old one is dropped.
pub(super) struct Slot {
    view: Option<AnyView>,
}

impl Render for Slot {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().children(self.view.clone())
    }
}

/// The app's globals, key bindings and languages, and the window of the cases.
pub(super) fn start(cx: &mut TestAppContext) -> (Entity<Slot>, &mut VisualTestContext) {
    test_views::init(cx);
    cx.update(shortcuts::bind);
    cx.update(crate::git::languages::init);
    cx.add_window_view(|_, _| Slot { view: None })
}

/// Shows `text` (with its caret mark) in the host's editor, focused, and returns that editor.
pub(super) fn open(
    host: &Host,
    text: &str,
    slot: &Entity<Slot>,
    cx: &mut VisualTestContext,
) -> Entity<Editor> {
    let caret = text.find(CARET).unwrap_or(text.len());
    let before = &text[..caret];
    let row = before.matches('\n').count() as u32;
    let column = (caret - before.rfind('\n').map_or(0, |at| at + 1)) as u32;
    let text = text.replace(CARET, "");
    let editor = slot.update_in(cx, |slot, window, cx| {
        let (view, editor): (AnyView, _) = if host.json_tab {
            let pipeline = cx.new(|_| PipelineEditor::new(&text));
            let tab = cx.new(|cx| JsonTab::new(pipeline, window, cx));
            let editor = tab.read(cx).editor().clone();
            (tab.into(), editor)
        } else {
            let body = cx.new(|cx| FileEditorBody::new(host.path, &text, window, cx));
            let editor = body.read(cx).editor().clone();
            (body.into(), editor)
        };
        slot.view = Some(view);
        cx.notify();
        editor
    });
    cx.run_until_parked();
    editor.update_in(cx, |editor, window, cx| {
        window.focus(&editor.focus_handle(cx), cx);
        let point = text::Point::new(row, column);
        editor.change_selections(Default::default(), window, cx, |selections| {
            selections.select_ranges([point..point])
        });
    });
    cx.run_until_parked();
    editor
}

/// The text and each selection as `[tail row, tail column, head row, head column]`.
pub(super) fn state(editor: &Entity<Editor>, cx: &mut VisualTestContext) -> (String, Value) {
    editor.update(cx, |editor, cx| {
        let snapshot = editor.display_snapshot(cx);
        let selections: Vec<Value> = editor
            .selections
            .all_adjusted(&snapshot)
            .iter()
            .map(|selection| {
                let (tail, head) = (selection.tail(), selection.head());
                json!([tail.row, tail.column, head.row, head.column])
            })
            .collect();
        (editor.text(cx), Value::Array(selections))
    })
}

/// The language of the host, then one row per case and step: `[case, step, text, selections]`.
fn run(host: &Host, cx: &mut TestAppContext) -> Vec<Value> {
    let (slot, cx) = start(cx);
    let mut rows = Vec::new();
    for (i, case) in cases(host).into_iter().enumerate() {
        let editor = open(host, &case.text, &slot, cx);
        if i == 0 {
            let language = editor.update(cx, |editor, cx| {
                let buffer = editor.buffer().read(cx).as_singleton()?;
                let language = buffer.read(cx).language()?;
                Some(language.name().to_string())
            });
            rows.push(json!({ "path": host.path, "language": language }));
        }
        let (text, selections) = state(&editor, cx);
        rows.push(json!([case.label, 0, text, selections]));
        for (n, step) in case.steps.iter().enumerate() {
            match step {
                Step::Type(text) => type_text(cx, text),
                Step::Press(keys) => press(cx, keys),
            }
            cx.run_until_parked();
            let (text, selections) = state(&editor, cx);
            rows.push(json!([case.label, n + 1, text, selections]));
        }
    }
    rows
}

const C_STRING: &str = "char *s = \"abˇ\";";
const SLASH_COMMENT: &str = "// abˇ";
const HASH_COMMENT: &str = "# abˇ";
const JS_STRING: &str = "const s = \"abˇ\"";
const JSON_STRING: &str = "{\"a\": \"abˇ\"}";

macro_rules! hosts {
    ($($module:ident: $host:expr,)*) => {$(
        mod $module {
            use super::*;

            #[gpui::test]
            fn typing_matches_the_golden_file(cx: &mut TestAppContext) {
                let host = $host;
                golden_files::check(SET, host.name, &run(&host, cx));
            }

            #[gpui::test]
            #[ignore]
            fn golden_code_typing(cx: &mut TestAppContext) {
                let host = $host;
                golden_files::write(SET, host.name, &run(&host, cx));
            }
        }
    )*};
}

hosts! {
    bash: Host::new("bash", "x.sh").string("echo \"abˇ\"").comment(HASH_COMMENT),
    c: Host::new("c", "x.c").string(C_STRING).comment(SLASH_COMMENT),
    cpp: Host::new("cpp", "x.cpp").string(C_STRING).comment(SLASH_COMMENT),
    css: Host::new("css", "x.css")
        .string("a { content: \"abˇ\"; }")
        .comment("/* abˇ */"),
    diff: Host::new("diff", "x.diff"),
    env: Host::new("env", "prod.env").string("A=\"abˇ\"").comment(HASH_COMMENT),
    gitcommit: Host::new("gitcommit", "COMMIT_EDITMSG").comment(HASH_COMMENT),
    go: Host::new("go", "x.go").string("var s = \"abˇ\"").comment(SLASH_COMMENT),
    gomod: Host::new("gomod", "go.mod").comment(SLASH_COMMENT),
    gowork: Host::new("gowork", "go.work").comment(SLASH_COMMENT),
    javascript: Host::new("javascript", "x.js").string(JS_STRING).comment(SLASH_COMMENT),
    jsdoc: Host::new("jsdoc", "x.js").base("/**\n * ˇ\n */"),
    json: Host::new("json", "x.json").string(JSON_STRING),
    json_tab: Host {
        json_tab: true,
        ..Host::new("json-tab", ".natamux.json").string(JSON_STRING)
    },
    jsonc: Host::new("jsonc", "x.jsonc").string(JSON_STRING).comment(SLASH_COMMENT),
    markdown: Host::new("markdown", "x.md"),
    markdown_inline: Host::new("markdown-inline", "x.md").base("A line ˇ"),
    python: Host::new("python", "x.py").string("s = \"abˇ\"").comment(HASH_COMMENT),
    regex: Host::new("regex", "x.js").base("const r = /aˇ/;"),
    rust: Host::new("rust", "x.rs").string("let s = \"abˇ\";").comment(SLASH_COMMENT),
    tsx: Host::new("tsx", "x.tsx").string(JS_STRING).comment(SLASH_COMMENT),
    typescript: Host::new("typescript", "x.ts").string(JS_STRING).comment(SLASH_COMMENT),
    yaml: Host::new("yaml", "x.yaml").string("a: \"abˇ\"").comment(HASH_COMMENT),
}
