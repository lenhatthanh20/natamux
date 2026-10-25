//! Request bodies as Fastify 5 reads them: JSON (with secure-json-parse's refusal of prototype
//! keys) and plain text, no body when there is none, and its errors with their statuses, which
//! the error handler answers as `{ error: 'bad_request', message }`.

use http_body_util::{BodyExt, Full, Limited};
use hyper::body::{Bytes, Incoming};
use hyper::header::{
    CACHE_CONTROL, CONTENT_LENGTH, CONTENT_SECURITY_POLICY, CONTENT_TYPE, HeaderValue,
    TRANSFER_ENCODING,
};
use hyper::{HeaderMap, StatusCode};
use serde_json::Value;

use super::errors::{Answer, bad_request};

/// A body Fastify refuses: its status and message. Small, so a `Result` of it stays small.
#[derive(Debug, PartialEq, Eq)]
pub struct BodyError {
    pub status: StatusCode,
    pub message: String,
}

impl BodyError {
    fn new(status: StatusCode, message: &str) -> BodyError {
        BodyError {
            status,
            message: message.into(),
        }
    }

    pub fn answer(&self) -> Answer {
        bad_request(self.status, &self.message)
    }
}

/// Raw bytes of a file of the user. An SVG can hold scripts, so the answer may not run anything,
/// even when it is opened on its own.
pub fn sandboxed_bytes(content_type: &str, bytes: Vec<u8>) -> Answer {
    let mut answer = Answer::new(Full::new(Bytes::from(bytes)));
    let headers = answer.headers_mut();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_str(content_type)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; style-src 'unsafe-inline'; sandbox"),
    );
    answer
}

/// Fastify's default `bodyLimit`.
pub const DEFAULT_BODY_LIMIT: usize = 1_048_576;

fn too_large() -> BodyError {
    BodyError::new(StatusCode::PAYLOAD_TOO_LARGE, "Request body is too large")
}

/// The body of a request that may carry one: None when there is none (`request.body` undefined).
pub async fn read_body(
    headers: &HeaderMap,
    body: Incoming,
    limit: usize,
) -> Result<Option<Value>, BodyError> {
    let header = |name| {
        headers
            .get(name)
            .and_then(|value: &hyper::header::HeaderValue| value.to_str().ok())
    };
    let length = header(CONTENT_LENGTH);
    if length
        .and_then(|length| length.trim().parse::<usize>().ok())
        .is_some_and(|length| length > limit)
    {
        return Err(too_large());
    }
    let content_type = header(CONTENT_TYPE).map(str::to_string);
    let no_body = header(TRANSFER_ENCODING).is_none() && matches!(length, None | Some("0"));
    if content_type.is_none() && no_body {
        return Ok(None);
    }
    let bytes = match Limited::new(body, limit).collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(_) => return Err(too_large()),
    };
    let media = content_type.as_deref().map(|kind| {
        kind.split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase()
    });
    match media.as_deref() {
        Some("application/json") => parse_json(&bytes).map(Some),
        Some("text/plain") => Ok(Some(Value::String(
            String::from_utf8_lossy(&bytes).into_owned(),
        ))),
        _ => Err(BodyError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Unsupported Media Type",
        )),
    }
}

fn parse_json(bytes: &[u8]) -> Result<Value, BodyError> {
    let text = String::from_utf8_lossy(bytes);
    if text.is_empty() {
        return Err(BodyError::new(
            StatusCode::BAD_REQUEST,
            "Body cannot be empty when content-type is set to 'application/json'",
        ));
    }
    // Fastify answers a parse error and a prototype key alike (FST_ERR_CTP_INVALID_JSON_BODY).
    let invalid = || {
        BodyError::new(
            StatusCode::BAD_REQUEST,
            "Body is not valid JSON but content-type is set to 'application/json'",
        )
    };
    let value = natamux_core::json::parse_js(&text).map_err(|_| invalid())?;
    if has_prototype_key(&value) {
        return Err(invalid());
    }
    Ok(value)
}

/// `__proto__`, or `constructor` holding `prototype`, anywhere.
fn has_prototype_key(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(key, each)| {
            key == "__proto__"
                || (key == "constructor"
                    && each
                        .as_object()
                        .is_some_and(|inner| inner.contains_key("prototype")))
                || has_prototype_key(each)
        }),
        Value::Array(items) => items.iter().any(has_prototype_key),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_prototype_keys_anywhere() {
        let refused = |text: &str| has_prototype_key(&natamux_core::json::parse_js(text).unwrap());
        assert!(refused(r#"{"a":[{"__proto__":{}}]}"#));
        assert!(refused(r#"{"constructor":{"prototype":{}}}"#));
        assert!(!refused(r#"{"constructor":1,"proto":2}"#));
    }
}
