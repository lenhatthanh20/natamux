//! Helpers of the live-daemon tests.

// Test code: a failed unwrap is a failed test. clippy.toml allows it only inside #[test] fns.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

use std::any::Any;
use std::sync::Mutex;

use natamux_client::Client;

pub fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

/// The tasks a flow made, so they go even when a step fails.
#[derive(Default)]
pub struct Made(Mutex<Vec<String>>);

impl Made {
    pub fn add(&self, id: &str) {
        self.0.lock().unwrap().push(id.to_string());
    }
}

/// Removes the workspaces and deletes the tasks of `made`, the newest first. A task that is gone
/// already is no error here. Then the failure of the flow, if any, goes on.
pub async fn finish(client: &Client, made: &Made, result: Result<(), Box<dyn Any + Send>>) {
    let ids = made.0.lock().unwrap().clone();
    for id in ids.iter().rev() {
        let _ = client.remove_workspace(id, true).await;
        let _ = client.delete_task(id).await;
    }
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}
