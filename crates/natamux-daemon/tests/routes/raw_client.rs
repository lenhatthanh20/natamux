//! A request sent as raw bytes, so a path with "\\" reaches the daemon as it is.

use tokio::net::TcpStream;

use crate::server::TestDaemon;

impl TestDaemon {
    /// The status of a request sent as raw bytes, so a path with "\\" reaches the daemon as it is.
    pub async fn raw_status(&self, method: &str, path: &str) -> u16 {
        self.raw_status_as(method, path, None).await
    }

    /// `raw_status` with a bearer token.
    pub async fn raw_status_as(&self, method: &str, path: &str, token: Option<&str>) -> u16 {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = TcpStream::connect(self.daemon.address).await.unwrap();
        let auth = token.map_or_else(String::new, |token| {
            format!("authorization: Bearer {token}\r\n")
        });
        let request = format!(
            "{method} {path} HTTP/1.1\r\nhost: localhost\r\n{auth}connection: close\r\n\r\n"
        );
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut answer = Vec::new();
        let _ = stream.read_to_end(&mut answer).await;
        let text = String::from_utf8_lossy(&answer);
        text.split_whitespace()
            .nth(1)
            .and_then(|code| code.parse().ok())
            .unwrap_or(0)
    }
}
