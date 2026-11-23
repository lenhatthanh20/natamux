//! `GET /security`: the security mode and what each key blocks; any token may read it, task
//! tokens too. `PUT /security`: the only way to change the settings, desktop token only, so
//! turning Strict off or loosening a key needs the person at the app.

use std::sync::Arc;

use hyper::StatusCode;
use natamux_core::security::SecurityState;
use serde_json::Value;

use crate::http::errors::{Answer, error_answer, internal_error, json_answer};
use crate::http::server::AppState;
use crate::security::store::security_body;
use crate::terminals::isolation::isolation_body;

pub async fn get_security(state: Arc<AppState>) -> Answer {
    let isolation = isolation_value(&state);
    match tokio::task::spawn_blocking(move || state.security.state()).await {
        Ok(now) => json_answer(StatusCode::OK, &with_isolation(&now, isolation)),
        Err(_) => internal_error(),
    }
}

/// How shells are isolated in this daemon run: a change of the setting needs a restart.
fn isolation_value(state: &AppState) -> Value {
    serde_json::to_value(isolation_body(state.agents.isolation().as_ref())).unwrap_or(Value::Null)
}

fn with_isolation(now: &SecurityState, isolation: Value) -> Value {
    let mut body = security_body(now);
    if let Some(keys) = body.as_object_mut() {
        keys.insert("isolation".into(), isolation);
    }
    body
}

pub async fn put_security(state: Arc<AppState>, body: Option<Value>, by_agent: bool) -> Answer {
    if by_agent {
        let message = "Only the person at the app changes the security settings.";
        return error_answer(StatusCode::FORBIDDEN, "forbidden", message);
    }
    let body = body.unwrap_or(Value::Null);
    let isolation = isolation_value(&state);
    let saved = tokio::task::spawn_blocking(move || {
        let saved = state.security.save(&body);
        state.security.tell(&state.bus);
        saved
    })
    .await;
    match saved {
        Ok(Ok(now)) => json_answer(StatusCode::OK, &with_isolation(&now, isolation)),
        Ok(Err(message)) => error_answer(StatusCode::BAD_REQUEST, "invalid_request", &message),
        Err(_) => internal_error(),
    }
}
