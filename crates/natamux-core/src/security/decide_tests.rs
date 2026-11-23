use serde_json::json;

use super::*;
use crate::security::CiPush;

fn strict(ci_push: CiPush, grants: &[&str]) -> SecurityMode {
    SecurityMode::Strict(StrictSettings {
        ci_push,
        agent_api: AgentApi {
            grants: grants.iter().map(|it| it.to_string()).collect(),
            ..AgentApi::default()
        },
        ..StrictSettings::default()
    })
}

#[test]
fn tighten_keeps_the_stricter_key_by_key() {
    let a = strict(CiPush::Forbid, &["tasks.list", "pr.create.own"]);
    let b = strict(CiPush::ApprovedSnapshot, &["pr.create.own"]);
    let both = strict(CiPush::Forbid, &["pr.create.own"]);
    assert_eq!(tighten(&a, &b), both);
    assert_eq!(tighten(&b, &a), both);
    assert_eq!(tighten(&SecurityMode::Personal, &b), b);
    assert_eq!(tighten(&b, &SecurityMode::Personal), b);
    let personal = SecurityMode::Personal;
    assert_eq!(tighten(&personal, &personal), personal);
}

#[test]
fn a_repo_can_tighten_but_never_loosen() {
    let user = strict(CiPush::ApprovedSnapshot, &["tasks.list"]);
    let looser = json!({ "security": {
        "strict": false, "ciPush": "approved-commit", "agentApi": { "grants": ["tasks.list", "git.push.own"] },
    }});
    assert_eq!(tighten_with_repo(&user, &looser), user);
    let tighter = json!({ "security": { "ciPush": "forbid", "agentApi": { "grants": [] } } });
    assert_eq!(
        tighten_with_repo(&user, &tighter),
        strict(CiPush::Forbid, &[])
    );
    // A repo may turn Strict on for its own work.
    let on = json!({ "security": { "strict": true } });
    assert_eq!(
        tighten_with_repo(&SecurityMode::Personal, &on),
        SecurityMode::Strict(StrictSettings::default())
    );
    // Keys without `strict` change nothing in Personal mode, and no key at all changes nothing.
    let keys_only = json!({ "security": { "ciPush": "forbid" } });
    assert_eq!(
        tighten_with_repo(&SecurityMode::Personal, &keys_only),
        SecurityMode::Personal
    );
    assert_eq!(tighten_with_repo(&user, &json!({ "stages": [] })), user);
}

#[test]
fn mode_in_reads_the_folder_file_and_ignores_a_broken_one() {
    let root = std::env::temp_dir().join(format!("natamux-mode-in-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    assert_eq!(mode_in(&root), SecurityMode::Personal);
    std::fs::write(root.join(".natamux.json"), "{").unwrap();
    assert_eq!(mode_in(&root), SecurityMode::Personal);
    std::fs::write(
        root.join(".natamux.json"),
        r#"{"security":{"strict":true}}"#,
    )
    .unwrap();
    assert!(mode_in(&root).is_strict());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn personal_blocks_nothing_and_strict_names_each_rule() {
    let personal = key_states(&SecurityMode::Personal);
    assert!(personal.iter().all(|it| it.decision == Decision::Allowed));
    let states = key_states(&strict(CiPush::Forbid, &["tasks.list"]));
    let ci = states.iter().find(|it| it.key == "ciPush").unwrap();
    assert_eq!(ci.value, "forbid");
    assert_eq!(
        ci.decision,
        Decision::Blocked {
            key: "ciPush",
            reason: CI_FORBID_REASON.into()
        }
    );
    let auto = states.iter().find(|it| it.key == "agentAutoMode").unwrap();
    assert!(
        matches!(&auto.decision, Decision::Blocked { reason, .. } if reason == AUTO_FORBID_REASON)
    );
    let grants = states
        .iter()
        .find(|it| it.key == "agentApi.grants")
        .unwrap();
    assert_eq!(grants.value, "tasks.list");
    assert_eq!(states.len(), 2 + FIXED_KEYS.len());
}
