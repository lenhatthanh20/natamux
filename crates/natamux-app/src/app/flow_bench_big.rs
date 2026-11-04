//! The flows of the big views: typing in a 5,000-line file in the editor tab, and opening a
//! 10,000-line diff. They run last, as they add a file and a change to the fake's reviewed task.

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result, anyhow};
use gpui::{AsyncApp, Focusable as _, Keystroke, WindowHandle};
use natamux_proto::workspaces::changes::{ChangeStatus, FileDiff};

use super::super::fake_daemon;
use super::super::shots::REVIEWED;
use super::super::window::Root;
use super::{POLL, RUNS, SETTLE, any_frame, drawn_when, in_window, record, sleep};
use crate::files::file_editor_body::FileEditorBody;

const EDITOR_FILE: &str = "bench/big_file.ts";
const EDITOR_LINES: usize = 5_000;
/// One key each; the flow times each of them.
const TYPED: &str = "abcdefghijklmnopqrst";
const DIFF_FILE: &str = "bench/big_diff.ts";
const DIFF_LINES: usize = 10_000;
/// The file is read, parsed and drawn within this.
const OPEN_WITHIN: Duration = Duration::from_secs(15);

/// Per key: the key to the end of the first frame drawn after it, in the editor tab with a
/// 5,000-line TypeScript file, the cursor at its start.
pub(super) async fn editor_type(handle: WindowHandle<Root>, cx: &mut AsyncApp) -> Result<()> {
    let text = (1..=EDITOR_LINES).fold(String::new(), |mut text, line| {
        let _ = writeln!(
            text,
            "export const item{line} = {{ id: {line}, name: 'Item {line}' }}"
        );
        text
    });
    fake_daemon::shared().put_workspace_file(REVIEWED, EDITOR_FILE, text);
    handle.update(cx, |root, _, cx| {
        root.dashboard().update(cx, |dashboard, cx| {
            dashboard.open(REVIEWED, cx);
            dashboard.set_details_open(false, cx);
            dashboard.open_file(REVIEWED, EDITOR_FILE, cx);
        });
    })?;
    let body = parsed_body(handle, cx).await?;
    in_window(handle, cx, |window, cx| {
        let focus = body.read(cx).editor().focus_handle(cx);
        window.focus(&focus, cx);
    })?;
    sleep(SETTLE, cx).await;
    let mut took = Vec::new();
    for key in TYPED.chars() {
        let keystroke = Keystroke::parse(&key.to_string()).map_err(|error| anyhow!("{error}"))?;
        let started = Instant::now();
        in_window(handle, cx, |window, cx| {
            window.dispatch_keystroke(keystroke, cx);
        })?;
        let (_, drawn) = drawn_when(handle, cx, any_frame(0)).await?;
        took.push(drawn.saturating_duration_since(started));
        sleep(SETTLE, cx).await;
    }
    // Only keys that reached the file count.
    let shown = in_window(handle, cx, |_, cx| body.read(cx).text(cx))?;
    if !shown.contains(TYPED) {
        return Err(anyhow!("the keys did not reach the file"));
    }
    for one in took {
        record("editor_type_5000", one, cx);
    }
    Ok(())
}

/// The editor of the file, once it has the text and its language is parsed.
async fn parsed_body(
    handle: WindowHandle<Root>,
    cx: &mut AsyncApp,
) -> Result<gpui::Entity<FileEditorBody>> {
    let started = Instant::now();
    while started.elapsed() < OPEN_WITHIN {
        let body = handle.update(cx, |root, _, cx| {
            let editor = root.workspace_view().read(cx).file_editor(EDITOR_FILE, cx);
            let body = editor.and_then(|editor| editor.read(cx).body().cloned());
            body.filter(|body| body.read(cx).parsed(cx))
        })?;
        if let Some(body) = body {
            return Ok(body);
        }
        sleep(POLL, cx).await;
    }
    Err(anyhow!("the file did not open in the editor tab"))
}

