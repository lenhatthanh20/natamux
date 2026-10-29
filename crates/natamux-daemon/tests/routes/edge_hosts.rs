//! S-12, S-15, S-26: the Host and Origin checks, and the two ways to send the `/events` token.

use async_tungstenite::tokio::connect_async;
use async_tungstenite::tungstenite::client::IntoClientRequest;
use hyper::header::HeaderValue;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::server::{TOKEN, TestDaemon, bare};

/// The status of a raw request with the given extra header lines.
async fn status(d: &TestDaemon, path: &str, host: &str, extra: &str) -> u16 {
    let mut stream = TcpStream::connect(d.daemon.address).await.unwrap();
    let request = format!(
        "GET {path} HTTP/1.1\r\nhost: {host}\r\nauthorization: Bearer {TOKEN}\r\n{extra}\
         connection: close\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    // One read: a 101 keeps the connection open, so reading to the end would wait.
    let mut answer = vec![0; 1024];
    let read = stream.read(&mut answer).await.unwrap_or(0);
    String::from_utf8_lossy(&answer[..read])
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or(0)
}

fn upgrade(origin: Option<&str>) -> String {
    let origin = origin.map_or_else(String::new, |origin| format!("origin: {origin}\r\n"));
    format!(
        "{origin}upgrade: websocket\r\nconnection: Upgrade\r\nsec-websocket-version: 13\r\n\
         sec-websocket-key: dGhlIHNhbXBsZSBub25jZQ==\r\n"
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn answers_403_for_a_host_that_is_not_allowed_also_for_a_static_file() {
    let d = bare("edge-host").await;
    for path in ["/health", "/", "/index.html", "/tasks"] {
        assert_eq!(status(&d, path, "evil.example", "").await, 403, "{path}");
        assert_eq!(
            status(&d, path, "evil.example:4317", "").await,
            403,
            "{path}"
        );
    }
    assert_eq!(status(&d, "/health", "localhost:4317", "").await, 200);
    assert_eq!(status(&d, "/health", "127.0.0.1", "").await, 200);
    assert_eq!(status(&d, "/health", "[::1]:4317", "").await, 200);
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_a_websocket_origin_that_is_not_allowed_on_both_sockets() {
    let d = bare("edge-origin").await;
    let id = d.make_task("Socket").await;
    let terminal = format!("/tasks/{id}/terminal");
    for path in ["/events", terminal.as_str()] {
        let bad = upgrade(Some("http://evil.example"));
        assert_eq!(status(&d, path, "localhost", &bad).await, 403, "{path}");
        let local = upgrade(Some("http://localhost:5173"));
        assert_ne!(status(&d, path, "localhost", &local).await, 403, "{path}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn accepts_a_websocket_with_no_origin() {
    let d = bare("edge-no-origin").await;
    assert_eq!(
        status(&d, "/events", "localhost", &upgrade(None)).await,
        101
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn events_take_the_bearer_header_and_the_web_pages_query_token() {
    let d = bare("edge-events-token").await;
    let url = format!("ws://{}/events", d.daemon.address);
    let mut request = url.as_str().into_client_request().unwrap();
    let bearer = HeaderValue::from_str(&format!("Bearer {TOKEN}")).unwrap();
    request.headers_mut().insert("authorization", bearer);
    assert!(connect_async(request).await.is_ok());
    assert!(connect_async(url.as_str()).await.is_err());
    // The legacy web page cannot set a header on a WebSocket.
    let mut web = d.dashboard(TOKEN).await;
    assert!(web.open_after(50).await);
}
