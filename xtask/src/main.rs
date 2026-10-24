//! Repo tasks, run as `cargo xtask <task>`.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Output};

/// The L4 crates of docs/plan-rust-migration.md. Only they may use the UI framework.
const UI_CRATES: &[&str] = &["natamux-app", "natamux-cli", "natamux-update"];

const USAGE: &str = "usage: cargo xtask <task>
  ci      fmt, clippy, tests, layers, colour literals
  ci [--no-app] [--all-errors]
          --no-app: natamux-app left out of clippy and tests
          --all-errors: every step runs after a failure, for CI snapshots
  layers  fail when a crate below L4 uses gpui
  colours fail on a colour literal in crates/natamux-app/src outside the theme files";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["ci", ref flags @ ..] => match ci_options(flags) {
            Some(options) => ci(options),
            None => Err(USAGE.to_string()),
        },
        ["layers"] => layers(),
        ["colours"] => colours(),
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::FAILURE;
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("xtask: {error}");
            ExitCode::FAILURE
        }
    }
}

#[derive(Clone, Copy, Default)]
struct CiOptions {
    no_app: bool,
    all_errors: bool,
}

fn ci_options(flags: &[&str]) -> Option<CiOptions> {
    let mut options = CiOptions::default();
    for flag in flags {
        match *flag {
            "--no-app" => options.no_app = true,
            "--all-errors" => options.all_errors = true,
            _ => return None,
        }
    }
    Some(options)
}

fn ci(options: CiOptions) -> Result<(), String> {
    let mut failed = Vec::new();
    for args in ci_cargo_args(options) {
        if let Err(error) = run(&args) {
            if !options.all_errors {
                return Err(error);
            }
            failed.push(error);
        }
    }
    for check in [layers, colours] {
        if let Err(error) = check() {
            if !options.all_errors {
                return Err(error);
            }
            failed.push(error);
        }
    }
    if failed.is_empty() {
        Ok(())
    } else {
        Err(failed.join("\n"))
    }
}

/// The cargo runs of `ci`: fmt, clippy, test. `no_app` leaves out natamux-app, which pulls in the
/// whole GPUI graph, for the daemon snapshots. `all_errors` shows every error of a snapshot in one
/// run, as nothing builds on the owner's Mac.
fn ci_cargo_args(options: CiOptions) -> Vec<Vec<&'static str>> {
    let exclude: &[&str] = if options.no_app {
        &["--exclude", "natamux-app"]
    } else {
        &[]
    };
    let (keep_going, no_fail_fast): (&[&str], &[&str]) = if options.all_errors {
        (&["--keep-going"], &["--no-fail-fast"])
    } else {
        (&[], &[])
    };
    vec![
        vec!["fmt", "--all", "--check"],
        [
            &["clippy", "--workspace"][..],
            exclude,
            keep_going,
            &["--all-targets", "--locked", "--", "-D", "warnings"],
        ]
        .concat(),
        [
            &["test", "--workspace"][..],
            exclude,
            no_fail_fast,
            &["--locked"],
        ]
        .concat(),
    ]
}

/// Views read colours from the theme, never as literals (docs/ui-spec.md, 2.3).
const COLOUR_ROOT: &str = "crates/natamux-app/src";

fn colours() -> Result<(), String> {
    let mut files = Vec::new();
    rust_files(Path::new(COLOUR_ROOT), &mut files)?;
    let mut bad = Vec::new();
    for file in files.iter().filter(|file| !colour_literals_allowed(file)) {
        let source = std::fs::read_to_string(file)
            .map_err(|error| format!("cannot read {}: {error}", file.display()))?;
        for line in colour_literal_lines(&source) {
            bad.push(format!("{}:{line}", file.display()));
        }
    }
    if bad.is_empty() {
        println!("colours: ok");
        return Ok(());
    }
    Err(format!(
        "colour literals outside components/colors.rs and the contrast test data (read the theme instead):\n  {}",
        bad.join("\n  ")
    ))
}

