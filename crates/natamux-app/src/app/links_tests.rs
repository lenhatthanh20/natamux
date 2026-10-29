use super::*;

#[test]
fn opens_a_web_link_outside_the_app() {
    for href in [
        "https://example.com",
        "http://example.com",
        "mailto:a@b.c",
        "HTTPS://example.com",
        "MailTo:a@b.c",
    ] {
        assert!(is_safe_link(href), "{href}");
    }
}

#[test]
fn refuses_any_other_link_target() {
    for href in [
        "javascript:alert(1)",
        "JavaScript:alert(1)",
        " javascript:alert(1)",
        "java\tscript:alert(1)",
        "data:text/html,hi",
        "vbscript:msgbox",
        "file:///etc/passwd",
        "ssh://host",
        "zed://settings",
        "myapp://run",
        "plan.md",
        "#sec",
        "",
        "//example.com",
        "1http://x",
    ] {
        assert!(!is_safe_link(href), "{href}");
    }
}

fn rust_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// A direct `open_url` would skip the scheme check.
#[test]
fn only_links_rs_calls_open_url() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files(&src, &mut files);
    let call = ["open", "_url("].concat();
    let found: Vec<_> = files
        .iter()
        .filter(|path| !path.ends_with("app/links.rs") && !path.ends_with("app/links_tests.rs"))
        .filter(|path| std::fs::read_to_string(path).unwrap().contains(&call))
        .collect();
    assert!(found.is_empty(), "use open_link instead: {found:?}");
}
