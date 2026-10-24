//! Verify the legacy minisign signature before trusting a release checksum.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ring::signature::{ED25519, UnparsedPublicKey};
use sha2::{Digest, Sha256};

use crate::check::is_newer;

/// The same public key as the last Electron release (key id 3F449A1E170BB7A0).
pub const RELEASE_SIGNER: &str = "RWSgtwsXHppEP5n4kgg1Q3yNIYODQNGsMQBKF93tjSRwzL1BrjeGoBpo";
pub const MAX_SIGNED_BYTES: usize = 64 * 1024;

pub fn verify_download(bytes: &[u8], expected: &str) -> bool {
    let actual = Sha256::digest(bytes);
    let actual = actual
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    actual.eq_ignore_ascii_case(expected)
}

fn decode(text: &str, length: usize, what: &str) -> Result<Vec<u8>, String> {
    let bytes = STANDARD
        .decode(text)
        .map_err(|_| format!("{what} is not valid base64"))?;
    if bytes.len() == length {
        Ok(bytes)
    } else {
        Err(format!("{what} is not {length} bytes"))
    }
}

pub fn verify_sums(
    sums: &[u8],
    signature: &str,
    public_key: &str,
    tag: &str,
    current: &str,
) -> Result<(), String> {
    if sums.len() > MAX_SIGNED_BYTES || signature.len() > MAX_SIGNED_BYTES {
        return Err("The signed files are too large".into());
    }
    let key = decode(public_key, 42, "The release public key")?;
    if !key.starts_with(b"Ed") {
        return Err("The release public key is not an Ed25519 key".into());
    }
    let lines: Vec<&str> = signature
        .lines()
        .map(|line| line.trim_end_matches('\r'))
        .collect();
    let [untrusted, signed_line, trusted_line, global_line] = lines.as_slice() else {
        return Err("SHA256SUMS.txt.minisig is not a minisign signature".into());
    };
    if !untrusted.starts_with("untrusted comment: ") {
        return Err("The untrusted comment is missing".into());
    }
    let comment = trusted_line
        .strip_prefix("trusted comment: ")
        .ok_or("The trusted comment is missing")?;
    let signed = decode(signed_line, 74, "The file signature")?;
    if !signed.starts_with(b"Ed") || signed[2..10] != key[2..10] {
        return Err("The file is not signed by the release key".into());
    }
    let verifier = UnparsedPublicKey::new(&ED25519, &key[10..]);
    verifier
        .verify(sums, &signed[10..])
        .map_err(|_| "The signature of SHA256SUMS.txt is not valid")?;
    let global = decode(global_line, 64, "The trusted comment signature")?;
    let mut message = signed[10..].to_vec();
    message.extend_from_slice(comment.as_bytes());
    verifier
        .verify(&message, &global)
        .map_err(|_| "The trusted comment is not signed")?;
    if comment != format!("natamux {tag}") {
        return Err(format!("The signature is for {comment}, not natamux {tag}"));
    }
    if !is_newer(tag, current) {
        return Err(format!("{tag} is not newer than {current}"));
    }
    Ok(())
}
