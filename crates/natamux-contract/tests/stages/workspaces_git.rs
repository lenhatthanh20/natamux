//! 42 `changes_and_files`, 43 `commit_and_draft` and 44 `rollback`, in the `workspaces` module
//! (the CI filter `workspaces::`).

use natamux_contract::daemon::Daemon;
use natamux_contract::events::Events;
use natamux_contract::transcript::Transcript;
use serde_json::{Value, json};

use super::{alias, dirty_count, make_task, open_workspace, retire, save, save_with};
use crate::{both, get, kept, post, put, stage_events, twin};

/// Notes the answer of a read, and the headers of an image (the route sets its own). JSON
/// answers leave their headers out: nginx of the web container adds its own to Node's.
async fn note_get(d: &Daemon, t: &mut Transcript, step: &str, path: &str) {
    let answer = get(d, path).await;
    t.note(step, kept(&answer));
    let json = answer
        .headers
        .iter()
        .any(|(name, value)| name == "content-type" && value.starts_with("application/json"));
    if !json {
        t.note(&format!("{step}: headers"), json!(answer.headers));
    }
}

/// Commits what is staged with a name git knows, in the container of the daemon.
async fn commit_staged(d: &Daemon, ws: &str, message: &str) {
    let args = format!(
        "-c user.name=Contract -c user.email=contract@example.com commit -q -m '{message}'"
    );
    d.git_in(ws, &args).await;
}

/// The changed files of a worktree: files written, `GET /changes` (`fresh=1`), the diff of a
/// file, the bytes of an image (and the refusals), the tree, `GET` and `PUT /file` (version, 409
/// changed on disk, 413, `.git` and outside refused, 400 without `If-Match`), `workspace.dirty`
/// after the save, and `GET /diffstats`.
#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn changes_and_files() {
    both(
        "changes_and_files",
        4201,
        &[],
        |d: Daemon, id, mut t: Transcript| async move {
            make_task(&d, 4201, "Files").await;
            let ws = open_workspace(&d, &id).await;
            // A committed image, then changed: both sides of it exist.
            d.exec_in(&format!("cd '{ws}' && printf 'png v1' > img.png"))
                .await;
            d.git_in(&ws, "add img.png").await;
            commit_staged(&d, &ws, "Add an image").await;
            let write = "printf 'png v2!' > img.png && printf 'more\\n' >> README.md \
                         && mkdir -p src && printf 'one\\n' > src/new.txt";
            d.exec_in(&format!("cd '{ws}' && {write}")).await;
            let base = format!("/tasks/{id}");
            let image = format!("{base}/changes/image?path=img.png");
            let reads = [
                ("changes", format!("{base}/changes?fresh=1")),
                ("diff", format!("{base}/changes/file?path=README.md")),
                ("image original", format!("{image}&side=original")),
                ("image modified", format!("{image}&side=modified")),
                (
                    "diff of an unchanged file",
                    format!("{base}/changes/file?path=Missing.txt"),
                ),
                ("bad side", format!("{image}&side=middle")),
                (
                    "not an image",
                    format!("{base}/changes/image?path=README.md&side=modified"),
                ),
                ("tree", format!("{base}/tree?path=")),
                ("tree of src", format!("{base}/tree?path=src")),
                ("tree of a file", format!("{base}/tree?path=README.md")),
                ("tree outside", format!("{base}/tree?path=../..")),
                ("tree of .git", format!("{base}/tree?path=.git")),
            ];
            for (step, path) in &reads {
                note_get(&d, &mut t, step, path).await;
            }
            file_saves(&d, &mut t, &id).await;
            let stats = get(&d, "/diffstats").await;
            t.note(
                "diff stats",
                json!([stats.status, stats.body["stats"][&id]]),
            );
            retire(&d, &id).await;
            t
        },
    )
    .await;
}

