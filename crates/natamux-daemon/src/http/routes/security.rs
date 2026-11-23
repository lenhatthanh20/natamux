//! `GET /security`: the security mode and what each key blocks; any token may read it, task
//! tokens too. `PUT /security`: the only way to change the settings, desktop token only, so
//! turning Strict off or loosening a key needs the person at the app.

use std::sync::Arc;

use hyper::StatusCode;
use serde_json::Value;

use crate::http::errors::{Answer, error_answer, internal_error, json_answer};
use crate::http::server::AppState;
use crate::security::store::security_body;

pub async fn get_security(state: Arc<AppState>) -> Answer {
    match tokio::task::spawn_blocking(move || state.security.state()).await {
        Ok(now) => json_answer(StatusCode::OK, &security_body(&now)),
        Err(_) => internal_error(),
    }
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
        saved
    })
    .await;
    match saved {
        Ok(Ok(now)) => json_answer(StatusCode::OK, &security_body(&now)),
        Ok(Err(message)) => error_answer(StatusCode::BAD_REQUEST, "invalid_request", &message),
        Err(_) => internal_error(),
    }
}
