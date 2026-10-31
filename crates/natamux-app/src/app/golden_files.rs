//! Golden files of the MIT move (`docs/plan-mit-license.md`, 7.6): facts the old code gives on
//! our own fixtures, kept as JSON under `tests/golden/<set>/<os>/`, so the new code must give the
//! same. A `golden_*` test (ignored) writes a file to `$NATAMUX_GOLDEN_OUT/<set>/` on CI (the
//! `Tool: golden` run); the normal test compares the same rows with the committed file.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::path::PathBuf;

use serde_json::Value;

/// The folder of this OS: the files differ where keys differ (Alt or Ctrl for word moves).
fn os() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

/// The JSON of `rows`, one row per line, so a changed row shows as one changed line.
fn lines(rows: &[Value]) -> String {
    let rows: Vec<String> = rows.iter().map(Value::to_string).collect();
    format!("[\n{}\n]\n", rows.join(",\n"))
}

/// Writes `rows` as `<set>/<name>.json` under `$NATAMUX_GOLDEN_OUT`.
pub fn write(set: &str, name: &str, rows: &[Value]) {
    let Ok(out) = std::env::var("NATAMUX_GOLDEN_OUT") else {
        panic!("set NATAMUX_GOLDEN_OUT to write the golden files");
    };
    let dir = PathBuf::from(out).join(set);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{name}.json")), lines(rows)).unwrap();
}

/// Fails unless `rows` equal the committed `<set>/<os>/<name>.json`, and names the first row
/// that differs.
pub fn check(set: &str, name: &str, rows: &[Value]) {
    if let Some(problem) = differs(set, name, rows) {
        panic!("{problem}");
    }
}

/// What differs between `rows` and the committed `<set>/<os>/<name>.json`: the first row that
/// differs, or the number of rows. None when they are equal.
pub fn differs(set: &str, name: &str, rows: &[Value]) -> Option<String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(set)
        .join(os())
        .join(format!("{name}.json"));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let golden: Vec<Value> = serde_json::from_str(&text).unwrap();
    if let Some(i) = golden.iter().zip(rows).position(|(old, new)| old != new) {
        return Some(format!(
            "{}: row {i} differs\n golden: {}\n now:    {}",
            path.display(),
            golden[i],
            rows[i]
        ));
    }
    (golden.len() != rows.len()).then(|| {
        format!(
            "{}: the number of rows differs, golden {} now {}",
            path.display(),
            golden.len(),
            rows.len()
        )
    })
}

mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn writes_one_row_per_line() {
        let rows = [json!([0, 3, "keyword"]), json!({"path": "a.rs"})];
        assert_eq!(
            lines(&rows),
            "[\n[0,3,\"keyword\"],\n{\"path\":\"a.rs\"}\n]\n"
        );
        let back: Vec<Value> = serde_json::from_str(&lines(&rows)).unwrap();
        assert_eq!(back, rows);
    }
}
