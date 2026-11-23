use natamux_core::security::{SecurityMode, StrictSettings};

use super::*;

const TOP_KEYS: &str = "arch,createdAt,daemonVersion,log,os,security,tasks,uptimeSecs";
const SECURITY_KEYS: &str =
    "auditLog,egress,ignored,isolation,keys,logMaxBytes,retentionDays,strict,warning";

fn keys_of(value: &serde_json::Value) -> String {
    let object = value.as_object().unwrap();
    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    keys.join(",")
}

#[test]
fn trace_lines_go_and_secrets_are_masked() {
    let log = "start\n\
        input 1/T-1 recv +5 \"TYPED-MARK\"\n\
        push failed: https://u:FAKE-PASS@h.test/x.git token ghp_FAKEMARKER12345\n\
        end\n";
    let tail = tail_of(log, 10);
    let text = tail.join("\n");
    assert_eq!(tail.len(), 3, "{text}");
    assert!(!text.contains("TYPED-MARK"), "{text}");
    assert!(!text.contains("FAKE-PASS"), "{text}");
    assert!(!text.contains("FAKEMARKER"), "{text}");
    assert_eq!(tail_of("a\nb\nc\n", 2), ["b", "c"]);
}

#[test]
fn the_bundle_has_only_the_fixed_fields() {
    let security = SecurityState {
        mode: SecurityMode::Strict(StrictSettings::default()),
        ignored: Vec::new(),
        warning: None,
    };
    let bundle = build(BundleInput {
        created_at: "2026-11-23T00:00:00.000Z".into(),
        security: &security,
        isolation: "off".into(),
        egress: Egress::Open,
        data: DataSettings::defaults(true),
        states: vec![TaskState::Done, TaskState::Done, TaskState::Running],
        log: vec!["one".into()],
    });
    let value = serde_json::to_value(&bundle).unwrap();
    assert_eq!(keys_of(&value), TOP_KEYS);
    assert_eq!(keys_of(&value["security"]), SECURITY_KEYS);
    assert_eq!(value["tasks"]["done"], 2);
    assert_eq!(value["tasks"]["running"], 1);
    assert_eq!(value["security"]["keys"]["ciPush"], "approved-commit");
}
