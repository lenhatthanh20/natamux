//! One HTTP/1.1 request to the daemon, with its bearer token. The daemon listens on plain HTTP
//! on this machine, so a connection per request is cheap and needs no pool.

use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::{Method, Request, Uri};
use hyper_util::rt::TokioIo;
use serde::de::DeserializeOwned;
use tokio::net::TcpStream;

use crate::config::Config;
use crate::error::ClientError;

pub async fn call<T: DeserializeOwned>(
    config: &Config,
    method: Method,
    path: &str,
) -> Result<T, ClientError> {
    let body = send(config, method, path).await?;
    serde_json::from_slice(&body).map_err(|error| ClientError::BadAnswer(error.to_string()))
}

pub async fn send(config: &Config, method: Method, path: &str) -> Result<Bytes, ClientError> {
    let (status, body) = request(config, method, path, &[], None).await?;
    if status == hyper::StatusCode::UNAUTHORIZED.as_u16() {
        return Err(ClientError::Unauthorized);
    }
    if !(200..300).contains(&status) {
        return Err(ClientError::Status {
            status,
            message: error_message(&body),
        });
    }
    Ok(body)
}

/// One request with these extra headers and body, and the answer's status and body whatever the
/// status is: the `natamux` command words the daemon's errors itself.
pub async fn request(
    config: &Config,
    method: Method,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<Vec<u8>>,
) -> Result<(u16, Bytes), ClientError> {
    let url = format!("{}{path}", config.base_url);
    let unreachable = |reason: String| ClientError::Unreachable {
        url: config.base_url.clone(),
        reason,
    };
    let uri: Uri = url
        .parse()
        .map_err(|_| ClientError::BadUrl(config.base_url.clone()))?;
    let host = uri
        .host()
        .ok_or_else(|| ClientError::BadUrl(config.base_url.clone()))?
        .to_string();
    let port = uri.port_u16().unwrap_or(80);
    let stream = TcpStream::connect((host.as_str(), port))
        .await
        .map_err(|error| unreachable(error.to_string()))?;
    let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .map_err(|error| unreachable(error.to_string()))?;
    tokio::spawn(async move {
        // Ends with the request; its error, if any, also fails send_request below.
        let _ = connection.await;
    });
    let authority = uri
        .authority()
        .map(|authority| authority.to_string())
        .unwrap_or(host);
    let target = uri
        .path_and_query()
        .map(|target| target.as_str())
        .unwrap_or("/");
    let body = body.unwrap_or_default();
    let mut builder = Request::builder()
        .method(method)
        .uri(target)
        .header(hyper::header::HOST, authority)
        .header(
            hyper::header::AUTHORIZATION,
            format!("Bearer {}", config.token),
        )
        .header(hyper::header::CONTENT_LENGTH, body.len());
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    let request = builder
        .body(Full::new(Bytes::from(body)))
        .map_err(|error| ClientError::BadUrl(error.to_string()))?;
    let response = sender
        .send_request(request)
        .await
        .map_err(|error| unreachable(error.to_string()))?;
    let status = response.status().as_u16();
    let body = response
        .into_body()
        .collect()
        .await
        .map_err(|error| unreachable(error.to_string()))?
        .to_bytes();
    Ok((status, body))
}

/// The daemon's errors are `{"message": ...}` (or `{"error": ...}` for a 401).
fn error_message(body: &[u8]) -> String {
    let parsed: Option<serde_json::Value> = serde_json::from_slice(body).ok();
    parsed
        .as_ref()
        .and_then(|value| value.get("message").or_else(|| value.get("error")))
        .and_then(|message| message.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| String::from_utf8_lossy(body).chars().take(200).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_daemons_error_message() {
        assert_eq!(
            error_message(br#"{"message":"Task T-9 not found"}"#),
            "Task T-9 not found"
        );
        assert_eq!(
            error_message(br#"{"error":"unauthorized"}"#),
            "unauthorized"
        );
        assert_eq!(error_message(b"Bad Gateway"), "Bad Gateway");
    }
}
