//! The editor tab commands that T-CODE-1 does not record (advisor A25, part 6), written on Zed's
//! editor so the own code view can be held to them:
//! - Mod+/ (comment or uncomment the lines) on one line, an indented line, part of a line, lines
//!   with an empty one between, and a commented line;
//! - Ctrl+Shift+Up and Ctrl+Shift+Down (grow and shrink the selection by the syntax tree);
//! - the gaps of part 3: a closing word or bracket typed on the new line after Enter (Bash
//!   `fi`/`done`/`esac`, Python `else:`, C `else`, `}`, `]`, `)`), and the block comments that
//!   do not end in `*/` (`<!--`, `"""`, `{/*`) + Enter.
//!
//! Each case runs in every editor tab host, so a language with no rule for a case records that it
//! does nothing.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gpui::TestAppContext;
use serde_json::Value;

use super::code_typing_tests::{
    C_STRING, Case, HASH_COMMENT, Host, JS_STRING, JSON_STRING, SLASH_COMMENT, Step, case,
    check_own, p, run_cases, t,
};
use crate::app::golden_files;

const SET: &str = "code_commands";
const LARGER: &str = "ctrl-shift-up";
const SMALLER: &str = "ctrl-shift-down";

/// A line that opens a block, and what closes it when typed on the line after.
const CLOSERS: [(&str, &str); 13] = [
    ("a {", "}"),
    ("a [", "]"),
    ("a (", ")"),
    ("if (a)", "else"),
    ("if a; then", "fi"),
    ("if a; then", "else"),
    ("if a; then", "elif"),
    ("for x in y; do", "done"),
    ("case x in", "esac"),
    ("if a:", "else:"),
    ("if a:", "elif b:"),
    ("try:", "except:"),
    ("try:", "finally:"),
];

/// Mod+/.
fn comment() -> Step {
    p(if cfg!(target_os = "macos") {
        "cmd-/"
    } else {
        "ctrl-/"
    })
}

/// Grow four times, then shrink twice.
fn grow() -> Vec<Step> {
    [LARGER, LARGER, LARGER, LARGER, SMALLER, SMALLER]
        .into_iter()
        .map(p)
        .collect()
}

fn cases(host: &Host) -> Vec<Case> {
    let mut out = vec![
        case("comment a line", "a ˇb\nc", vec![comment(), comment()]),
        case(
            "comment lines",
            "ˇ    a\n\n    b\nc",
            vec![
                p("shift-down"),
                p("shift-down"),
                p("shift-end"),
                comment(),
                comment(),
            ],
        ),
    ];
    out.push(case(
        "comment an indented line",
        "    aˇ",
        vec![comment(), comment()],
    ));
    out.push(case(
        "comment part of a line",
        "a ˇb c",
        vec![p("shift-right"), comment(), comment()],
    ));
    if let Some(text) = host.comment {
        out.push(case("uncomment", text, vec![comment()]));
        out.push(case(
            "uncomment an indented line",
            format!("    {text}"),
            vec![comment()],
        ));
    }
    if let Some(text) = host.string.or(host.base) {
        out.push(case("grow in a string", text, grow()));
    }
    if host.base.is_none() {
        out.push(case("grow in code", "x = f(a, [b + cˇ]);", grow()));
        for (open, close) in CLOSERS {
            out.push(case(
                format!("{open} then {close}"),
                format!("{open}\n    bˇ"),
                vec![p("enter"), t(close)],
            ));
        }
        for start in ["<!--", "\"\"\"", "{/*"] {
            out.push(case(
                format!("comment {start}"),
                "ˇ",
                vec![t(start), p("enter")],
            ));
        }
    }
    out
}

/// `own`: the cases run on the own code view of the editor tab.
fn run(host: &Host, own: bool, cx: &mut TestAppContext) -> Vec<Value> {
    run_cases(host, own, cases(host), cx)
}

macro_rules! hosts {
    ($($module:ident: $host:expr,)*) => {$(
        mod $module {
            use super::*;

            #[gpui::test]
            fn commands_match_the_golden_file(cx: &mut TestAppContext) {
                let host = $host;
                golden_files::check(SET, host.name, &run(&host, false, cx));
            }

            #[gpui::test]
            fn own_commands_match_the_golden_file(cx: &mut TestAppContext) {
                let host = $host;
                check_own(SET, &host, &run(&host, true, cx));
            }

            #[gpui::test]
            #[ignore]
            fn golden_code_commands(cx: &mut TestAppContext) {
                let host = $host;
                golden_files::write(SET, host.name, &run(&host, false, cx));
            }
        }
    )*};
}

// The editor tab hosts of T-CODE-1; the JSON tab is on the own code view already.
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
