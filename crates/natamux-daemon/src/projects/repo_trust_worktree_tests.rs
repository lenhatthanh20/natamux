use std::fs::{create_dir_all, write};

use natamux_core::security::{SecurityMode, StrictSettings, force};
use natamux_git::test_support::{TempDir, run_git};
use serde_json::json;

use super::*;
use crate::projects::repo_trust::{Approval, MAX_EXTRA};
use crate::projects::settings_parse::SETTINGS_FILE;

const CHECKED: &str = r#"{"stages":[{"id":"context","kind":"context","gate":"manual"},{"id":"test","kind":"agent","gate":"auto","check":"pnpm test"}]}"#;

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
    // Personal mode trusts every worktree of an approved project: worktree digests are Strict's.
    let _strict = force(SecurityMode::Strict(StrictSettings::default()));
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

/// Writes `record` as the approval of "default".
fn store(base: &TempDir, record: serde_json::Value) {
    let file = base.path().join("data/repo-trust.json");
    write(file, json!({ "default": record }).to_string()).unwrap();
}

// Records written by older versions have no `by_user` key: one with approved agent files means the
// user approved the project; one without (a settings save, the first start) does not.
#[test]
fn an_approval_stored_by_an_older_version_counts_in_personal_mode() {
    let (base, repos) = setup();
    write(base.path().join("folder/.mcp.json"), "{}\n").unwrap();
    assert!(!repos.is_trusted(None));
    let path = base.path().join("folder").to_string_lossy().to_string();

    store(
        &base,
        json!({ "path": path, "hash": "old", "setup": "old" }),
    );
    assert!(!repos.is_trusted(None));

    let record = json!({ "path": path, "hash": "old", "setup": "old", "agents": "old" });
    store(&base, record);
    assert!(repos.is_trusted(None));
    assert!(repos.is_trusted_in(None, &base.path().join("worktree")));
    let _strict = force(SecurityMode::Strict(StrictSettings::default()));
    assert!(!repos.is_trusted(None));
}

#[test]
fn an_approval_for_another_project_path_does_not_count() {
    let (base, repos) = setup();
    write(base.path().join("folder/.mcp.json"), "{}\n").unwrap();
    let hash = repos.trust_view("default").unwrap().hash;

    // A record that would cover the project, but of another path.
    let path = "/elsewhere";
    let record = json!({ "path": path, "hash": hash, "setup": "", "agents": "x", "by_user": true });
    store(&base, record);

    assert!(!repos.is_trusted(None));
    let view = repos.trust_view("default").unwrap();
    assert!(!view.trusted && !view.covered);
}

// A record of another path must not be the "before" of a settings save either.
#[test]
fn a_settings_save_does_not_take_over_a_record_of_another_path() {
    let (base, repos) = setup();
    let folder = base.path().join("folder");
    let body: serde_json::Value = serde_json::from_str(CHECKED).unwrap();
    write(folder.join(SETTINGS_FILE), CHECKED).unwrap();
    write(folder.join(".mcp.json"), "{}\n").unwrap();
    let mut record = repos.current("default", None).unwrap().approval();
    record.path = "/elsewhere".into();
    record.by_user = true;
    let stored = |all: &mut std::collections::BTreeMap<String, Approval>| {
        all.insert("default".into(), record);
    };
    repos.trust.change(stored).unwrap();

    repos.approve_saved("default", &body).unwrap();

    let view = repos.trust_view("default").unwrap();
    assert!(!view.trusted && !view.covered);
}

#[test]
fn covered_only_after_an_approval_and_always_in_strict_mode() {
    let (base, repos) = setup();
    let view = repos.trust_view("default").unwrap();
    assert!(
        view.trusted && !view.covered,
        "nothing to approve still asks once"
    );
    write(base.path().join("folder/.mcp.json"), "{}\n").unwrap();
    let view = repos.trust_view("default").unwrap();
    assert!(!view.trusted && !view.covered);

    repos.approve("default", &view.hash).unwrap();

    let worktree = base.path().join("worktree");
    write(worktree.join(".mcp.json"), "{ }\n").unwrap();
    let in_task = repos.trust_view_in("default", Some(&worktree)).unwrap();
    assert!(in_task.trusted && in_task.covered && in_task.changed.is_empty());
    let view = repos.trust_view("default").unwrap();
    assert!(view.trusted && view.covered && view.changed.is_empty());
    let _strict = force(SecurityMode::Strict(StrictSettings::default()));
    let (other, _) = setup();
    write(other.path().join("folder/.mcp.json"), "{}\n").unwrap();
    let strict = Repos::new(&other.path().join("data"), Arc::new(|| 0_i64));
    let view = strict.trust_view("default").unwrap();
    assert!(!view.trusted && view.covered);
}

// A task branch that drops `security.strict` from its own file stays Strict.
#[test]
fn a_project_folder_that_is_strict_by_its_file_keeps_its_worktrees_strict() {
    let (base, repos) = setup();
    let folder = base.path().join("folder");
    write(
        folder.join(SETTINGS_FILE),
        r#"{"security":{"strict":true}}"#,
    )
    .unwrap();
    write(folder.join(".mcp.json"), "{}\n").unwrap();
    let hash = repos.trust_view("default").unwrap().hash;
    repos.approve("default", &hash).unwrap();
    let worktree = base.path().join("worktree");
    write(worktree.join(SETTINGS_FILE), "{}").unwrap();
    write(worktree.join(".mcp.json"), "{ }\n").unwrap();

    assert!(!repos.is_trusted_in(None, &worktree));
}

#[test]
fn a_project_removed_and_added_again_asks_again() {
    let (base, repos) = setup();
    let folder = base.path().join("folder");
    run_git(&folder, &["init", "-q"]);
    write(folder.join(".mcp.json"), "{}\n").unwrap();
    let hash = repos.trust_view("default").unwrap().hash;
    repos.approve("default", &hash).unwrap();

    repos.remove("default").unwrap();
    let path = folder.to_string_lossy().to_string();
    let repo = repos.add(&json!({ "path": path })).unwrap();

    let view = repos.trust_view(&repo.id).unwrap();
    assert!(!view.trusted && !view.covered);
}
