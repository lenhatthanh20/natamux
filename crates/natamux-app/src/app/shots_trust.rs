//! T-0375's `--shot` states: Start agent refused because the worktree's agent config changed (its
//! error toast offers Review config), the trust dialog of that task's worktree, and the CI approval
//! of a push that also sends commits no remote has and changes a workflow file.

use gpui::{App, Context, Entity, Window};

use super::dashboard::Dashboard;
use super::fake_daemon;
use super::options::Shot;
use super::shots::REVIEWED;
use super::window::Root;
use crate::components::confirm::Confirm;
use crate::projects::repo_trust_dialog::{review_task, rules::repo_name};
use crate::stages::ci_approval_rules::{CiAsk, ask_description, ask_rows};

/// The project of the reviewed task; the fake gives its worktree a changed `.mcp.json`.
const PROJECT: &str = "natamux";
const AGENT: &str = "claude-code";
/// As in the CI dialog (`tasks/details_ci.rs`).
const SHOWN_FILES: usize = 12;

pub fn is_trust_shot(shot: Shot) -> bool {
    matches!(
        shot,
        Shot::ReviewConfigToast | Shot::RepoTrustTask | Shot::CiApprovalCommits
    )
}

/// The fake's data of the shot, before the window watches it.
pub fn prepare(shot: Shot) {
    if shot == Shot::ReviewConfigToast {
        fake_daemon::shared().refuse_start(REVIEWED);
    }
}

pub fn enter(
    shot: Shot,
    dashboard: &Entity<Dashboard>,
    window: &mut Window,
    cx: &mut Context<Root>,
) {
    match shot {
        // An error toast with an action stays until it is dismissed.
        Shot::ReviewConfigToast => {
            let started = dashboard.update(cx, |d, cx| d.start_agent(REVIEWED, AGENT, cx));
            started.detach();
        }
        Shot::RepoTrustTask => {
            let name = repo_name(&dashboard.read(cx).data.repos, PROJECT);
            if let Some(source) = dashboard.read(cx).source().cloned() {
                review_task(&source, PROJECT, REVIEWED, &name, window, cx);
            }
        }
        Shot::CiApprovalCommits => ci_approval(window, cx),
        _ => {}
    }
}

/// The dialog as `tasks/details_ci.rs` opens it. Its opener is private to the details panel, so
/// the shot builds the same dialog from the same rules.
fn ci_approval(window: &mut Window, cx: &mut App) {
    let ask = sample_ask();
    let rows = ask_rows(&ask, SHOWN_FILES)
        .into_iter()
        .map(|(id, file)| (id.into(), file.into()))
        .collect();
    Confirm::new(
        format!("Approve the CI push of {}?", ask.stage),
        ask_description(&ask),
        "Approve and push",
        |_, _| {},
    )
    .rows(rows)
    .open(window, cx);
}

fn paths(list: &[&str]) -> Vec<String> {
    list.iter().map(|path| (*path).to_owned()).collect()
}

/// Two commits no remote has, one of them changes a GitHub workflow.
fn sample_ask() -> CiAsk {
    CiAsk {
        stage: "check".into(),
        tree: "9f2c41be0d7a3c5e8b1f6a2d4c7e9b0a1d3f5c7e".into(),
        branch: "natamux/t-0143-rust-migration-phase-1".into(),
        remote: "origin".into(),
        files: paths(&["Cargo.toml", "src/cart.ts", "src/checkout.ts"]),
        not_sent: paths(&[".env"]),
        not_sent_count: 1,
        new_commits: Some(2),
        commit_files: paths(&[".github/workflows/rust.yml", "src/checkout.ts"]),
        commit_files_count: 2,
        workflows_changed: Some(true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ci_shot_shows_the_warning_and_the_commit_files() {
        let rows = ask_rows(&sample_ask(), SHOWN_FILES);
        assert_eq!(rows[0].0, "ci-warning");
        assert!(
            rows.iter()
                .any(|row| row.0 == "commit-file-.github/workflows/rust.yml")
        );
        assert!(ask_description(&sample_ask()).contains("It also sends 2 commits"));
    }
}