/// `GET` and `PUT /file`: the version read, `workspace.dirty` after the save, 409 changed on
/// disk, 413, `.git` and outside refused, 400 without `If-Match`.
async fn file_saves(d: &Daemon, t: &mut Transcript, id: &str) {
    let base = format!("/tasks/{id}");
    let file = get(d, &format!("{base}/file?path=README.md")).await;
    t.note("file", kept(&file));
    let old = file.body["version"]
        .as_str()
        .unwrap_or_default()
        .to_string();

    let mut events = Events::open(d, Some(&d.token)).await;
    let seen = dirty_count(&events.messages, id);
    t.note(
        "save",
        kept(&save_with(d, id, "README.md", "edited\n", &old).await),
    );
    let more = |m: &[Value]| dirty_count(m, id) > seen;
    t.note("dirty after the save", json!(events.until(more).await));
    let stale = save_with(d, id, "README.md", "again\n", &old).await;
    t.note("changed on disk", kept(&stale));
    let big = "x".repeat(1_000_001);
    t.note("too large", kept(&save(d, id, "README.md", &big).await));
    t.note(
        "save in .git",
        kept(&save_with(d, id, ".git", "x", "v").await),
    );
    t.note(
        "save outside",
        kept(&save_with(d, id, "../out.txt", "x", "v").await),
    );
    let url = format!("{base}/file?path=README.md");
    t.note(
        "no If-Match",
        kept(&put(d, &url, json!({ "text": "x" })).await),
    );
}

/// `POST /commit-message` (the fake agent's `Add fixed message`, and the simple draft for a task
/// whose title has `[no-draft]`), `POST /commit` (`git.committed`), 409 with nothing to commit,
/// 400 with an empty message, and `GET /graph`.
#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn commit_and_draft() {
    both(
        "commit_and_draft",
        4301,
        &[],
        |d: Daemon, id, mut t: Transcript| async move {
            let plain = twin(&d, 4302);
            alias(&mut t, &plain, "<task 2>");
            make_task(&d, 4301, "Commit").await;
            make_task(&d, 4302, "[no-draft] Commit").await;
            let ws = open_workspace(&d, &id).await;
            let other = open_workspace(&d, &plain).await;
            for dir in [&ws, &other] {
                d.exec_in(&format!("cd '{dir}' && printf 'feature\\n' > feature.txt"))
                    .await;
            }
            let draft = post(&d, &format!("/tasks/{id}/commit-message"), json!({})).await;
            t.note("draft", kept(&draft));
            let simple = post(&d, &format!("/tasks/{plain}/commit-message"), json!({})).await;
            t.note("simple draft", kept(&simple));
            let path = format!("/tasks/{id}/commit");
            t.note(
                "commit",
                kept(&post(&d, &path, json!({ "message": "Add feature" })).await),
            );
            t.note(
                "nothing to commit",
                kept(&post(&d, &path, json!({ "message": "Again" })).await),
            );
            t.note(
                "empty message",
                kept(&post(&d, &path, json!({ "message": "  " })).await),
            );
            t.note(
                "graph",
                kept(&get(&d, &format!("/tasks/{id}/graph?fresh=1")).await),
            );
            t.note("events", stage_events(&d, &id).await);
            retire(&d, &id).await;
            retire(&d, &plain).await;
            t
        },
    )
    .await;
}

/// A changed and an untracked file are rolled back (and the files are as they were in the last
/// commit); one undo brings them back; a second undo is 409 `Nothing to undo.`; a file that did
/// not change is 404; a bad list is 400.
#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn rollback() {
    both(
        "rollback",
        4401,
        &[],
        |d: Daemon, id, mut t: Transcript| async move {
            make_task(&d, 4401, "Rollback").await;
            let ws = open_workspace(&d, &id).await;
            let files =
                format!("cd '{ws}' && cat README.md; test -e new.txt && echo new || echo none");
            d.exec_in(&format!(
                "cd '{ws}' && printf 'edit\\n' >> README.md && printf 'new\\n' > new.txt"
            ))
            .await;
            let edited = d.exec_in(&files).await;
            let paths = json!({ "paths": ["README.md", "new.txt"] });
            let path = format!("/tasks/{id}/rollback");
            t.note("rollback", kept(&post(&d, &path, paths).await));
            t.note("files after", json!(d.exec_in(&files).await));
            let undo = format!("/tasks/{id}/rollback/undo");
            t.note("undo", kept(&post(&d, &undo, json!({})).await));
            t.note("files restored", json!(d.exec_in(&files).await == edited));
            t.note("second undo", kept(&post(&d, &undo, json!({})).await));
            let unchanged = json!({ "paths": ["Missing.txt"] });
            t.note("not changed", kept(&post(&d, &path, unchanged).await));
            t.note(
                "empty list",
                kept(&post(&d, &path, json!({ "paths": [] })).await),
            );
            t.note(
                "bad list",
                kept(&post(&d, &path, json!({ "paths": "README.md" })).await),
            );
            retire(&d, &id).await;
            t
        },
    )
    .await;
}