/// A 10,000-line diff picked in the Changes tab, to the end of the first frame that draws it.
/// The fake answers the read at once, so the texts arrive with the click. Each run picks another
/// file first, so the big one is built again.
pub(super) async fn diff_open(handle: WindowHandle<Root>, cx: &mut AsyncApp) -> Result<()> {
    let fake = fake_daemon::shared();
    let ids = handle.update(cx, |root, _, cx| {
        let tasks = &root.dashboard().read(cx).data.tasks;
        tasks.iter().map(|task| task.id.clone()).collect::<Vec<_>>()
    })?;
    let (id, other) = ids
        .into_iter()
        .find_map(|id| {
            let list = fake.list_changes(&id, false).ok()?;
            // A text diff that shows as one: no image, no binary file.
            let text = list.iter().find_map(|entry| {
                let diff = fake.file_diff(&id, &entry.path).ok()?;
                let both = diff.original.is_some() && diff.modified.is_some();
                (both && !diff.binary && diff.original != diff.modified).then(|| entry.path.clone())
            })?;
            Some((id, text))
        })
        .ok_or_else(|| anyhow!("no fake task has a changed text file"))?;
    let (original, modified) = big_diff();
    fake.put_change(
        &id,
        FileDiff {
            path: DIFF_FILE.to_owned(),
            status: ChangeStatus::Modified,
            original: Some(original),
            modified: Some(modified),
            binary: false,
            too_large: false,
        },
    );
    handle.update(cx, |root, _, cx| {
        root.dashboard()
            .update(cx, |dashboard, cx| dashboard.open_changes(&id, false, cx));
    })?;
    sleep(Duration::from_secs(1), cx).await;
    let changes = handle.update(cx, |root, _, cx| {
        root.workspace_view().read(cx).changes_view().clone()
    })?;
    // The view already read its list; hand it the one that has the big change.
    let list = fake.list_changes(&id, false).map_err(|error| anyhow!("{error:?}"))?;
    handle.update(cx, |_, _, cx| {
        changes.update(cx, |changes, cx| changes.set_list(&id, Some(list), None, cx))
    })?;
    sleep(SETTLE, cx).await;
    for _ in 0..RUNS {
        let (view, path) = (changes.clone(), other.clone());
        handle.update(cx, |_, _, cx| {
            view.update(cx, |c, cx| c.shot_pick(&path, cx))
        })?;
        let (view, path) = (changes.clone(), other.clone());
        drawn_when(handle, cx, move |_, cx| view.read(cx).shot_shown(&path, cx))
            .await
            .context("the other file was not shown")?;
        sleep(SETTLE, cx).await;
        let started = Instant::now();
        handle.update(cx, |_, _, cx| {
            changes.update(cx, |changes, cx| changes.shot_pick(DIFF_FILE, cx))
        })?;
        let view = changes.clone();
        let (_, drawn) = drawn_when(handle, cx, move |_, cx| {
            view.read(cx).shot_shown(DIFF_FILE, cx)
        })
        .await
        .context("the big diff was not shown")?;
        record(
            "diff_open_10000",
            drawn.saturating_duration_since(started),
            cx,
        );
        sleep(SETTLE, cx).await;
    }
    Ok(())
}

/// 10,000 lines before and after: every 100th line changed and a line added after every 250th,
/// so the diff has about 140 hunks.
fn big_diff() -> (String, String) {
    let before: Vec<String> = (1..=DIFF_LINES)
        .map(|n| format!("export const price{n} = {}", n * 3))
        .collect();
    let mut after = Vec::with_capacity(DIFF_LINES + DIFF_LINES / 250);
    for (index, line) in before.iter().enumerate() {
        let n = index + 1;
        if n % 100 == 0 {
            after.push(format!("export const price{n} = {}", n * 5));
        } else {
            after.push(line.clone());
        }
        if n % 250 == 0 {
            after.push(format!("export const extra{n} = {n}"));
        }
    }
    (before.join("\n") + "\n", after.join("\n") + "\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_big_diff_has_both_sides_and_its_hunks() {
        let (before, after) = big_diff();
        assert_eq!(before.lines().count(), DIFF_LINES);
        assert_eq!(after.lines().count(), DIFF_LINES + DIFF_LINES / 250);
        let changed = before
            .lines()
            .zip(after.lines())
            .filter(|(a, b)| a != b)
            .count();
        assert!(changed >= DIFF_LINES / 100);
    }
}
