//! One HTTP/1.1 request to the daemon, with its bearer token. The daemon listens on plain HTTP
//! on this machine, so a connection per request is cheap and needs no pool.

use std::time::Duration;

use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::{Method, Request, Uri};
use hyper_util::rt::TokioIo;
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::net::TcpStream;

use crate::config::Config;
use crate::error::ClientError;

pub async fn call<T: DeserializeOwned>(
    config: &Config,
    method: Method,
    path: &str,
) -> Result<T, ClientError> {
    read(&send(config, method, path, None).await?)
}

/// A request with a JSON body, and its JSON answer.
pub async fn call_with<T: DeserializeOwned>(
    config: &Config,
    method: Method,
    path: &str,
    body: &impl Serialize,
) -> Result<T, ClientError> {
    let body =
        serde_json::to_vec(body).map_err(|error| ClientError::BadAnswer(error.to_string()))?;
    read(&send(config, method, path, Some(body)).await?)
}

fn read<T: DeserializeOwned>(body: &[u8]) -> Result<T, ClientError> {
    serde_json::from_slice(body).map_err(|error| ClientError::BadAnswer(error.to_string()))
}

/// A read that hangs (a slow `git` behind `/diffstats`) must not stop the live feed, which reads
/// one route after the other. Writes have no limit: git work can take long and still succeed.
const READ_LIMIT: Duration = Duration::from_secs(30);

/// `json`: a JSON body. Without one there is no content type, as the daemon refuses an empty
/// JSON body.
pub async fn send(
    config: &Config,
    method: Method,
    path: &str,
    json: Option<Vec<u8>>,
) -> Result<Bytes, ClientError> {
    send_within(config, method, path, json, READ_LIMIT).await
}

async fn send_within(
    config: &Config,
    method: Method,
    path: &str,
    json: Option<Vec<u8>>,
    read_limit: Duration,
) -> Result<Bytes, ClientError> {
    let headers: &[(&str, &str)] = match json {
        Some(_) => &[("content-type", "application/json")],
        None => &[],
    };
    let is_read = method == Method::GET;
    let answer = request(config, method, path, headers, json);
    let (status, body) = if is_read {
        tokio::time::timeout(read_limit, answer)
            .await
            .map_err(|_| ClientError::Unreachable {
                url: config.base_url.clone(),
                reason: format!("the daemon did not answer in {} s", read_limit.as_secs()),
            })??
    } else {
        answer.await?
    };
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

    #[test]
    fn a_read_that_gets_no_answer_fails_after_the_limit() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let answer = rt.block_on(async {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            // Takes the connection and never answers.
            tokio::spawn(async move {
                let held = listener.accept().await;
                std::future::pending::<()>().await;
                drop(held);
            });
            let config = Config {
                base_url: format!("http://127.0.0.1:{port}"),
                token: "t".into(),
            };
            let limit = Duration::from_millis(200);
            send_within(&config, Method::GET, "/diffstats", None, limit).await
        });
        assert!(
            matches!(&answer, Err(ClientError::Unreachable { reason, .. }) if reason.contains("did not answer")),
            "{answer:?}"
        );
    }
}
