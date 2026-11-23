//! The audit log, `logs/audit.log` in the data folder: one JSON line per blocked action or
//! security change, with only `ts`, `event`, `id`, `result` and `settingsVersion`. Written only
//! while `data.auditLog` is on (the Strict default), kept under `data.logMaxBytes`, mode 0600.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use natamux_core::js::iso_now;
use natamux_core::privacy::audit::{self, clean_word};
use natamux_core::privacy::log_trim::trim_file;
use natamux_core::security::{FILE_NAME, data_in};
use natamux_core::workspaces::private_dir::{append_options, private_dir};
use serde_json::json;
use sha2::{Digest, Sha256};

pub const AUDIT_FILE: &str = "logs/audit.log";

pub fn audit_path(data_dir: &Path) -> PathBuf {
    data_dir.join(AUDIT_FILE)
}

/// The first 16 hex digits of the SHA-256 of `security.json`, `none` with no file: which
/// settings were in force, without their content.
pub fn settings_version(data_dir: &Path) -> String {
    let Ok(bytes) = std::fs::read(data_dir.join(FILE_NAME)) else {
        return "none".to_string();
    };
    let digest = Sha256::digest(&bytes);
    digest
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// One line. The words are cleaned again here, so a caller can never pass free text.
pub fn line_of(ts: &str, event: &str, id: &str, result: &str, version: &str) -> String {
    json!({
        "ts": ts,
        "event": clean_word(event),
        "id": clean_word(id),
        "result": clean_word(result),
        "settingsVersion": clean_word(version),
    })
    .to_string()
}

/// Adds one line when the audit log is on. A log that cannot be written never stops the action.
pub fn write(data_dir: &Path, event: &str, id: &str, result: &str) {
    let settings = data_in(data_dir);
    if !settings.audit_log {
        return;
    }
    let path = audit_path(data_dir);
    let line = line_of(&iso_now(), event, id, result, &settings_version(data_dir));
    let written = path
        .parent()
        .map_or(Ok(()), private_dir)
        .and_then(|()| append_options().open(&path))
        .and_then(|mut file| file.write_all(format!("{line}\n").as_bytes()));
    if let Err(error) = written {
        eprintln!("The audit log could not be written: {error}");
        return;
    }
    // Cut to three quarters, so a full log is not rewritten on every line.
    let over = std::fs::metadata(&path).is_ok_and(|meta| meta.len() > settings.log_max_bytes);
    if over {
        let _ = trim_file(&path, settings.log_max_bytes / 4 * 3);
    }
}

/// Makes every `natamux_core::privacy::audit::record` of this process write to `data_dir`.
pub fn install(data_dir: &Path) {
    let data_dir = data_dir.to_path_buf();
    audit::install(Arc::new(move |event, id, result| {
        write(&data_dir, event, id, result);
    }));
}

#[cfg(test)]
#[path = "audit_tests.rs"]
mod tests;
