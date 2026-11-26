//! The app and UI code never fetch from the network by accident: no `img` from an `http` literal
//! and no HTTP client given to gpui. Natamux promises no telemetry and no outside requests.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

const CLIENT_CALLS: [&str; 2] = ["with_http_client(", "set_http_client("];

fn rust_files(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
}

/// True when `line` gives `img(` a string literal that starts with `http`, or sets an HTTP client.
fn fetches(line: &str) -> bool {
    let remote_image = line.split("img(").skip(1).any(|rest| {
        let rest = rest
            .trim_start()
            .trim_start_matches('r')
            .trim_start_matches('#');
        rest.starts_with("\"http")
    });
    remote_image || CLIENT_CALLS.iter().any(|call| line.contains(call))
}

#[test]
fn the_check_finds_a_remote_image_and_an_http_client() {
    assert!(fetches(r#"    img("https://example.com/a.png")"#));
    assert!(fetches(r#"img( "http://x/a.png").size_4()"#));
    assert!(fetches(r##"img(r#"https://x"#)"##));
    assert!(fetches("cx.with_http_client(client)"));
    assert!(fetches("app.set_http_client(client);"));
    assert!(!fetches("img(SharedString::from(path))"));
    assert!(!fetches(r#"img("icons/agents/claude.svg")"#));
    assert!(!fetches(r#"let url = "https://github.com";"#));
}

#[test]
fn app_and_ui_code_never_fetch_from_the_network() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rust_files(&manifest.join("src"), &mut files);
    rust_files(&manifest.join("../natamux-ui/src"), &mut files);
    assert!(!files.is_empty());
    let mut found = Vec::new();
    for file in files {
        let name = file
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if name.ends_with("_tests.rs") {
            continue;
        }
        let text = std::fs::read_to_string(&file).unwrap();
        for (line_no, line) in text.lines().enumerate() {
            if fetches(line) {
                found.push(format!("{}:{}", file.display(), line_no + 1));
            }
        }
    }
    assert!(
        found.is_empty(),
        "no remote image or HTTP client: {found:?}"
    );
}
