//! The typing facts of each language of the code view (plan 6.4), as values: the pairs and where
//! they close themselves, the comments that go on at Enter, the word chars, and what makes a new
//! line one level deeper. The pair, comment and word facts are the ones the old editor took from
//! each language's settings (plan 6.4). The indent facts are our own: node kinds of each grammar
//! chosen so the code typing golden files of the old editor hold, and for Shell, Python and YAML
//! a small rule of our own for the line that opens a block (`shell_opens`, `python_opens`,
//! `yaml_opens`). Every editor indents by 4 columns here (the app's tab size), Go with a tab.

use super::code_rules::{AUTO, NEWLINE, NO_COMMENT, NO_STRING, Pair, Typing, pair};

const BASE: Typing = Typing {
    pairs: &[],
    close_before: "",
    word_chars: "",
    line_comments: &[],
    doc_comment: None,
    hard_tabs: false,
    lists: false,
    jsx: false,
    indent_nodes: &[],
    indent_brackets: &[],
    indent_after: None,
    string_nodes: &[],
    comment_nodes: &[],
};

/// The chars that let a pair close itself in most languages.
const CODE_CLOSE: &str = ";:.,=}])>";
const SLASHES: &[&str] = &["// "];
const HASH: &[&str] = &["# "];
/// Any node with both tokens of one of these as children is a block.
const ALL_BRACKETS: &[(&str, &str)] = &[("(", ")"), ("[", "]"), ("{", "}")];

/// `line` without its `#` comment: from the first `#` outside quotes, and with `after_space`
/// only a `#` at the line start or after a space or tab (as in Shell and YAML).
// shortcut: a quote char after a backslash still counts; good enough for one line's indent.
fn before_hash(line: &str, after_space: bool) -> &str {
    let mut quote = None;
    let mut last = ' ';
    for (at, c) in line.char_indices() {
        match quote {
            Some(open) if c == open => quote = None,
            Some(_) => {}
            None if c == '"' || c == '\'' => quote = Some(c),
            None if c == '#' && (!after_space || last == ' ' || last == '\t') => {
                return &line[..at];
            }
            None => {}
        }
        last = c;
    }
    line
}

/// Shell: a line that ends with `then`, `do`, `else` or the `in` of `case`, or a `case` pattern
/// such as `a|b)`, opens a block.
fn shell_opens(line: &str) -> bool {
    let code = before_hash(line, true).trim();
    let last = code
        .rsplit(|c: char| c.is_whitespace() || c == ';')
        .next()
        .unwrap_or("");
    matches!(last, "then" | "do" | "else" | "in")
        || (code.len() > 1 && code.ends_with(')') && !code.contains('('))
}

/// Python: a line whose code ends with `:` (`def f():`, `else:`) opens a block.
fn python_opens(line: &str) -> bool {
    before_hash(line, false).trim_end().ends_with(':')
}

/// YAML: a key with no value on its line (`a:`, `- a:`), or with only a block scalar mark
/// (`a: |`, `a: >-`), opens a block.
fn yaml_opens(line: &str) -> bool {
    let code = before_hash(line, true).trim_end();
    let mark = code.trim_end_matches(|c: char| c == '-' || c == '+' || c.is_ascii_digit());
    let key = match mark.strip_suffix(['|', '>']) {
        Some(key) => key.trim_end(),
        None => code,
    };
    key.ends_with(':')
}

const SHELL_STRINGS: &[&str] = &["string", "raw_string", "ansi_c_string"];
const COMMENT: &[&str] = &["comment"];

const SHELL: Typing = Typing {
    pairs: &[
        pair("[", "]", AUTO),
        pair("(", ")", AUTO | NEWLINE),
        pair("{", "}", AUTO | NEWLINE),
        pair("\"", "\"", AUTO | NO_STRING | NO_COMMENT),
        pair("'", "'", AUTO | NO_STRING | NO_COMMENT),
    ],
    close_before: "}])",
    line_comments: HASH,
    indent_nodes: &[("subshell", ")"), ("compound_statement", "}")],
    indent_after: Some(shell_opens),
    string_nodes: SHELL_STRINGS,
    comment_nodes: COMMENT,
    ..BASE
};

