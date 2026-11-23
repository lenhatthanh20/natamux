use std::fs::{create_dir_all, write};

use natamux_git::test_support::TempDir;
use serde_json::json;

use super::*;
use crate::projects::repo_trust::{Approval, MAX_EXTRA};

/// Project "default" at `<dir>/folder`, and a worktree folder beside it.
fn setup() -> (TempDir, Arc<Repos>) {
    let base = TempDir::new("trust-worktree");
    let data = base.path().join("data");
    create_dir_all(&data).unwrap();
    create_dir_all(base.path().join("folder")).unwrap();
    create_dir_all(base.path().join("worktree")).unwrap();
    let path = base.path().join("folder").to_string_lossy().to_string();
    let list = json!([{ "id": "default", "name": "default", "path": path }]);
    write(data.join("repos.json"), list.to_string()).unwrap();
    let repos = Repos::new(&data, Arc::new(|| 0_i64));
    (base, repos)
}

#[test]
fn reads_an_approval_stored_before_worktree_digests() {
    let text = r#"{"path":"/p","hash":"ab","setup":"cd","agents":"ef"}"#;
    let approval: Approval = serde_json::from_str(text).unwrap();
    assert!(approval.extra.is_empty());
}

#[test]
fn keeps_at_most_32_worktree_digests_and_drops_the_oldest() {
    let text = r#"{"path":"/p","hash":"main","setup":"","agents":""}"#;
    let mut approval: Approval = serde_json::from_str(text).unwrap();
    for index in 0..40 {
        approval.accept(&format!("d{index}"));
    }
    approval.accept("main");
    approval.accept("d39");
    assert_eq!(approval.extra.len(), MAX_EXTRA);
    assert_eq!(approval.extra.first().map(String::as_str), Some("d8"));
    assert_eq!(approval.extra.last().map(String::as_str), Some("d39"));
}

#[test]
fn approves_a_worktree_digest_beside_the_project_and_refuses_a_stale_one() {
    let (base, repos) = setup();
    let worktree = base.path().join("worktree");
    write(base.path().join("folder/.mcp.json"), "{}\n").unwrap();
    let main = repos.trust_view("default").unwrap().hash;
    repos.approve("default", &main).unwrap();
    write(worktree.join(".mcp.json"), "{ }\n").unwrap();

    let state = repos.is_trusted_in_task(None, Some(&worktree));
    let TrustState::Changed { digest, files } = state else {
        panic!("{state:?}");
    };
    assert_eq!(files, [".mcp.json"]);

    write(worktree.join(".mcp.json"), "{  }\n").unwrap();
    let stale = repos.approve_in("default", Some(&worktree), &digest);
    assert_eq!(stale.unwrap_err().status_and_code(), (409, "conflict"));

    let now = repos.trust_view_in("default", Some(&worktree)).unwrap();
    assert!(!now.trusted);
    repos
        .approve_in("default", Some(&worktree), &now.hash)
        .unwrap();
    assert!(repos.is_trusted_in_task(None, Some(&worktree)).is_trusted());
    // The project folder keeps its own approval.
    assert!(repos.is_trusted(None));
}
