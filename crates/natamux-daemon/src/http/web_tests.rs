use super::web::{WebAssets, clean_path};
use hyper::Method;
use hyper::header::{CACHE_CONTROL, CONTENT_TYPE};

#[test]
fn refuses_paths_outside_the_web_root() {
    assert_eq!(clean_path("/assets/app.js"), Some("assets/app.js".into()));
    assert_eq!(clean_path("/%2e%2e/secret"), None);
    assert_eq!(clean_path("/assets/%2e%2e/secret"), None);
    assert_eq!(clean_path("/assets/%2fsecret"), None);
    assert_eq!(clean_path("/assets/\\secret"), None);
    assert_eq!(clean_path("/assets/%zz"), None);
    assert_eq!(clean_path("/C:secret"), None);
}

#[test]
fn serves_embedded_files_and_html_deep_links_without_a_token() {
    let web = WebAssets::for_test(&[
        ("index.html", b"<h1>dashboard</h1>"),
        ("assets/app.js", b"console.log('ready')"),
    ]);
    let file = web
        .answer(&Method::GET, "/assets/app.js", "*/*")
        .expect("asset response");
    let home = web
        .answer(&Method::GET, "/", "*/*")
        .expect("dashboard page");
    assert_eq!(home.headers()[CONTENT_TYPE], "text/html; charset=utf-8");
    assert!(home.headers().contains_key("content-security-policy"));
    assert_eq!(
        file.headers()[CONTENT_TYPE],
        "text/javascript; charset=utf-8"
    );
    assert_eq!(
        file.headers()[CACHE_CONTROL],
        "public, max-age=31536000, immutable"
    );
    assert!(
        web.answer(&Method::GET, "/tasks/T-1", "application/json")
            .is_none()
    );
    let page = web
        .answer(&Method::GET, "/w/T-1/changes", "text/html")
        .expect("deep link");
    assert_eq!(page.headers()[CONTENT_TYPE], "text/html; charset=utf-8");
    assert_eq!(page.headers()[CACHE_CONTROL], "no-cache");
    assert!(web.answer(&Method::POST, "/", "text/html").is_none());
    assert!(
        web.answer(&Method::GET, "/api/missing", "text/html")
            .is_none()
    );
    assert!(
        web.answer(&Method::GET, "/a%70i/tasks", "text/html")
            .is_none()
    );
    assert!(web.answer(&Method::GET, "/tasks", "text/html").is_none());
    assert!(
        web.answer(&Method::GET, "/t%61sks/T-1", "text/html")
            .is_none()
    );
    assert!(
        web.answer(&Method::GET, "/%2e%2e/secret", "text/html")
            .is_none()
    );
    assert!(
        web.answer(&Method::GET, "/assets/%2fsecret", "text/html")
            .is_none()
    );
}

#[test]
fn external_web_dir_takes_precedence_over_embedded_files() {
    let root = std::env::temp_dir().join(format!("natamux-web-test-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp web dir");
    std::fs::write(root.join("index.html"), "external").expect("temp index");
    let web = WebAssets::for_test(&[("index.html", b"embedded")]).with_dir(root.clone());
    assert_eq!(web.bytes_for("/index.html"), Some(b"external".to_vec()));
    assert_eq!(web.bytes_for("/missing.js"), None);
    std::fs::remove_file(root.join("index.html")).expect("remove temp index");
    std::fs::remove_dir(root).expect("remove temp dir");
}

#[cfg(unix)]
#[test]
fn external_web_dir_does_not_follow_a_link_outside_it() {
    use std::os::unix::fs::symlink;

    let root = std::env::temp_dir().join(format!("natamux-web-link-test-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp web dir");
    symlink("/etc/passwd", root.join("outside.txt")).expect("link");
    let web = WebAssets::for_test(&[]).with_dir(root.clone());
    assert_eq!(web.bytes_for("/outside.txt"), None);
    std::fs::remove_file(root.join("outside.txt")).expect("remove link");
    std::fs::remove_dir(root).expect("remove temp dir");
}
