//! Node's `path.relative`, done on the text of the paths: no file is read.

use std::path::{Component, Path, PathBuf};

/// The absolute, lexically normal form of a path (`path.resolve`).
fn resolve(path: &Path) -> PathBuf {
    let full = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    let mut out = PathBuf::new();
    for part in full.components() {
        match part {
            Component::ParentDir => {
                if matches!(out.components().next_back(), Some(Component::Normal(_))) {
                    out.pop();
                }
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// `path.relative(from, to)`, with `/` between the parts. Empty for the same place.
pub fn relative(from: &Path, to: &Path) -> String {
    let from = resolve(from);
    let to = resolve(to);
    let from_parts: Vec<Component> = from.components().collect();
    let to_parts: Vec<Component> = to.components().collect();
    let common = from_parts
        .iter()
        .zip(&to_parts)
        .take_while(|(a, b)| a == b)
        .count();
    // Another drive or root: no way from one to the other.
    if common == 0 && !from_parts.is_empty() && !to_parts.is_empty() {
        return to.to_string_lossy().into_owned();
    }
    let mut parts: Vec<String> = vec!["..".to_string(); from_parts.len() - common];
    parts.extend(
        to_parts[common..]
            .iter()
            .map(|part| part.as_os_str().to_string_lossy().into_owned()),
    );
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walks_up_and_down() {
        let rel = |a: &str, b: &str| relative(Path::new(a), Path::new(b));
        assert_eq!(rel("/a/b", "/a/b/"), "");
        assert_eq!(rel("/a/b", "/a/b/c/d"), "c/d");
        assert_eq!(rel("/a/b", "/a/c"), "../c");
        assert_eq!(rel("/a/b/", "/a/x/../b/y"), "y");
        assert_eq!(rel("/", "/a"), "a");
        assert_eq!(rel("/a/T-002", "/a/T-0027"), "../T-0027");
    }
}
