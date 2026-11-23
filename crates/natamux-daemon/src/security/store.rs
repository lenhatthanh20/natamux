//! The daemon's side of `security.json`: one `Source` for the routes and the rules, the body of
//! `GET /security`, the checks of a save, and `security.changed` when the state moves.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use natamux_core::security::{
    SecurityState, Source, installed, isolation_of, isolation_value, read_value, to_text, view_of,
};
use serde_json::Value;

use crate::shared::event_bus::{DaemonMessage, EventBus};

pub struct SecurityStore {
    pub source: Arc<Source>,
    /// The generation the last `security.changed` told.
    told: AtomicU64,
}

impl SecurityStore {
    /// The settings the daemon binary installed for `mode()`, else the data folder's own: a
    /// test daemon reads its settings without changing the mode of other tests.
    pub fn open(data_dir: &Path) -> Arc<SecurityStore> {
        let source = installed()
            .filter(|source| source.path().parent() == Some(data_dir))
            .unwrap_or_else(|| Arc::new(Source::open(data_dir)));
        let told = AtomicU64::new(source.generation());
        Arc::new(SecurityStore { source, told })
    }

    pub fn state(&self) -> SecurityState {
        self.source.current()
    }

    /// Sends `security.changed` once per change of the state.
    pub fn tell(&self, bus: &EventBus) {
        let now = self.source.generation();
        if self.told.swap(now, Ordering::SeqCst) == now {
            return;
        }
        let state = self.source.current();
        bus.publish(DaemonMessage::SecurityChanged {
            strict: state.mode.is_strict(),
            warning: state.warning,
        });
    }

    /// The save of `PUT /security`.
    pub fn save(&self, body: &Value) -> Result<SecurityState, String> {
        let text = with_isolation_kept(checked_text(body)?, body, self.source.path());
        self.source
            .write(&text)
            .map_err(|error| format!("security.json could not be written: {error}"))
    }
}

/// The file text of a `PUT /security` body: the shape of `security.json`. A key looser than the
/// Strict default is refused instead of dropped, so the user sees what did not apply.
pub fn checked_text(body: &Value) -> Result<String, String> {
    let Some(keys) = body.as_object() else {
        return Err("Send the security settings as a JSON object".into());
    };
    if !keys.get("strict").is_some_and(Value::is_boolean) {
        return Err("strict must be true or false".into());
    }
    let state = read_value(body);
    if !state.ignored.is_empty() {
        return Err(format!(
            "These keys can only be stricter than the Strict default: {}",
            state.ignored.join(", ")
        ));
    }
    Ok(to_text(&state.mode))
}

/// `isolation` is its own setting, which the Security page does not show: a save keeps the
/// file's own unless the body sets it. It applies at the next start.
pub fn with_isolation_kept(text: String, body: &Value, file: &Path) -> String {
    let kept = match body.get("isolation") {
        Some(_) => Some(isolation_of(body)),
        None => std::fs::read_to_string(file)
            .ok()
            .and_then(|old| serde_json::from_str::<Value>(&old).ok())
            .filter(|old| old.get("isolation").is_some())
            .map(|old| isolation_of(&old)),
    };
    let Some(settings) = kept else {
        return text;
    };
    let Ok(mut value) = serde_json::from_str::<Value>(&text) else {
        return text;
    };
    if let Some(keys) = value.as_object_mut() {
        keys.insert("isolation".into(), isolation_value(&settings));
    }
    serde_json::to_string_pretty(&value).unwrap_or(text)
}

/// The body of `GET /security` and `PUT /security`. No secrets: task tokens may read it.
pub fn security_body(state: &SecurityState) -> Value {
    serde_json::to_value(view_of(state)).unwrap_or(Value::Null)
}

/// Reads the file once per second while the daemon runs, so an outside change is told at once
/// and not only at the next decision. Ends with the daemon.
pub fn watch(store: &Arc<SecurityStore>, bus: &Arc<EventBus>) {
    let (store, bus) = (Arc::downgrade(store), Arc::downgrade(bus));
    let spawned = std::thread::Builder::new()
        .name("security-watch".into())
        .spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
                let (Some(store), Some(bus)) = (store.upgrade(), bus.upgrade()) else {
                    return;
                };
                store.source.check_now();
                store.tell(&bus);
            }
        });
    if let Err(error) = spawned {
        eprintln!("The security settings are not watched: {error}");
    }
}

/// The store of a starting daemon, watched.
pub fn start(data_dir: &Path, bus: &Arc<EventBus>) -> Arc<SecurityStore> {
    let store = SecurityStore::open(data_dir);
    watch(&store, bus);
    store
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
