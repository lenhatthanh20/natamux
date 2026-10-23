//! 17 `templates`, 18 `attachments` and 19 `issue_link`.

use hyper::Method;
use natamux_contract::daemon::{Body, Daemon};
use natamux_contract::transcript::{Allowed, Transcript};
use serde_json::{Value, json};

use crate::{both, call, clean_text, data_file, get, kept, post};

#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn templates() {
    both("templates", &[], |d: Daemon, mut t: Transcript| async move {
        let template = json!({
            "name": " Contract bug ", "title": "Bug: something", "description": "Steps:\n1. Open it",
            "priority": "urgent", "labels": [" bug "],
        });
        t.note("save", kept(&post(&d, "/templates", template).await));
        let again = json!({ "name": "Contract bug", "title": "Bug again" });
        t.note("same name", kept(&post(&d, "/templates", again).await));
        post(&d, "/templates", json!({ "name": "contract Alpha", "title": "A" })).await;
        t.note("list", kept(&get(&d, "/templates").await));
        t.note("file", json!(clean_text(&data_file(&d, "templates/contract-bug.md").await)));
        let task = json!({ "title": "CT-1701: From a template" });
        t.note("task", kept(&post(&d, "/templates/contract-bug/tasks", task).await));
        let missing = json!({ "title": "CT-1702: x" });
        t.note("unknown template", kept(&post(&d, "/templates/nope/tasks", missing).await));
        t.note("bad template id", kept(&post(&d, "/templates/Bad_Id/tasks", json!({ "title": "x" })).await));
        t.note("bad title", kept(&post(&d, "/templates/contract-bug/tasks", json!({ "title": "" })).await));
        t.note("bad template", kept(&post(&d, "/templates", json!({ "name": "", "labels": [1] })).await));
        t
    })
    .await;
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |sum, (i, b)| sum | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn file(name: &str, bytes: &[u8]) -> Value {
    json!({ "name": name, "data": base64(bytes) })
}

#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn attachments() {
    let allowed = [Allowed {
        step: "over 1 MB",
        reason: "nginx of the web container refuses bodies over 1 MB before the Node daemon sees them; the Rust daemon is reached directly",
    }];
    both(
        "attachments",
        &allowed,
        |d: Daemon, mut t: Transcript| async move {
            let png = b"\x89PNG\r\n\x1a\nfake";
            let files = json!([
                file("../Shot 1.png", png),
                file("shot-1.PNG", png),
                file("logo.svg", b"<svg/>"),
                file("page.html", b"<script>1</script>"),
                file("report.pdf", b"%PDF-1.4"),
                file(".hidden", b"x"),
            ]);
            let task = json!({ "title": "CT-1801: With files", "attachments": files });
            t.note("create", kept(&post(&d, "/tasks", task).await));
            t.note("list", kept(&get(&d, "/tasks/CT-1801/attachments").await));
            for name in [
                "Shot-1.png",
                "shot-1-2.PNG",
                "logo.svg",
                "page.html",
                "report.pdf",
                "hidden",
            ] {
                let answer = get(&d, &format!("/tasks/CT-1801/attachments/{name}")).await;
                t.note(
                    &format!("get {name}"),
                    json!([answer.status, answer.headers, answer.body]),
                );
            }
            t.note(
                "missing",
                kept(&get(&d, "/tasks/CT-1801/attachments/none.png").await),
            );
            t.note(
                "unsafe name",
                kept(&get(&d, "/tasks/CT-1801/attachments/a%20b.png").await),
            );
            t.note(
                "unknown task",
                kept(&get(&d, "/tasks/CT-99999/attachments").await),
            );
            let many: Vec<Value> = (0..21).map(|i| file(&format!("f{i}.txt"), b"x")).collect();
            let refused = json!({ "title": "CT-1802: Too many", "attachments": many });
            t.note("too many", kept(&post(&d, "/tasks", refused).await));
            t.note("not made", kept(&get(&d, "/tasks/CT-1802").await));
            let big = vec![b'a'; 1_200_000];
            let large =
                json!({ "title": "CT-1803: Large", "attachments": [file("big.txt", &big)] });
            t.note("over 1 MB", kept(&post(&d, "/tasks", large).await));
            t.note("log", kept(&get(&d, "/tasks/CT-1801/events").await));
            t.note(
                "file",
                json!(clean_text(&data_file(&d, "tasks/CT-1801.md").await)),
            );
            t
        },
    )
    .await;
}

#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn issue_link() {
    both("issue_link", &[], |d: Daemon, mut t: Transcript| async move {
        post(&d, "/tasks", json!({ "title": "CT-1901: Linked" })).await;
        let put = |path: &str, body: Value| {
            let d = d.clone();
            let path = path.to_string();
            async move { kept(&call(&d, Method::PUT, &path, Body::Json(body)).await) }
        };
        let link = json!({ "provider": " github ", "key": "#7", "url": " https://github.com/o/r/issues/7\n" });
        t.note("https", put("/tasks/CT-1901/issue", link).await);
        let plain = json!({ "provider": "jira", "key": "AB-1", "url": "http://jira.local/AB-1" });
        t.note("http", put("/tasks/CT-1901/issue", plain).await);
        let other = json!({ "provider": "x", "key": "y", "url": "vscode://file/etc" });
        t.note("other scheme", put("/tasks/CT-1901/issue", other).await);
        t.note("bad url", put("/tasks/CT-1901/issue", json!({ "provider": "x", "key": "y", "url": "https://" })).await);
        t.note("unknown task", put("/tasks/CT-99999/issue", json!({ "provider": "x", "key": "y", "url": "https://a.b" })).await);
        t.note("log", kept(&get(&d, "/tasks/CT-1901/events").await));
        t
    })
    .await;
}
