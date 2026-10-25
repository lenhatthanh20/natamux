//! The dashboard's files and HTML fallback. API and socket paths stay in the normal router.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use http_body_util::Full;
use hyper::Method;
use hyper::body::Bytes;
use hyper::header::{CACHE_CONTROL, CONTENT_TYPE, HeaderValue};

use super::errors::Answer;

include!(concat!(env!("OUT_DIR"), "/web_assets.rs"));

const PAGE_CSP: &str = "default-src 'self'; connect-src 'self' ws: wss:; img-src 'self' data: blob:; worker-src 'self' blob:; style-src 'self' 'unsafe-inline'; frame-ancestors 'none'";

pub struct WebAssets {
    dir: Option<PathBuf>,
    embedded: HashMap<String, Vec<u8>>,
}

impl WebAssets {
    pub fn new(dir: Option<PathBuf>) -> Self {
        Self {
            dir,
            embedded: EMBEDDED_WEB
                .iter()
                .map(|(name, bytes)| (String::from(*name), bytes.to_vec()))
                .collect(),
        }
    }

    #[cfg(test)]
    pub(super) fn for_test(files: &[(&str, &[u8])]) -> Self {
        Self {
            dir: None,
            embedded: files
                .iter()
                .map(|(name, bytes)| (String::from(*name), bytes.to_vec()))
                .collect(),
        }
    }

    #[cfg(test)]
    pub(super) fn with_dir(mut self, dir: PathBuf) -> Self {
        self.dir = Some(dir);
        self
    }

    pub(super) fn bytes_for(&self, url: &str) -> Option<Vec<u8>> {
        let path = clean_path(url)?;
        match &self.dir {
            Some(root) => {
                let root = root.canonicalize().ok()?;
                let file = root.join(path).canonicalize().ok()?;
                file.starts_with(&root)
                    .then(|| std::fs::read(file).ok())
                    .flatten()
            }
            None => self
                .embedded
                .get(&path.to_string_lossy().replace('\\', "/"))
                .cloned(),
        }
    }

    /// A known file may load without a token. Only HTML navigation gets the index fallback.
    pub fn answer(&self, method: &Method, url: &str, accept: &str) -> Option<Answer> {
        if *method != Method::GET || url == "/api" || url.starts_with("/api/") {
            return None;
        }
        let path = url.split('?').next().unwrap_or(url);
        let clean = if path == "/" {
            PathBuf::from("index.html")
        } else {
            clean_path(path)?
        };
        if reserved_path(&clean) {
            return None;
        }
        let file = if path == "/" { "/index.html" } else { path };
        let (bytes, name) = match self.bytes_for(file) {
            Some(bytes) => (bytes, file),
            None if accept.contains("text/html") => (self.index()?, "/index.html"),
            None => return None,
        };
        let kind = content_type(name);
        let mut answer = Answer::new(Full::new(Bytes::from(bytes)));
        let headers = answer.headers_mut();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static(kind));
        headers.insert(
            CACHE_CONTROL,
            HeaderValue::from_static(if path.starts_with("/assets/") && name == path {
                "public, max-age=31536000, immutable"
            } else {
                "no-cache"
            }),
        );
        headers.insert(
            "x-content-type-options",
            HeaderValue::from_static("nosniff"),
        );
        if kind.starts_with("text/html") {
            headers.insert("x-frame-options", HeaderValue::from_static("DENY"));
            headers.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
            headers.insert(
                "content-security-policy",
                HeaderValue::from_static(PAGE_CSP),
            );
        }
        Some(answer)
    }

    fn index(&self) -> Option<Vec<u8>> {
        self.bytes_for("/index.html")
    }
}

/// Decode a URL path one segment at a time, so encoded separators and traversal cannot escape.
pub(super) fn clean_path(url: &str) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for segment in url.split('?').next()?.trim_start_matches('/').split('/') {
        if segment.is_empty() {
            continue;
        }
        let decoded = decode_segment(segment)?;
        if decoded == "."
            || decoded == ".."
            || decoded
                .chars()
                .any(|part| matches!(part, '/' | '\\' | ':' | '\0'))
        {
            return None;
        }
        out.push(decoded);
    }
    (!out.as_os_str().is_empty()).then_some(out)
}

fn decode_segment(segment: &str) -> Option<String> {
    let bytes = segment.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            let hi = bytes.get(at + 1)?.to_ascii_uppercase();
            let lo = bytes.get(at + 2)?.to_ascii_uppercase();
            let hex = |value: u8| match value {
                b'0'..=b'9' => Some(value - b'0'),
                b'A'..=b'F' => Some(value - b'A' + 10),
                _ => None,
            };
            out.push(hex(hi)? * 16 + hex(lo)?);
            at += 3;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn reserved_path(path: &Path) -> bool {
    matches!(
        path.components()
            .next()
            .and_then(|part| part.as_os_str().to_str()),
        Some(
            "health"
                | "api"
                | "events"
                | "tasks"
                | "templates"
                | "workspaces"
                | "repos"
                | "git"
                | "github"
                | "hooks"
                | "diffstats"
                | "jobs"
                | "activity"
                | "usage"
        )
    )
}

fn content_type(path: &str) -> &'static str {
    match Path::new(path).extension().and_then(|ext| ext.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("json" | "map") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("ttf") => "font/ttf",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("wasm") => "application/wasm",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}
