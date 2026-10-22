//! The options of `natamux`, read as Node's `util.parseArgs` reads them (strict, positionals
//! allowed), with its error texts (Node 22, lib/internal/util/parse_args and lib/internal/errors),
//! so a wrong command line reads as it did with the TypeScript command.

use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Flag,
    Text,
    /// A text option that may come more than once.
    Texts,
}

struct Spec {
    name: &'static str,
    short: Option<char>,
    kind: Kind,
}

const fn spec(name: &'static str, kind: Kind) -> Spec {
    Spec {
        name,
        short: None,
        kind,
    }
}

/// `parseOptions` in packages/cli/src/command.ts.
const OPTIONS: &[Spec] = &[
    Spec {
        name: "help",
        short: Some('h'),
        kind: Kind::Flag,
    },
    spec("json", Kind::Flag),
    spec("description", Kind::Text),
    spec("priority", Kind::Text),
    spec("label", Kind::Texts),
    spec("agent", Kind::Text),
    spec("depends-on", Kind::Texts),
    spec("state", Kind::Text),
    spec("source", Kind::Text),
    spec("payload", Kind::Text),
    spec("yes", Kind::Flag),
    spec("delete-branch", Kind::Flag),
    spec("title", Kind::Text),
    spec("prompt", Kind::Text),
    spec("task", Kind::Text),
    spec("base", Kind::Text),
    spec("context", Kind::Text),
    spec("stage", Kind::Text),
    spec("to", Kind::Text),
    spec("reason", Kind::Text),
    spec("issue", Kind::Text),
    spec("url", Kind::Text),
    spec("provider", Kind::Text),
    spec("repo", Kind::Text),
    spec("parent", Kind::Text),
    spec("attach", Kind::Texts),
];

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Parsed {
    flags: Vec<&'static str>,
    texts: HashMap<&'static str, Vec<String>>,
    pub positionals: Vec<String>,
}

impl Parsed {
    pub fn flag(&self, name: &str) -> bool {
        self.flags.contains(&name)
    }

    /// The last value of an option.
    pub fn text(&self, name: &str) -> Option<&str> {
        self.texts.get(name)?.last().map(String::as_str)
    }

    /// Every value of an option that may repeat, None when it was not given.
    pub fn texts(&self, name: &str) -> Option<&[String]> {
        self.texts.get(name).map(Vec::as_slice)
    }
}

fn unknown(raw: &str) -> String {
    let quoted = serde_json::to_string(raw).unwrap_or_default();
    // Node's text has no closing quote.
    format!(
        "Unknown option '{raw}'. To specify a positional argument starting with a '-', place it at the end of the command after '--', as in '-- {quoted}"
    )
}

/// "-h, --help" or "--title", as Node names an option in its errors.
fn short_and_long(spec: &Spec) -> String {
    match spec.short {
        Some(short) => format!("-{short}, --{}", spec.name),
        None => format!("--{}", spec.name),
    }
}

/// A value of more than one character that starts with "-" looks like a forgotten option value.
fn looks_like_option(value: &str) -> bool {
    value.len() > 1 && value.starts_with('-')
}

pub fn parse_args(argv: &[String]) -> Result<Parsed, String> {
    let mut parsed = Parsed::default();
    let mut args = argv.iter();
    while let Some(arg) = args.next() {
        if arg == "--" {
            parsed.positionals.extend(args.by_ref().cloned());
            break;
        }
        if let Some(long) = arg.strip_prefix("--") {
            let (name, inline) = match long.split_once('=') {
                Some((name, value)) => (name, Some(value.to_string())),
                None => (long, None),
            };
            let raw = format!("--{name}");
            let Some(spec) = OPTIONS.iter().find(|spec| spec.name == name) else {
                return Err(unknown(&raw));
            };
            if spec.kind == Kind::Flag {
                if inline.is_some() {
                    return Err(format!(
                        "Option '{}' does not take an argument",
                        short_and_long(spec)
                    ));
                }
                parsed.flags.push(spec.name);
                continue;
            }
            let value = match inline {
                Some(value) => value,
                None => {
                    let Some(next) = args.next() else {
                        return Err(format!(
                            "Option '{} <value>' argument missing",
                            short_and_long(spec)
                        ));
                    };
                    if looks_like_option(next) {
                        return Err(format!(
                            "Option '{raw}' argument is ambiguous.\nDid you forget to specify the option argument for '{raw}'?\nTo specify an option argument starting with a dash use '{raw}=-XYZ'."
                        ));
                    }
                    next.clone()
                }
            };
            let values = parsed.texts.entry(spec.name).or_default();
            if spec.kind == Kind::Text {
                values.clear();
            }
            values.push(value);
            continue;
        }
        if let Some(shorts) = arg.strip_prefix('-')
            && !shorts.is_empty()
        {
            for short in shorts.chars() {
                match OPTIONS.iter().find(|spec| spec.short == Some(short)) {
                    Some(spec) => parsed.flags.push(spec.name),
                    None => return Err(unknown(&format!("-{short}"))),
                }
            }
            continue;
        }
        parsed.positionals.push(arg.clone());
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Parsed, String> {
        parse_args(&args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn reads_flags_texts_repeats_and_positionals() {
        let parsed = parse(&[
            "task",
            "create",
            "Fix",
            "--json",
            "--label",
            "a",
            "--label=b",
            "--title",
            "x",
            "--title",
            "y",
            "-h",
        ])
        .unwrap();
        assert_eq!(parsed.positionals, ["task", "create", "Fix"]);
        assert!(parsed.flag("json") && parsed.flag("help"));
        assert_eq!(parsed.texts("label").unwrap(), ["a", "b"]);
        assert_eq!(parsed.text("title"), Some("y"));
        assert_eq!(parsed.text("agent"), None);
    }

    #[test]
    fn takes_everything_after_a_double_dash_and_a_lone_dash_as_positionals() {
        let parsed = parse(&["ask", "-", "--", "--json", "-x"]).unwrap();
        assert_eq!(parsed.positionals, ["ask", "-", "--json", "-x"]);
        assert!(!parsed.flag("json"));
    }

    #[test]
    fn a_value_may_start_with_a_dash_only_inline() {
        assert_eq!(parse(&["--title=-x"]).unwrap().text("title"), Some("-x"));
        assert_eq!(parse(&["--title", "-"]).unwrap().text("title"), Some("-"));
        assert_eq!(
            parse(&["--title", "--json"]).unwrap_err(),
            "Option '--title' argument is ambiguous.\nDid you forget to specify the option argument for '--title'?\nTo specify an option argument starting with a dash use '--title=-XYZ'."
        );
    }

    #[test]
    fn has_nodes_error_texts() {
        assert_eq!(
            parse(&["task", "list", "--colour=red"]).unwrap_err(),
            "Unknown option '--colour'. To specify a positional argument starting with a '-', place it at the end of the command after '--', as in '-- \"--colour\""
        );
        assert_eq!(
            parse(&["-x"]).unwrap_err(),
            "Unknown option '-x'. To specify a positional argument starting with a '-', place it at the end of the command after '--', as in '-- \"-x\""
        );
        assert_eq!(
            parse(&["--title"]).unwrap_err(),
            "Option '--title <value>' argument missing"
        );
        assert_eq!(
            parse(&["--json=1"]).unwrap_err(),
            "Option '--json' does not take an argument"
        );
        assert_eq!(
            parse(&["--help=1"]).unwrap_err(),
            "Option '-h, --help' does not take an argument"
        );
    }
}