fn rust_files(folder: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(folder)
        .map_err(|error| format!("cannot read {}: {error}", folder.display()))?;
    for entry in entries {
        let path = entry.map_err(|error| error.to_string())?.path();
        if path.is_dir() {
            rust_files(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
    out.sort();
    Ok(())
}

/// The only files of `COLOUR_ROOT` that may hold colour literals: the colour module, and the
/// contrast test's hex data. The theme itself is JSON in `assets/`.
const COLOUR_FILES: &[&str] = &["components/colors.rs", "components/theme_contrast.rs"];

fn colour_literals_allowed(path: &Path) -> bool {
    path.strip_prefix(COLOUR_ROOT)
        .is_ok_and(|inner| COLOUR_FILES.iter().any(|file| inner == Path::new(file)))
}

/// 1-based numbers of the lines with `rgb(`, `rgba(`, `hsla(`, a hex integer of 6 or 8 digits
/// such as `0x1b2a4aff`, or a quoted `#` hex colour. Comment lines are skipped.
fn colour_literal_lines(source: &str) -> Vec<usize> {
    source
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim_start().starts_with("//"))
        .filter(|(_, line)| has_colour_literal(line))
        .map(|(index, _)| index + 1)
        .collect()
}

fn has_colour_literal(line: &str) -> bool {
    ["rgb(", "rgba(", "hsla("]
        .iter()
        .any(|call| call_at_word_start(line, call))
        || hex_literal(line, "0x", false)
        || hex_literal(line, "\"#", true)
}

/// `rgb(` counts, `to_rgb(` and `srgb(` do not.
fn call_at_word_start(line: &str, call: &str) -> bool {
    line.match_indices(call).any(|(at, _)| {
        !line[..at]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_')
    })
}

/// A `0x` run of 6 or 8 hex digits, or a `"#` run of 3, 4, 6 or 8 closed by a quote.
fn hex_literal(line: &str, start: &str, quoted: bool) -> bool {
    line.match_indices(start).any(|(at, _)| {
        let rest = &line[at + start.len()..];
        let digits = rest.chars().take_while(char::is_ascii_hexdigit).count();
        let after = rest[digits..].chars().next();
        let ends_ok = if quoted {
            after == Some('"')
        } else {
            !after.is_some_and(|c| c.is_alphanumeric() || c == '_')
        };
        let sizes: &[usize] = if quoted { &[3, 4, 6, 8] } else { &[6, 8] };
        sizes.contains(&digits) && ends_ok
    })
}

fn layers() -> Result<(), String> {
    let members = output(&tree_args(&["--depth", "0"]))?;
    let tree = cargo(&tree_args(&["--invert", "gpui"]))?;
    let tree = if tree.status.success() {
        text(&tree.stdout)
    } else if gpui_not_in_graph(&text(&tree.stderr)) {
        String::new()
    } else {
        return Err(format!("cargo tree failed:\n{}", text(&tree.stderr)));
    };
    let bad = crates_breaking_layers(&members, &tree);
    if bad.is_empty() {
        println!("layers: ok");
        return Ok(());
    }
    Err(format!(
        "only {} may use gpui, but these crates reach it: {}. See the path with `cargo tree --invert gpui --package <crate>`.",
        UI_CRATES.join(", "),
        bad.join(", ")
    ))
}

/// Build dependencies and every target count: a crate can use gpui on one OS only.
fn tree_args<'a>(extra: &[&'a str]) -> Vec<&'a str> {
    let base = [
        "tree",
        "--workspace",
        "--locked",
        "--prefix",
        "none",
        "--edges",
        "normal,build",
        "--target",
        "all",
    ];
    [&base[..], extra].concat()
}

/// Workspace crates that appear in `cargo tree --invert gpui` and are not L4 crates.
/// Both inputs are `cargo tree --prefix none` output: one `name vX.Y.Z (source)` per line.
fn crates_breaking_layers(members: &str, gpui_tree: &str) -> Vec<String> {
    let members: Vec<&str> = names(members).collect();
    let mut bad: Vec<String> = names(gpui_tree)
        .filter(|name| members.contains(name) && !UI_CRATES.contains(name))
        .map(String::from)
        .collect();
    bad.sort();
    bad.dedup();
    bad
}

fn names(tree: &str) -> impl Iterator<Item = &str> {
    tree.lines()
        .filter_map(|line| line.split_whitespace().next())
}

/// cargo tree exits with an error when gpui is in no dependency graph at all.
fn gpui_not_in_graph(stderr: &str) -> bool {
    stderr.contains("did not match any packages")
}

fn run(args: &[&str]) -> Result<(), String> {
    eprintln!("$ cargo {}", args.join(" "));
    let status = Command::new(cargo_bin())
        .args(args)
        .status()
        .map_err(|error| format!("cannot start cargo: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("cargo {} failed", args.join(" ")))
    }
}

fn output(args: &[&str]) -> Result<String, String> {
    let out = cargo(args)?;
    if out.status.success() {
        Ok(text(&out.stdout))
    } else {
        Err(format!(
            "cargo {} failed:\n{}",
            args.join(" "),
            text(&out.stderr)
        ))
    }
}

fn cargo(args: &[&str]) -> Result<Output, String> {
    Command::new(cargo_bin())
        .args(args)
        .output()
        .map_err(|error| format!("cannot start cargo: {error}"))
}

fn cargo_bin() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string())
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MEMBERS: &str = "natamux-app v0.1.0 (/w/crates/natamux-app)

natamux-core v0.1.0 (/w/crates/natamux-core)

natamux-store v0.1.0 (/w/crates/natamux-store)

