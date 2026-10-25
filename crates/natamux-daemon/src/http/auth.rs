//! The daemon token and the agent token (`auth.ts`).

use subtle::ConstantTimeEq;

/// Compared in constant time, so the time of a wrong guess tells nothing.
pub fn is_authorized(provided: Option<&str>, token: &str) -> bool {
    provided.is_some_and(|provided| bool::from(provided.as_bytes().ct_eq(token.as_bytes())))
}

pub fn bearer_token(header: Option<&str>) -> Option<&str> {
    header?
        .strip_prefix("Bearer ")
        .filter(|token| !token.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_the_exact_token() {
        assert!(is_authorized(Some("secret"), "secret"));
        assert!(!is_authorized(Some("secret2"), "secret"));
        assert!(!is_authorized(Some("secre"), "secret"));
        assert!(!is_authorized(Some(""), "secret"));
        assert!(!is_authorized(None, "secret"));
    }

    #[test]
    fn reads_a_bearer_header() {
        assert_eq!(bearer_token(Some("Bearer abc")), Some("abc"));
        assert_eq!(bearer_token(Some("bearer abc")), None);
        assert_eq!(bearer_token(Some("Bearer ")), None);
        assert_eq!(bearer_token(None), None);
    }
}
