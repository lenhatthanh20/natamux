//! A removed worktree leaves the trusted folders of Cursor and Copilot (repo trust).

use natamux_agents::trust::{TrustPaths, trust_paths, trust_workspace};

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

/// Two open tasks in the project folder, with the folder trusted by Copilot.
fn two_in_the_main_checkout() -> (Fx, Arc<WorkspaceService>, TrustPaths, [Task; 2], String) {
    let fx = Fx::new();
    let paths = trust_paths(|_| None, &fx.root.path().join("home"));
    fs::create_dir_all(paths.copilot_config.parent().unwrap()).unwrap();
    fs::write(&paths.copilot_config, r#"{"trustedFolders":[]}"#).unwrap();
    let kept = paths.clone();
    let trusting = fx.build(move |options| options.trust_paths = Some(kept));
    let first = trusting
        .use_main_checkout(&fx.task("Fix the bug").id)
        .unwrap();
    let second = trusting
        .use_main_checkout(&fx.task("Fix another bug").id)
        .unwrap();
    let folder = first.workspace.clone().unwrap();
    trust_workspace("copilot", &folder, &paths);
    (fx, trusting, paths, [first, second], folder)
}

fn trusted_folders(paths: &TrustPaths) -> Value {
    let text = fs::read_to_string(&paths.copilot_config).unwrap();
    serde_json::from_str::<Value>(&text).unwrap()["trustedFolders"].clone()
}

#[test]
fn takes_the_main_checkout_out_of_the_trusted_folders_with_its_last_task() {
    let (_fx, trusting, paths, [first, second], folder) = two_in_the_main_checkout();
    assert_eq!(trusted_folders(&paths), json!([folder]));

    // Another task still works in the folder: the trust stays.
    trusting.remove(&first.id, false).unwrap();
    assert_eq!(trusted_folders(&paths), json!([folder]));

    trusting.remove(&second.id, false).unwrap();
    assert_eq!(trusted_folders(&paths), json!([]));
}
