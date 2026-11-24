//! The `data` key of `security.json`: how long finished tasks stay, how big a log may grow, and
//! the audit log. Retention is never a default (0 = off); the audit log is on by default only in
//! Strict mode, where turning it off is looser and ignored.

use std::path::Path;

use serde_json::Value;

use super::source::{FILE_NAME, evaluate, read, written_in};

pub const DEFAULT_LOG_MAX_BYTES: u64 = 1024 * 1024;
/// A smaller cap would cut a log to a few lines.
const MIN_LOG_MAX_BYTES: u64 = 4 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DataSettings {
    /// Finished tasks older than this many days are deleted. 0: never.
    pub retention_days: u32,
    pub log_max_bytes: u64,
    pub audit_log: bool,
}

impl DataSettings {
    pub fn defaults(strict: bool) -> DataSettings {
        DataSettings {
            retention_days: 0,
            log_max_bytes: DEFAULT_LOG_MAX_BYTES,
            audit_log: strict,
        }
    }
}

/// The `data` object of a `security.json` value. A bad value keeps the default: no deletion and
/// the default cap.
pub fn data_of(value: &Value, strict: bool) -> DataSettings {
    let mut settings = DataSettings::defaults(strict);
    let Some(data) = value.get("data") else {
        return settings;
    };
    if let Some(days) = data.get("retentionDays").and_then(Value::as_u64) {
        settings.retention_days = u32::try_from(days).unwrap_or(u32::MAX);
    }
    if let Some(max) = data.get("logMaxBytes").and_then(Value::as_u64) {
        settings.log_max_bytes = max.max(MIN_LOG_MAX_BYTES);
    }
    if data.get("auditLog") == Some(&Value::Bool(true)) {
        settings.audit_log = true;
    }
    settings
}

fn value_of(bytes: Option<&[u8]>) -> Value {
    bytes
        .and_then(|bytes| serde_json::from_slice::<Value>(bytes).ok())
        .unwrap_or(Value::Null)
}

/// The settings of a data folder. While the file differs from the copy the daemon wrote (an
/// outside change), the audit log is on if either has it, the smaller cap counts, and nothing is
/// deleted: deletion never follows settings Natamux did not write.
pub fn data_in(data_dir: &Path) -> DataSettings {
    data_with(data_dir, written_in(data_dir))
}

/// `data_in` with the copy the daemon wrote given.
fn data_with(data_dir: &Path, written: Option<Vec<u8>>) -> DataSettings {
    let file = read(&data_dir.join(FILE_NAME));
    let strict = super::mode().is_strict()
        || evaluate(file.as_deref(), written.as_deref())
            .mode
            .is_strict();
    let now = data_of(&value_of(file.as_deref()), strict);
    if file.is_none() || written.is_none() || file == written {
        // No copy yet (hand-made file): no deletion either, for the same reason.
        if file.is_some() && written.is_none() {
            return DataSettings {
                retention_days: 0,
                ..now
            };
        }
        return now;
    }
    let before = data_of(&value_of(written.as_deref()), strict);
    DataSettings {
        retention_days: 0,
        log_max_bytes: now.log_max_bytes.min(before.log_max_bytes),
        audit_log: now.audit_log || before.audit_log,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::security::{Source, WRITTEN_NAME};

    #[test]
    fn defaults_keep_everything_and_audit_only_in_strict() {
        assert_eq!(data_of(&json!({}), false), DataSettings::defaults(false));
        assert!(!data_of(&json!({}), false).audit_log);
        assert!(data_of(&json!({}), true).audit_log);
        let set = data_of(
            &json!({ "data": { "retentionDays": 30, "logMaxBytes": 10, "auditLog": true } }),
            false,
        );
        assert_eq!(set.retention_days, 30);
        assert_eq!(set.log_max_bytes, MIN_LOG_MAX_BYTES);
        assert!(set.audit_log);
        // Off is looser than Strict: ignored.
        assert!(data_of(&json!({ "data": { "auditLog": false } }), true).audit_log);
        assert_eq!(
            data_of(&json!({ "data": { "retentionDays": "30" } }), false).retention_days,
            0
        );
    }

    #[test]
    fn a_file_natamux_did_not_write_deletes_nothing() {
        let dir = std::env::temp_dir().join(format!("natamux-data-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(data_in(&dir), DataSettings::defaults(false));
        let text = r#"{"strict":false,"data":{"retentionDays":7,"auditLog":true}}"#;
        std::fs::write(dir.join(FILE_NAME), text).unwrap();
        let hand = data_in(&dir);
        assert_eq!((hand.retention_days, hand.audit_log), (0, true));
        std::fs::write(dir.join(WRITTEN_NAME), text).unwrap();
        assert_eq!(data_in(&dir).retention_days, 7);
        std::fs::write(
            dir.join(FILE_NAME),
            r#"{"strict":false,"data":{"retentionDays":1}}"#,
        )
        .unwrap();
        let changed = data_in(&dir);
        assert_eq!((changed.retention_days, changed.audit_log), (0, true));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_running_daemons_copy_counts_when_both_files_change() {
        let dir = std::env::temp_dir().join(format!("natamux-data-both-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let strict = r#"{"strict":true,"data":{"retentionDays":7}}"#;
        let source = Source::open(&dir);
        source.write(strict).unwrap();
        assert_eq!(data_with(&dir, source.written()).retention_days, 7);
        let looser = r#"{"strict":false,"data":{"retentionDays":1}}"#;
        std::fs::write(dir.join(FILE_NAME), looser).unwrap();
        std::fs::write(dir.join(WRITTEN_NAME), looser).unwrap();
        // Deletion never follows settings Natamux did not write; Strict keeps the audit log.
        let kept = data_with(&dir, source.written());
        assert_eq!((kept.retention_days, kept.audit_log), (0, true));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_unreadable_file_keeps_the_audit_log() {
        let dir = std::env::temp_dir().join(format!("natamux-data-dir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        // A folder where the file should be: reading it fails with an error other than NotFound.
        std::fs::create_dir_all(dir.join(FILE_NAME)).unwrap();
        let broken = data_in(&dir);
        assert_eq!((broken.retention_days, broken.audit_log), (0, true));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
