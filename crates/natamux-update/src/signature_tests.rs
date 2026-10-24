use super::signature::{RELEASE_SIGNER, verify_download, verify_sums};

const TEST_KEY: &str = "RWRlDDYUMCwx+Oo1OxIoyy7UaN2jSLqaEfEX9pmWdYLOJWE1WCES0Jkf";
const SUMS: &str = include_str!("../tests/fixtures/minisign/SHA256SUMS.txt");
const SIGNATURE: &str = include_str!("../tests/fixtures/minisign/SHA256SUMS.txt.legacy.minisig");
const PREHASHED: &str = include_str!("../tests/fixtures/minisign/SHA256SUMS.txt.minisig");

#[test]
fn accepts_the_real_minisign_fixture() {
    assert!(verify_sums(SUMS.as_bytes(), SIGNATURE, TEST_KEY, "v1.2.3", "1.2.2").is_ok());
}

#[test]
fn rejects_tampering_downgrades_and_wrong_comments() {
    assert!(verify_sums(b"changed", SIGNATURE, TEST_KEY, "v1.2.3", "1.2.2").is_err());
    assert!(verify_sums(SUMS.as_bytes(), SIGNATURE, TEST_KEY, "v1.2.4", "1.2.2").is_err());
    assert!(verify_sums(SUMS.as_bytes(), SIGNATURE, TEST_KEY, "v1.2.3", "1.2.3").is_err());
}

#[test]
fn rejects_the_wrong_key_and_a_prehash_signature() {
    assert!(
        verify_sums(
            SUMS.as_bytes(),
            SIGNATURE,
            RELEASE_SIGNER,
            "v1.2.3",
            "1.2.2"
        )
        .is_err()
    );
    assert!(verify_sums(SUMS.as_bytes(), PREHASHED, TEST_KEY, "v1.2.3", "1.2.2").is_err());
}

#[test]
fn rejects_each_wrong_trusted_comment() {
    for comment in ["natamux v1.2.2", "natamux  v1.2.3", "v1.2.3"] {
        let changed = SIGNATURE.replace(
            "trusted comment: natamux v1.2.3",
            &format!("trusted comment: {comment}"),
        );
        assert_ne!(changed, SIGNATURE);
        assert!(verify_sums(SUMS.as_bytes(), &changed, TEST_KEY, "v1.2.3", "1.2.2").is_err());
    }
}

#[test]
fn checks_a_download_against_the_signed_hash() {
    let hash = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
    assert!(verify_download(b"hello", hash));
    assert!(!verify_download(b"changed", hash));
}
