//! What the hook tests add to the test daemon: a home folder for session files, and a hook as an
//! agent's hook command posts it.

use hyper::Method;
use serde_json::Value;

use crate::server::{TestDaemon, start_daemon, test_root};

/// A bare daemon whose home folder is `<root>/home`: session files of agents are read from it.
pub async fn bare_with_home(name: &str) -> TestDaemon {
    let root = test_root(name);
    let home = root.join("home");
    std::fs::create_dir_all(&home).unwrap();
    start_daemon(
        root,
        None,
        natamux_proto::terminals::messages::SILENT_CLOSE,
        Some(home),
    )
    .await
}

impl TestDaemon {
    /// `POST /hooks/<agent><query>` as an agent's hook command sends it: `headers` are extra
    /// request headers (`x-natamux-task`, `x-natamux-terminal`).
    pub async fn hook(
        &self,
        agent: &str,
        query: &str,
        headers: &[(&str, &str)],
        token: Option<&str>,
        body: &Value,
    ) -> (u16, Value) {
        let path = format!("/hooks/{agent}{query}");
        let (status, _, bytes) = self
            .exchange_with(Method::POST, &path, token, headers, Some(body))
            .await;
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }
}
