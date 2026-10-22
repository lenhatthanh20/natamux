//! Repo tasks, run as `cargo xtask <task>`.

use std::process::{Command, ExitCode, Output};

/// The L4 crates of docs/plan-rust-migration.md. Only they may use the UI framework.
const UI_CRATES: &[&str] = &["natamux-app", "natamux-cli", "natamux-update"];

const USAGE: &str = "usage: cargo xtask <task>
  ci      fmt, clippy, tests, layers
  layers  fail when a crate below L4 uses gpui";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["ci"] => ci(),
        ["layers"] => layers(),
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

fn ci() -> Result<(), String> {
    run(&["fmt", "--all", "--check"])?;
    run(&[
        "clippy",
        "--workspace",
        "--all-targets",
        "--locked",
        "--",
        "-D",
        "warnings",
    ])?;
    run(&["test", "--workspace", "--locked"])?;
    layers()
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
