//! `usage-service.test.ts`, ported: one file per `describe` group and the fixture of its
//! `beforeEach`. Not ported: the four `sessionFile` cases that test only the path check ('takes a
//! session file in the home folder', 'refuses a path outside the home folder, or not to a session
//! file', 'refuses a link to a file outside', 'refuses a file over 200 MB'); they are tests of
//! `natamux_agents::usage::session_file` (`natamux-agents/src/usage/session_file.rs`). The case
//! 'is not read when refused' stays here: it tests the service.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// `Fx::new` is `beforeEach`.
#![allow(clippy::new_without_default)]

mod limits;
mod session_file;
mod usage_service;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use natamux_daemon::agents::usage_service::{UsageOptions, UsageService};
use natamux_daemon::shared::event_bus::{DaemonMessage, EventBus};
use serde_json::{Value, json};

pub struct TempDir(pub PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub struct Fx {
    pub root: TempDir,
    pub home: PathBuf,
    pub data: PathBuf,
    pub bus: Arc<EventBus>,
    pub sent: Arc<Mutex<Vec<DaemonMessage>>>,
    pub service: UsageService,
    pub session: PathBuf,
}

impl Fx {
    pub fn new() -> Fx {
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let count = COUNT.fetch_add(1, Ordering::SeqCst);
        let root =
            std::env::temp_dir().join(format!("natamux-usage-{}-{count}", std::process::id()));
        let home = root.join("home");
        let data = root.join("data");
        std::fs::create_dir_all(home.join(".claude")).unwrap();
        let bus = EventBus::new();
        let sent: Arc<Mutex<Vec<DaemonMessage>>> = Arc::default();
        let sink = Arc::clone(&sent);
        bus.subscribe(move |message| sink.lock().unwrap().push(message.clone()));
        let service = UsageService::new(UsageOptions {
            data_dir: data.clone(),
            bus: Some(Arc::clone(&bus)),
            home: home.clone(),
            read_delay: Duration::ZERO,
        });
        let session = home.join(".claude").join("session.jsonl");
        Fx {
            root: TempDir(root),
            home,
            data,
            bus,
            sent,
            service,
            session,
        }
    }

    /// Another service on the same folders, as after a restart.
    pub fn restarted(&self, bus: bool) -> UsageService {
        UsageService::new(UsageOptions {
            data_dir: self.data.clone(),
            bus: bus.then(|| Arc::clone(&self.bus)),
            home: self.home.clone(),
            read_delay: Duration::ZERO,
        })
    }

    /// `read(stage, agent, path)`: a turn, and the wait for its read.
    pub fn read(&self, stage: Option<&str>, agent: &str, path: &Path) {
        self.service
            .seen("T-0001", stage, agent, &path.to_string_lossy());
        self.service.settle();
    }

    pub fn read_session(&self, stage: Option<&str>) {
        self.read(stage, "claude-code", &self.session);
    }

    pub fn messages(&self) -> Vec<DaemonMessage> {
        self.sent.lock().unwrap().clone()
    }
}

/// One message of a Claude Code session.
pub fn claude_line(id: &str, input: u64, output: u64, model: &str) -> String {
    let usage = json!({
        "input_tokens": input,
        "cache_creation_input_tokens": 0,
        "cache_read_input_tokens": 0,
        "output_tokens": output,
    });
    let line =
        json!({ "type": "assistant", "message": { "id": id, "model": model, "usage": usage } });
    format!("{line}\n")
}

pub fn opus_line(id: &str, input: u64, output: u64) -> String {
    claude_line(id, input, output, "claude-opus-5-5")
}

pub fn append(path: &Path, text: &str) {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)
        .unwrap();
    file.write_all(text.as_bytes()).unwrap();
}

/// `[stage, input]` of each row of a task's usage.
pub fn stage_inputs(usage: &Value) -> Vec<(String, u64)> {
    usage["stages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            (
                row["stage"].as_str().unwrap().to_string(),
                row["input"].as_u64().unwrap(),
            )
        })
        .collect()
}
