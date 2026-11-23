//! `GET /security`: the security mode and what each key blocks; any token may read it, task
//! tokens too. `PUT /security`: the only way to change the settings, desktop token only, so
//! turning Strict off or loosening a key needs the person at the app.

use std::sync::Arc;

use hyper::StatusCode;
use natamux_core::security::{Egress, NetworkSettings, SecurityState, network_in};
use serde_json::Value;

use crate::http::errors::{Answer, error_answer, internal_error, json_answer};
use crate::http::server::AppState;
use crate::security::store::security_body;
use crate::terminals::isolation::isolation_body;
use crate::terminals::isolation_network::network_view;

pub async fn get_security(state: Arc<AppState>) -> Answer {
    // `network` reads security.json: off the reactor.
    let read = move || (state.security.state(), isolation_value(&state));
    match tokio::task::spawn_blocking(read).await {
        Ok((now, isolation)) => json_answer(StatusCode::OK, &with_isolation(&now, isolation)),
        Err(_) => internal_error(),
    }
}

/// How shells are isolated in this daemon run (a change of the setting needs a restart), and
/// where Natamux's own calls may go.
fn isolation_value(state: &AppState) -> (Value, Value) {
    let isolation = state.agents.isolation();
    let abi = isolation.as_ref().and_then(|it| it.support.clone().ok());
    // The daemon's own folder: a test daemon does not install its settings for the process.
    let strict = state.security.state().mode.is_strict();
    let folder = state.security.source.path().parent();
    let mut network = folder.map_or_else(
        || NetworkSettings::defaults(strict),
        |dir| network_in(dir, strict),
    );
    if natamux_core::offline::offline() {
        network.egress = Egress::Loopback;
    }
    let network = network_view(&network, abi);
    (
        serde_json::to_value(isolation_body(isolation.as_ref())).unwrap_or(Value::Null),
        serde_json::to_value(network).unwrap_or(Value::Null),
    )
}

fn with_isolation(now: &SecurityState, (isolation, network): (Value, Value)) -> Value {
    let mut body = security_body(now);
    if let Some(keys) = body.as_object_mut() {
        keys.insert("isolation".into(), isolation);
        keys.insert("network".into(), network);
    }
    body
}

pub async fn put_security(state: Arc<AppState>, body: Option<Value>, by_agent: bool) -> Answer {
    if by_agent {
        let message = "Only the person at the app changes the security settings.";
        return error_answer(StatusCode::FORBIDDEN, "forbidden", message);
    }
    let body = body.unwrap_or(Value::Null);
    let saved = tokio::task::spawn_blocking(move || {
        let saved = state.security.save(&body);
        state.security.tell(&state.bus);
        saved.map(|now| (now, isolation_value(&state)))
    })
    .await;
    match saved {
        Ok(Ok((now, isolation))) => json_answer(StatusCode::OK, &with_isolation(&now, isolation)),
        Ok(Err(message)) => error_answer(StatusCode::BAD_REQUEST, "invalid_request", &message),
        Err(_) => internal_error(),
    }
}
