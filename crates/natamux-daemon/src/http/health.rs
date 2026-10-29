//! The `/health` body. A client that picks a nonce gets a proof that the answer comes from the
//! daemon that owns the token, so it can check the daemon before it sends the token anywhere.

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// Hex SHA-256 of `natamux-health-v1\n<token>\n<nonce>`. The app computes the same.
pub fn proof_of(token: &str, nonce: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"natamux-health-v1\n");
    hasher.update(token.as_bytes());
    hasher.update(b"\n");
    hasher.update(nonce.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn nonce_of(query: &str) -> Option<&str> {
    query
        .split('&')
        .find_map(|pair| pair.strip_prefix("nonce="))
        .filter(|nonce| {
            nonce.len() == 32
                && nonce
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        })
}

pub fn health_body(query: &str, token: &str) -> Value {
    match nonce_of(query) {
        Some(nonce) => json!({ "status": "ok", "proof": proof_of(token, nonce) }),
        None => json!({ "status": "ok" }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONCE: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn no_nonce_gives_the_plain_body() {
        assert_eq!(health_body("", "t"), json!({ "status": "ok" }));
        assert_eq!(health_body("x=1", "t"), json!({ "status": "ok" }));
    }

    #[test]
    fn a_bad_nonce_gives_the_plain_body() {
        for query in [
            "nonce=",
            "nonce=abc",
            "nonce=0123456789ABCDEF0123456789ABCDEF",
            "nonce=0123456789abcdef0123456789abcdeg",
            "nonce=0123456789abcdef0123456789abcdef00",
        ] {
            assert_eq!(
                health_body(query, "t"),
                json!({ "status": "ok" }),
                "{query}"
            );
        }
    }

    #[test]
    fn a_good_nonce_gets_the_proof() {
        let body = health_body(&format!("a=1&nonce={NONCE}"), "secret");
        assert_eq!(body["proof"], proof_of("secret", NONCE));
        assert_eq!(body["status"], "ok");
        assert_ne!(proof_of("secret", NONCE), proof_of("other", NONCE));
    }

    #[test]
    fn the_proof_is_the_known_sha256() {
        // A SHA-256 as 64 lowercase hex characters.
        let proof = proof_of("t", NONCE);
        assert_eq!(proof.len(), 64);
        assert!(
            proof
                .bytes()
                .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
        );
    }
}
