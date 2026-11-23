//! The support bundle (`GET /support-bundle`, `natamux support-bundle`): what a person may send
//! when asking for help. Built from a fixed struct, so a new field is a code change someone
//! reviews: versions, OS, the security state and keys, task counts by state, uptime, and the last
//! log lines with secrets masked and typed input left out. No task text, events or environment.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Instant;

use natamux_core::privacy::redact::redact_text;
use natamux_core::security::{DataSettings, Egress, SecurityState, key_states};
use natamux_proto::tasks::task::TaskState;
use serde::Serialize;

pub const LOG_LINES: usize = 200;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportBundle {
    pub created_at: String,
    pub daemon_version: &'static str,
    pub os: &'static str,
    pub arch: &'static str,
    pub uptime_secs: u64,
    pub security: SecurityPart,
    pub tasks: BTreeMap<String, usize>,
    pub log: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityPart {
    pub strict: bool,
    /// One of the fixed warnings of the settings, never user text.
    pub warning: Option<String>,
    pub keys: BTreeMap<String, String>,
    pub ignored: Vec<String>,
    pub isolation: String,
    pub egress: &'static str,
    pub audit_log: bool,
    pub retention_days: u32,
    pub log_max_bytes: u64,
}

/// What the bundle is built from.
pub struct BundleInput<'a> {
    pub created_at: String,
    pub security: &'a SecurityState,
    pub isolation: String,
    pub egress: Egress,
    pub data: DataSettings,
    pub states: Vec<TaskState>,
    pub log: Vec<String>,
}

static STARTED: OnceLock<Instant> = OnceLock::new();

/// Called once when the daemon starts; the bundle tells the time since.
pub fn mark_start() {
    let _ = STARTED.set(Instant::now());
}

fn uptime_secs() -> u64 {
    STARTED
        .get()
        .map_or(0, |started| started.elapsed().as_secs())
}

fn state_name(state: &TaskState) -> String {
    serde_json::to_value(state)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}

pub fn build(input: BundleInput<'_>) -> SupportBundle {
    let keys = key_states(&input.security.mode)
        .into_iter()
        .map(|each| (each.key.to_string(), each.value))
        .collect();
    let mut tasks = BTreeMap::new();
    for state in &input.states {
        *tasks.entry(state_name(state)).or_insert(0) += 1;
    }
    SupportBundle {
        created_at: input.created_at,
        daemon_version: env!("CARGO_PKG_VERSION"),
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        uptime_secs: uptime_secs(),
        security: SecurityPart {
            strict: input.security.mode.is_strict(),
            warning: input.security.warning.clone(),
            keys,
            ignored: input.security.ignored.clone(),
            isolation: input.isolation,
            egress: input.egress.as_str(),
            audit_log: input.data.audit_log,
            retention_days: input.data.retention_days,
            log_max_bytes: input.data.log_max_bytes,
        },
        tasks,
        log: input.log,
    }
}

/// The daemon log the app writes beside the data folder (`<app>/logs/daemon.log`).
pub fn daemon_log_path(data_dir: &Path) -> Option<PathBuf> {
    data_dir
        .parent()
        .map(|app| app.join("logs").join("daemon.log"))
}

/// A line of the terminal input trace (`input <key> recv|write …`): typed text, never sent.
pub fn is_trace_line(line: &str) -> bool {
    let words: Vec<&str> = line.split_whitespace().collect();
    words
        .windows(3)
        .any(|w| w[0] == "input" && matches!(w[2], "recv" | "write"))
}

/// The last `count` lines of `text`, trace lines left out, secrets masked.
pub fn tail_of(text: &str, count: usize) -> Vec<String> {
    let kept: Vec<&str> = text.lines().filter(|line| !is_trace_line(line)).collect();
    let from = kept.len().saturating_sub(count);
    kept.get(from..)
        .unwrap_or_default()
        .iter()
        .copied()
        .map(redact_text)
        .collect()
}

/// `tail_of` a file; a missing or unreadable file gives no lines.
pub fn log_tail(path: Option<&Path>) -> Vec<String> {
    let text = path
        .and_then(|path| std::fs::read(path).ok())
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_default();
    tail_of(&text, LOG_LINES)
}

#[cfg(test)]
#[path = "support_bundle_tests.rs"]
mod tests;
