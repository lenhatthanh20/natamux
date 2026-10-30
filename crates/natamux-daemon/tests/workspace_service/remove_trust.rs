//! A removed worktree leaves the trusted folders of Cursor and Copilot (repo trust).

use natamux_agents::trust::{trust_paths, trust_workspace};

use super::*;

#[test]
fn takes_the_removed_worktree_out_of_the_agents_trusted_folders() {
    let fx = Fx::new();
    let paths = trust_paths(|_| None, &fx.root.path().join("home"));
    fs::create_dir_all(paths.copilot_config.parent().unwrap()).unwrap();
    fs::write(
        &paths.copilot_config,
        r#"{"trustedFolders":["/elsewhere"]}"#,
    )
    .unwrap();
    let kept = paths.clone();
    let trusting = fx.build(move |options| options.trust_paths = Some(kept));
    let task = fx.task("Fix the bug");
    let created = trusting.create(&task.id, None, None).unwrap();
    let workspace = created.workspace.unwrap();
    for agent in ["cursor", "copilot"] {
        trust_workspace(agent, &workspace, &paths);
    }
    // Parsed, as JSON escapes the backslashes of a Windows path.
    let before: Value =
        serde_json::from_str(&fs::read_to_string(&paths.copilot_config).unwrap()).unwrap();
    assert!(
        before["trustedFolders"]
            .as_array()
            .unwrap()
            .contains(&json!(workspace))
    );

    trusting.remove(&task.id, false).unwrap();

    let after = fs::read_to_string(&paths.copilot_config).unwrap();
    let config: Value = serde_json::from_str(&after).unwrap();
    assert_eq!(config["trustedFolders"], json!(["/elsewhere"]));
    let markers = fs::read_dir(&paths.cursor_projects)
        .unwrap()
        .flat_map(|dir| fs::read_dir(dir.unwrap().path()).unwrap())
        .count();
    assert_eq!(markers, 0);
}
