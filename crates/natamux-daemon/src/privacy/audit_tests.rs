use std::fs;

use natamux_core::security::WRITTEN_NAME;
use serde_json::Value;

use super::*;

const MARK: &str = "AUDIT-MARK secret title";

fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("natamux-audit-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Settings as `PUT /security` writes them: the file and the copy are the same.
fn saved(dir: &Path, text: &str) {
    fs::write(dir.join(FILE_NAME), text).unwrap();
    fs::write(dir.join(WRITTEN_NAME), text).unwrap();
}

fn lines(dir: &Path) -> Vec<Value> {
    fs::read_to_string(audit_path(dir))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn personal_mode_writes_nothing_by_default() {
    let dir = folder("personal");
    write(&dir, "task.delete", "T-1", "ok");
    assert!(!audit_path(&dir).exists());
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_line_has_only_the_five_keys_and_never_a_marker() {
    let dir = folder("keys");
    saved(&dir, r#"{"strict":true}"#);
    write(&dir, "agent.blocked", "T-0042", "blocked");
    write(&dir, "agent.blocked", MARK, &format!("refused: {MARK}"));
    let text = fs::read_to_string(audit_path(&dir)).unwrap();
    assert!(!text.contains("AUDIT-MARK"), "{text}");
    let got = lines(&dir);
    assert_eq!(got.len(), 2);
    for line in &got {
        let mut keys: Vec<&String> = line.as_object().unwrap().keys().collect();
        keys.sort();
        assert_eq!(keys, ["event", "id", "result", "settingsVersion", "ts"]);
    }
    assert_eq!(got[0]["id"], "T-0042");
    assert_eq!(got[0]["result"], "blocked");
    assert_eq!(got[0]["settingsVersion"], settings_version(&dir));
    assert_eq!(got[0]["settingsVersion"].as_str().unwrap().len(), 16);
    assert_eq!(got[1]["id"], "-");
    assert_eq!(got[1]["result"], "-");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let meta = fs::metadata(audit_path(&dir)).unwrap();
        let mode = meta.permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn personal_mode_writes_when_the_user_turns_it_on() {
    let dir = folder("opt-in");
    saved(&dir, r#"{"strict":false,"data":{"auditLog":true}}"#);
    write(&dir, "task.delete", "T-1", "ok");
    assert_eq!(lines(&dir).len(), 1);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_log_stays_under_its_cap() {
    let dir = folder("cap");
    saved(&dir, r#"{"strict":true,"data":{"logMaxBytes":4096}}"#);
    for n in 0..200 {
        write(
            &dir,
            "egress.denied",
            &format!("host-{n}.example"),
            "denied",
        );
    }
    let size = fs::metadata(audit_path(&dir)).unwrap().len();
    assert!(size <= 4096, "{size}");
    let got = lines(&dir);
    assert_eq!(got.last().unwrap()["id"], "host-199.example");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_installed_sink_writes_what_other_crates_record() {
    let dir = folder("sink");
    saved(&dir, r#"{"strict":true}"#);
    let path = dir.clone();
    let _forced = audit::force(Arc::new(move |event, id, result| {
        write(&path, event, id, result);
    }));
    audit::record("egress.denied", "github.com", "denied");
    assert_eq!(lines(&dir)[0]["event"], "egress.denied");
    fs::remove_dir_all(&dir).unwrap();
}
