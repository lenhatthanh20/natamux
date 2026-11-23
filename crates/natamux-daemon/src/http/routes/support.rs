//! `GET /support-bundle`: the support bundle, desktop token only. An agent token never gets it:
//! the bundle shows the security state an agent could use to plan around it.

use std::sync::Arc;

use hyper::StatusCode;
use natamux_core::js::iso_now;
use natamux_core::security::{Egress, data_in, network_in};
use serde_json::Value;

use crate::http::errors::{Answer, error_answer, internal_error, json_answer};
use crate::http::server::AppState;
use crate::privacy::audit;
use crate::privacy::support_bundle::{BundleInput, build, daemon_log_path, log_tail};

fn bundle_of(state: &AppState) -> Value {
    let security = state.security.state();
    let strict = security.mode.is_strict();
    let mut egress = network_in(&state.data_dir, strict).egress;
    if natamux_core::offline::offline() {
        egress = Egress::Loopback;
    }
    let isolation = match state.agents.isolation() {
        None => "off".to_string(),
        Some(isolation) if isolation.support.is_ok() => "required".to_string(),
        Some(_) => "required, cannot run".to_string(),
    };
    let states = state
        .tasks
        .list_tasks(None)
        .unwrap_or_default()
        .into_iter()
        .map(|task| task.state)
        .collect();
    let bundle = build(BundleInput {
        created_at: iso_now(),
        security: &security,
        isolation,
        egress,
        data: data_in(&state.data_dir),
        states,
        log: log_tail(daemon_log_path(&state.data_dir).as_deref()),
    });
    serde_json::to_value(bundle).unwrap_or(Value::Null)
}

pub async fn get_support_bundle(state: Arc<AppState>, by_agent: bool) -> Answer {
    if by_agent {
        let message = "Only the person at the app makes a support bundle.";
        return error_answer(StatusCode::FORBIDDEN, "forbidden", message);
    }
    let made = tokio::task::spawn_blocking(move || {
        let body = bundle_of(&state);
        audit::write(&state.data_dir, "support-bundle.create", "desktop", "ok");
        body
    })
    .await;
    match made {
        Ok(body) => json_answer(StatusCode::OK, &body),
        Err(_) => internal_error(),
    }
}