xtask v0.0.0 (/w/xtask)
";

    fn lines(no_app: bool, all_errors: bool) -> Vec<String> {
        ci_cargo_args(CiOptions { no_app, all_errors })
            .iter()
            .map(|args| args.join(" "))
            .collect()
    }

    #[test]
    fn ci_runs_the_whole_workspace_by_default() {
        assert_eq!(
            lines(false, false),
            [
                "fmt --all --check",
                "clippy --workspace --all-targets --locked -- -D warnings",
                "test --workspace --locked",
            ]
        );
    }

    #[test]
    fn no_app_excludes_it_from_clippy_and_test_only() {
        assert_eq!(
            lines(true, false),
            [
                "fmt --all --check",
                "clippy --workspace --exclude natamux-app --all-targets --locked -- -D warnings",
                "test --workspace --exclude natamux-app --locked",
            ]
        );
    }

    #[test]
    fn all_errors_keeps_clippy_and_the_tests_going() {
        assert_eq!(
            lines(true, true),
            [
                "fmt --all --check",
                "clippy --workspace --exclude natamux-app --keep-going --all-targets --locked -- -D \
                 warnings",
                "test --workspace --exclude natamux-app --no-fail-fast --locked",
            ]
        );
    }

    #[test]
    fn reads_the_ci_flags_and_refuses_others() {
        assert!(
            ci_options(&["--no-app", "--all-errors"]).is_some_and(|o| o.no_app && o.all_errors)
        );
        assert!(ci_options(&[]).is_some_and(|o| !o.no_app && !o.all_errors));
        assert!(ci_options(&["--app"]).is_none());
    }

    #[test]
    fn finds_colour_literals_by_line() {
        let source = "let a = 1;
let b = rgb(0x1b2a4a);
// rgb(0x1b2a4a) in a comment
let c = gpui::rgba(5);
let d = 0x1b2a4aff;
let e = \"#1B2A4A\";
let f = hsla(0.1, 0.2, 0.3, 1.0);
";
        assert_eq!(colour_literal_lines(source), [2, 4, 5, 6, 7]);
    }

    #[test]
    fn ignores_text_that_only_looks_like_a_colour() {
        for line in [
            "let id = \"#tasks\";",
            "let n = 0x7fff;",
            "let big = 0x1b2a4a5b6c;",
            "let x = to_rgb(c);",
            "let m = 0x10ffff_u32;",
            "let s = \"#12345\";",
        ] {
            assert!(!has_colour_literal(line), "{line}");
        }
    }

    #[test]
    fn allows_literals_only_in_colors_rs_and_the_contrast_test() {
        let allowed = |file: &str| colour_literals_allowed(&Path::new(COLOUR_ROOT).join(file));
        assert!(allowed("components/colors.rs"));
        assert!(allowed("components/theme_contrast.rs"));
        // A view named after the theme gets no pass.
        assert!(!allowed("theme/palette.rs"));
        assert!(!allowed("app/theme_mode.rs"));
        assert!(!allowed("components/theme_card.rs"));
        assert!(!allowed("components/chips.rs"));
        assert!(!allowed("sidebar/colors.rs"));
    }

    #[test]
    fn passes_when_only_ui_crates_reach_gpui() {
        let tree = "gpui v0.2.2 (https://github.com/zed-industries/zed?rev=abc#abc)
ui v0.1.0 (https://github.com/zed-industries/zed?rev=abc#abc)
natamux-app v0.1.0 (/w/crates/natamux-app)
natamux-app v0.1.0 (/w/crates/natamux-app) (*)
";
        assert!(crates_breaking_layers(MEMBERS, tree).is_empty());
    }

    #[test]
    fn fails_for_a_lower_crate_that_reaches_gpui_through_another_crate() {
        let tree = "gpui v0.2.2 (https://github.com/zed-industries/zed?rev=abc#abc)
theme v0.1.0 (https://github.com/zed-industries/zed?rev=abc#abc)
natamux-store v0.1.0 (/w/crates/natamux-store)
natamux-core v0.1.0 (/w/crates/natamux-core)
natamux-store v0.1.0 (/w/crates/natamux-store) (*)
natamux-app v0.1.0 (/w/crates/natamux-app)
";
        assert_eq!(
            crates_breaking_layers(MEMBERS, tree),
            ["natamux-core", "natamux-store"]
        );
    }

    #[test]
    fn ignores_crates_outside_the_workspace() {
        let tree = "gpui v0.2.2 (/w/vendor/gpui)
serde_derive v1.0.228 (proc-macro)
";
        assert!(crates_breaking_layers(MEMBERS, tree).is_empty());
    }

    #[test]
    fn passes_when_gpui_is_in_no_graph() {
        assert!(crates_breaking_layers(MEMBERS, "").is_empty());
        assert!(gpui_not_in_graph(
            "error: package ID specification `gpui` did not match any packages\n"
        ));
    }

    #[test]
    fn other_cargo_errors_are_not_a_pass() {
        assert!(!gpui_not_in_graph(
            "error: the lock file /w/Cargo.lock needs to be updated but --locked was passed\n"
        ));
    }
}