const ENV: Typing = Typing {
    pairs: &[
        pair("{", "}", AUTO),
        pair("\"", "\"", AUTO | NO_STRING | NO_COMMENT),
        pair("'", "'", AUTO | NO_STRING | NO_COMMENT),
    ],
    close_before: "}",
    line_comments: HASH,
    string_nodes: SHELL_STRINGS,
    comment_nodes: COMMENT,
    ..BASE
};

const C_PAIRS: &[Pair] = &[
    pair("{", "}", AUTO | NEWLINE),
    pair("[", "]", AUTO | NEWLINE),
    pair("(", ")", AUTO | NEWLINE),
    pair("\"", "\"", AUTO | NO_STRING),
    pair("`", "`", NO_STRING),
    pair("'", "'", AUTO | NO_STRING | NO_COMMENT),
    pair("/*", " */", AUTO | NO_STRING | NO_COMMENT),
];

const C_INDENT: &[(&str, &str)] = &[
    ("compound_statement", "}"),
    ("argument_list", ")"),
    ("parameter_list", ")"),
    ("initializer_list", "}"),
    ("field_declaration_list", "}"),
    ("enumerator_list", "}"),
    ("declaration_list", "}"),
    ("init_declarator", ""),
    ("concatenated_string", ""),
    ("string_literal", ""),
];

const C: Typing = Typing {
    pairs: C_PAIRS,
    close_before: CODE_CLOSE,
    line_comments: SLASHES,
    doc_comment: Some("/*"),
    indent_nodes: C_INDENT,
    string_nodes: &["string_literal", "char_literal", "system_lib_string"],
    comment_nodes: COMMENT,
    ..BASE
};

const CPP: Typing = Typing {
    line_comments: &["// ", "/// ", "//! "],
    string_nodes: &[
        "string_literal",
        "char_literal",
        "raw_string_literal",
        "system_lib_string",
    ],
    ..C
};

const CSS: Typing = Typing {
    pairs: &[
        pair("{", "}", AUTO | NEWLINE),
        pair("[", "]", AUTO | NEWLINE),
        pair("(", ")", AUTO | NEWLINE),
        pair("\"", "\"", AUTO | NO_STRING | NO_COMMENT),
        pair("'", "'", AUTO | NO_STRING | NO_COMMENT),
    ],
    close_before: CODE_CLOSE,
    word_chars: "#",
    indent_nodes: &[("block", "}")],
    string_nodes: &["string_value"],
    comment_nodes: COMMENT,
    ..BASE
};

const GIT_COMMIT: Typing = Typing {
    pairs: &[
        pair("(", ")", AUTO),
        pair("`", "`", AUTO),
        pair("\"", "\"", AUTO),
        pair("'", "'", AUTO),
        pair("{", "}", AUTO),
        pair("[", "]", AUTO),
    ],
    line_comments: HASH,
    comment_nodes: COMMENT,
    ..BASE
};

const GO_STRINGS: &[&str] = &[
    "interpreted_string_literal",
    "raw_string_literal",
    "rune_literal",
];

const GO: Typing = Typing {
    pairs: &[
        pair("{", "}", AUTO | NEWLINE),
        pair("[", "]", AUTO | NEWLINE),
        pair("(", ")", AUTO | NEWLINE),
        pair("\"", "\"", AUTO | NO_STRING | NO_COMMENT),
        pair("'", "'", AUTO | NO_STRING | NO_COMMENT),
        pair("`", "`", AUTO | NO_STRING | NO_COMMENT),
        pair("/*", " */", AUTO | NO_STRING | NO_COMMENT),
    ],
    close_before: CODE_CLOSE,
    line_comments: SLASHES,
    doc_comment: Some("/*"),
    hard_tabs: true,
    indent_nodes: &[("block", "}")],
    string_nodes: GO_STRINGS,
    comment_nodes: COMMENT,
    ..BASE
};

