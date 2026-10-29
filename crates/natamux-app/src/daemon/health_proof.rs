//! Checks that a local port is answered by the daemon this app owns, without sending the token.
//! Another program can listen on a port the app picked or saved; the token must never reach it.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use sha2::{Digest, Sha256};

const MAX_RESPONSE: usize = 16 * 1024;

/// Hex SHA-256 of `natamux-health-v1\n<token>\n<nonce>`, as the daemon computes it.
pub fn proof_of(token: &str, nonce: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"natamux-health-v1\n");
    hasher.update(token.as_bytes());
    hasher.update(b"\n");
    hasher.update(nonce.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// 32 lowercase hex characters, new for every probe.
fn new_nonce() -> Option<String> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).ok()?;
    Some(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// True when the response is a 200 whose JSON body carries the proof for `token` and `nonce`.
/// The body may be chunked: the JSON is the text from the first `{` to the last `}`.
pub fn check(response: &str, token: &str, nonce: &str) -> bool {
    if !response.starts_with("HTTP/1.1 200") {
        return false;
    }
    let Some((_, body)) = response.split_once("\r\n\r\n") else {
        return false;
    };
    let (Some(start), Some(end)) = (body.find('{'), body.rfind('}')) else {
        return false;
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&body[start..=end]) else {
        return false;
    };
    json["proof"].as_str() == Some(proof_of(token, nonce).as_str())
}

/// Asks `/health` with a fresh nonce and no `Authorization` header.
pub fn daemon_proves_itself(port: u16, token: &str, timeout: Duration) -> bool {
    let Some(address) = ("127.0.0.1", port)
        .to_socket_addrs()
        .ok()
        .and_then(|mut addresses| addresses.next())
    else {
        return false;
    };
    let Some(nonce) = new_nonce() else {
        return false;
    };
    let Ok(mut stream) = TcpStream::connect_timeout(&address, timeout) else {
        return false;
    };
    if stream.set_read_timeout(Some(timeout)).is_err() {
        return false;
    }
    let request = format!(
        "GET /health?nonce={nonce} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    );
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }
    let mut response = Vec::new();
    let mut chunk = [0_u8; 1024];
    while response.len() < MAX_RESPONSE {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(size) => response.extend_from_slice(&chunk[..size]),
        }
    }
    check(&String::from_utf8_lossy(&response), token, &nonce)
}

#[cfg(test)]
#[path = "health_proof_tests.rs"]
mod tests;
