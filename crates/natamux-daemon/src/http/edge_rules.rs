//! Host and Origin checks (S-12, S-26): a web page on another site cannot reach the daemon, not
//! even through DNS rebinding. Hostnames only: nginx sends `$host` without the port.

use hyper::header::{HOST, ORIGIN};
use hyper::{HeaderMap, StatusCode};

use super::errors::{Answer, error_answer};

pub const DEFAULT_ALLOWED_HOSTS: [&str; 3] = ["127.0.0.1", "localhost", "::1"];

/// The hostname of a `Host` value or of the authority of a URL: no port, no IPv6 brackets.
fn hostname(authority: &str) -> String {
    let name = if let Some(rest) = authority.strip_prefix('[') {
        rest.split(']').next().unwrap_or(rest)
    } else if authority.matches(':').count() == 1 {
        authority.split(':').next().unwrap_or(authority)
    } else {
        authority
    };
    name.to_ascii_lowercase()
}

fn listed(name: &str, allowed: &[String]) -> bool {
    !name.is_empty() && allowed.iter().any(|host| host.eq_ignore_ascii_case(name))
}

/// A missing `Host` is let through: a browser always sends one, and this check exists for
/// browsers. A native client or a test that sends none has no rebinding attack to fear.
pub fn host_allowed(host_header: Option<&str>, allowed: &[String]) -> bool {
    host_header.is_none_or(|host| listed(&hostname(host.trim()), allowed))
}

/// No `Origin` means no browser (the native client): fine. A browser sends it on every
/// WebSocket handshake, and its hostname must be allowed.
pub fn origin_allowed(origin: Option<&str>, allowed: &[String]) -> bool {
    let Some(origin) = origin else {
        return true;
    };
    let Some((_, rest)) = origin.trim().split_once("://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    // `http://localhost@evil.example` is the host evil.example.
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    listed(&hostname(authority), allowed)
}

fn header(headers: &HeaderMap, name: hyper::header::HeaderName) -> Option<&str> {
    // A value that is not text cannot be a host we allow.
    headers
        .get(name)
        .map(|value| value.to_str().unwrap_or("\u{0}"))
}

fn refused(what: &str) -> Answer {
    error_answer(
        StatusCode::FORBIDDEN,
        "forbidden",
        &format!("This {what} is not allowed. Set NATAMUX_ALLOWED_HOSTS to add a host name."),
    )
}

/// The 403 for a request whose `Host` is not allowed.
pub fn check_host(headers: &HeaderMap, allowed: &[String]) -> Option<Answer> {
    (!host_allowed(header(headers, HOST), allowed)).then(|| refused("host"))
}

/// The 403 for a WebSocket handshake whose `Origin` is not allowed.
pub fn check_origin(headers: &HeaderMap, allowed: &[String]) -> Option<Answer> {
    (!origin_allowed(header(headers, ORIGIN), allowed)).then(|| refused("origin"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defaults() -> Vec<String> {
        DEFAULT_ALLOWED_HOSTS.map(String::from).to_vec()
    }

    #[test]
    fn host_takes_the_defaults_with_or_without_a_port() {
        let allowed = defaults();
        for host in ["localhost", "localhost:4317", "127.0.0.1:8080", "LocalHost"] {
            assert!(host_allowed(Some(host), &allowed), "{host}");
        }
        for host in ["[::1]:4317", "[::1]", "::1"] {
            assert!(host_allowed(Some(host), &allowed), "{host}");
        }
    }

    #[test]
    fn host_refuses_others_and_a_name_that_only_starts_like_one() {
        let allowed = defaults();
        for host in [
            "evil.example",
            "evil.example:4317",
            "localhost.evil.example",
            "",
        ] {
            assert!(!host_allowed(Some(host), &allowed), "{host}");
        }
        assert!(!host_allowed(Some("[::2]:4317"), &allowed));
    }

    #[test]
    fn host_takes_a_listed_extra_name_and_a_missing_header() {
        let mut allowed = defaults();
        allowed.push("daemon".into());
        assert!(host_allowed(Some("daemon:4317"), &allowed));
        assert!(host_allowed(None, &allowed));
    }

    #[test]
    fn origin_is_free_when_absent_and_checked_by_hostname_when_present() {
        let allowed = defaults();
        assert!(origin_allowed(None, &allowed));
        for origin in [
            "http://localhost:5173",
            "https://127.0.0.1",
            "http://[::1]:8080",
        ] {
            assert!(origin_allowed(Some(origin), &allowed), "{origin}");
        }
        for origin in [
            "http://evil.example",
            "null",
            "http://localhost@evil.example",
            "http://evil.example/localhost",
            "evil.example",
        ] {
            assert!(!origin_allowed(Some(origin), &allowed), "{origin}");
        }
    }
}
