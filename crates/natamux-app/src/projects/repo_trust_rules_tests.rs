#![allow(clippy::unwrap_used)]

use natamux_proto::projects::repo_trust::{RepoCommand, SetupScript};

use super::*;

fn view(trusted: bool, commands: usize, scripts: usize) -> RepoTrustView {
    RepoTrustView {
        trusted,
        hash: "ab12".into(),
        commands: (0..commands)
            .map(|index| RepoCommand {
                place: format!("Stage s{index}: check"),
                text: "cargo test".into(),
            })
            .collect(),
        setup: (0..scripts)
            .map(|_| SetupScript {
                file: ".natamux-setup".into(),
                text: "pnpm install\n".into(),
            })
            .collect(),
        agent_files: Vec::new(),
        changed: Vec::new(),
        unverifiable: None,
    }
}

/// From JSON, so a new field of `Repo` with a default does not break the test.
fn repo(id: &str, name: &str) -> Repo {
    serde_json::from_value(serde_json::json!({ "id": id, "name": name, "path": "/work/app" }))
        .unwrap()
}

#[test]
fn an_untrusted_project_with_commands_or_a_setup_script_is_reviewed() {
    assert!(needs_review(&view(false, 1, 0)));
    assert!(needs_review(&view(false, 0, 1)));
}

#[test]
fn an_untrusted_project_with_only_an_agent_config_file_is_reviewed() {
    let mut view = view(false, 0, 0);
    view.agent_files.push(SetupScript {
        file: ".mcp.json".into(),
        text: "{}\n".into(),
    });
    assert!(needs_review(&view));
}

#[test]
fn a_trusted_project_or_one_with_nothing_to_run_is_not_reviewed() {
    assert!(!needs_review(&view(true, 2, 1)));
    assert!(!needs_review(&view(false, 0, 0)));
}

#[test]
fn the_name_comes_from_the_project_list_or_falls_back_to_the_id() {
    let repos = [repo("shop", "My Shop")];
    assert_eq!(repo_name(&repos, "shop"), "My Shop");
    assert_eq!(repo_name(&repos, "new-one"), "new-one");
}

#[test]
fn the_title_names_the_project() {
    assert_eq!(title("My Shop"), "Approve the commands of My Shop?");
}

#[test]
fn names_the_changed_files_or_why_the_folder_cannot_be_checked() {
    assert_eq!(changed_note(&view(false, 1, 0)), None);
    let changed = RepoTrustView {
        changed: vec![".mcp.json".into(), ".claude/agents/x.md".into()],
        ..view(false, 0, 0)
    };
    assert_eq!(
        changed_note(&changed).as_deref(),
        Some("Changed since the last approval: .mcp.json, .claude/agents/x.md")
    );
    assert!(needs_review(&changed));
    let link = RepoTrustView {
        unverifiable: Some(".mcp.json is a link".into()),
        ..view(false, 0, 0)
    };
    assert_eq!(
        changed_note(&link).as_deref(),
        Some("This folder cannot be approved: .mcp.json is a link")
    );
    assert!(needs_review(&link));
}