/// `go.mod` and `go.work`.
const GO_FILE: Typing = Typing {
    pairs: &[pair("(", ")", AUTO | NEWLINE)],
    close_before: ")",
    line_comments: SLASHES,
    string_nodes: GO_STRINGS,
    comment_nodes: COMMENT,
    ..BASE
};

const SCRIPT_STRINGS: &[&str] = &["string", "template_string"];
const SCRIPT_INDENT: &[(&str, &str)] = &[("lexical_declaration", ""), ("variable_declaration", "")];

const TYPESCRIPT: Typing = Typing {
    pairs: &[
        pair("{", "}", AUTO | NEWLINE),
        pair("[", "]", AUTO | NEWLINE),
        pair("(", ")", AUTO | NEWLINE),
        pair("<", ">", NEWLINE | NO_STRING | NO_COMMENT),
        pair("\"", "\"", AUTO | NO_STRING),
        pair("'", "'", AUTO | NO_STRING | NO_COMMENT),
        pair("`", "`", AUTO | NO_STRING),
        pair("/*", " */", AUTO | NO_STRING | NO_COMMENT),
    ],
    close_before: CODE_CLOSE,
    word_chars: "#$",
    line_comments: SLASHES,
    doc_comment: Some("/**"),
    indent_nodes: SCRIPT_INDENT,
    indent_brackets: ALL_BRACKETS,
    string_nodes: SCRIPT_STRINGS,
    comment_nodes: COMMENT,
    ..BASE
};

const TSX: Typing = Typing {
    jsx: true,
    ..TYPESCRIPT
};

const JAVASCRIPT: Typing = Typing {
    pairs: &[
        pair("{", "}", AUTO | NEWLINE),
        pair("[", "]", AUTO | NEWLINE),
        pair("(", ")", AUTO | NEWLINE),
        pair("<", ">", NEWLINE | NO_STRING | NO_COMMENT),
        pair("\"", "\"", AUTO | NO_STRING | NO_COMMENT),
        pair("'", "'", AUTO | NO_STRING | NO_COMMENT),
        pair("`", "`", AUTO | NO_STRING | NO_COMMENT),
        pair("/*", " */", AUTO | NO_STRING | NO_COMMENT),
    ],
    ..TSX
};

const JSDOC: Typing = Typing {
    pairs: &[pair("{", "}", AUTO), pair("[", "]", AUTO)],
    close_before: "]}",
    ..BASE
};

const JSONC: Typing = Typing {
    pairs: &[
        pair("{", "}", AUTO | NEWLINE),
        pair("[", "]", AUTO | NEWLINE),
        pair("(", ")", AUTO),
        pair("\"", "\"", AUTO | NO_STRING),
    ],
    close_before: ",]}",
    line_comments: SLASHES,
    indent_nodes: &[("object", "}"), ("array", "]")],
    string_nodes: &["string"],
    comment_nodes: COMMENT,
    ..BASE
};

const JSON: Typing = Typing {
    word_chars: "#",
    ..JSONC
};

const MARKDOWN: Typing = Typing {
    pairs: &[
        pair("{", "}", AUTO | NEWLINE),
        pair("[", "]", AUTO | NEWLINE),
        pair("(", ")", AUTO | NEWLINE),
        pair("<", ">", AUTO | NEWLINE),
        pair("\"", "\"", 0),
        pair("'", "'", 0),
        pair("`", "`", 0),
        pair("*", "*", 0),
        pair("~", "~", 0),
    ],
    close_before: CODE_CLOSE,
    lists: true,
    ..BASE
};

/// A Python string opening with a prefix, closed by its quote.
const fn py(open: &'static str, close: &'static str) -> Pair {
    pair(open, close, AUTO | NO_STRING | NO_COMMENT)
}

