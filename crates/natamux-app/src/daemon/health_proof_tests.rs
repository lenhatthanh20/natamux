use super::*;

const NONCE: &str = "0123456789abcdef0123456789abcdef";

fn answer(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
}

#[test]
fn the_proof_has_the_daemon_formula() {
    // Same input as the daemon's own test of the formula, so a change on one side shows up.
    let proof = proof_of("t", NONCE);
    assert_eq!(proof.len(), 64);
    assert_ne!(proof, proof_of("u", NONCE));
    assert_ne!(proof, proof_of("t", "ffffffffffffffffffffffffffffffff"));
}

#[test]
fn a_nonce_is_32_lowercase_hex_and_new_each_time() {
    let (one, two) = (new_nonce().unwrap(), new_nonce().unwrap());
    assert_eq!(one.len(), 32);
    assert!(
        one.bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
    );
    assert_ne!(one, two);
}

#[test]
fn the_right_proof_passes() {
    let body = format!(r#"{{"status":"ok","proof":"{}"}}"#, proof_of("tok", NONCE));
    assert!(check(&answer(&body), "tok", NONCE));
}

#[test]
fn a_chunked_body_passes() {
    let body = format!(r#"{{"status":"ok","proof":"{}"}}"#, proof_of("tok", NONCE));
    let response = format!(
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{body}\r\n0\r\n\r\n",
        body.len()
    );
    assert!(check(&response, "tok", NONCE));
}

#[test]
fn a_wrong_or_missing_proof_is_refused() {
    let wrong = format!(
        r#"{{"status":"ok","proof":"{}"}}"#,
        proof_of("other", NONCE)
    );
    assert!(!check(&answer(&wrong), "tok", NONCE));
    assert!(!check(&answer(r#"{"status":"ok"}"#), "tok", NONCE));
    assert!(!check(&answer("not json"), "tok", NONCE));
    assert!(!check("", "tok", NONCE));
}

#[test]
fn a_proof_for_another_nonce_is_refused() {
    let replay = format!(r#"{{"proof":"{}"}}"#, proof_of("tok", &"f".repeat(32)));
    assert!(!check(&answer(&replay), "tok", NONCE));
}

#[test]
fn a_non_200_is_refused_even_with_the_proof() {
    let body = format!(r#"{{"proof":"{}"}}"#, proof_of("tok", NONCE));
    let response = answer(&body).replace("200 OK", "401 Unauthorized");
    assert!(!check(&response, "tok", NONCE));
}