const PYTHON: Typing = Typing {
    pairs: &[
        py("f\"", "\""),
        py("f'", "'"),
        py("b\"", "\""),
        py("b'", "'"),
        py("u\"", "\""),
        py("u'", "'"),
        py("r\"", "\""),
        py("r'", "'"),
        py("rb\"", "\""),
        py("rb'", "'"),
        py("t\"", "\""),
        py("t'", "'"),
        pair("\"\"\"", "\"\"\"", AUTO | NO_STRING),
        pair("'''", "'''", AUTO | NO_STRING),
        pair("{", "}", AUTO | NEWLINE),
        pair("[", "]", AUTO | NEWLINE),
        pair("(", ")", AUTO | NEWLINE),
        pair("\"", "\"", AUTO | NO_STRING),
        pair("'", "'", AUTO | NO_STRING),
    ],
    close_before: CODE_CLOSE,
    line_comments: HASH,
    indent_brackets: ALL_BRACKETS,
    indent_after: Some(python_opens),
    string_nodes: &["string"],
    comment_nodes: COMMENT,
    ..BASE
};

const REGEX: Typing = Typing {
    pairs: &[
        pair("(", ")", AUTO),
        pair("{", "}", AUTO),
        pair("[", "]", AUTO),
    ],
    close_before: ")]}",
    ..BASE
};

/// A Rust raw string opening.
const fn raw(open: &'static str, close: &'static str) -> Pair {
    pair(open, close, AUTO | NEWLINE | NO_STRING | NO_COMMENT)
}

const RUST: Typing = Typing {
    pairs: &[
        pair("{", "}", AUTO | NEWLINE),
        raw("r#\"", "\"#"),
        raw("r##\"", "\"##"),
        raw("r###\"", "\"###"),
        pair("[", "]", AUTO | NEWLINE),
        pair("(", ")", AUTO | NEWLINE),
        pair("<", ">", NEWLINE | NO_STRING | NO_COMMENT),
        pair("\"", "\"", AUTO | NO_STRING),
        pair("`", "`", NO_STRING),
        pair("/*", " */", AUTO | NO_STRING | NO_COMMENT),
    ],
    close_before: CODE_CLOSE,
    line_comments: &["// ", "/// ", "//! "],
    doc_comment: Some("/*"),
    indent_nodes: &[("let_declaration", "")],
    indent_brackets: ALL_BRACKETS,
    string_nodes: &["string_literal", "raw_string_literal", "char_literal"],
    comment_nodes: &["line_comment", "block_comment"],
    ..BASE
};

const YAML: Typing = Typing {
    pairs: &[
        pair("{", "}", AUTO | NEWLINE),
        pair("[", "]", AUTO | NEWLINE),
        pair("\"", "\"", AUTO | NO_STRING),
        pair("'", "'", AUTO | NO_STRING),
    ],
    close_before: ",]}",
    line_comments: HASH,
    indent_after: Some(yaml_opens),
    string_nodes: &["double_quote_scalar", "single_quote_scalar"],
    comment_nodes: COMMENT,
    ..BASE
};

/// The typing facts of the language named `name` (a name of `LANGS`).
pub fn typing_for(name: &str) -> Option<&'static Typing> {
    Some(match name {
        "Shell Script" => &SHELL,
        "C" => &C,
        "C++" => &CPP,
        "CSS" => &CSS,
        "Diff" => &BASE,
        "Env" => &ENV,
        "Git Commit" => &GIT_COMMIT,
        "Go" => &GO,
        "Go Mod" | "Go Work" => &GO_FILE,
        "JavaScript" => &JAVASCRIPT,
        "JSDoc" => &JSDOC,
        "JSON" => &JSON,
        "JSONC" => &JSONC,
        "Markdown" => &MARKDOWN,
        "Markdown-Inline" => &BASE,
        "Python" => &PYTHON,
        "Regex" => &REGEX,
        "Rust" => &RUST,
        "TSX" => &TSX,
        "TypeScript" => &TYPESCRIPT,
        "YAML" => &YAML,
        _ => return None,
    })
}

#[cfg(test)]
#[path = "code_langs_tests.rs"]
mod tests;
