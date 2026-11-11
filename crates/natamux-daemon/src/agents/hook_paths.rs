//! Which folder holds a path, as Node's `path.relative` reads it.

/// The parts of a path, as Node's `path.relative` reads them: `.` and `..` resolved, and on
/// Windows both slashes and any case.
pub fn parts_of(path: &str) -> Vec<String> {
    let text = if cfg!(windows) {
        path.replace('\\', "/").to_lowercase()
    } else {
        path.to_string()
    };
    let mut parts: Vec<String> = Vec::new();
    for part in text.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other.to_string()),
        }
    }
    parts
}

#[cfg(test)]
pub fn is_inside(folder: &str, path: &str) -> bool {
    is_inside_parts(folder, &parts_of(path))
}

/// `parts_of(folder)` is a start of `path`. A folder with no `.` or `..` is compared part by part
/// with no allocation: this runs for every task, under the store lock, on each hook.
pub fn is_inside_parts(folder: &str, path: &[String]) -> bool {
    let plain = !cfg!(windows) && folder.split('/').all(|part| part != "." && part != "..");
    if !plain {
        let folder = parts_of(folder);
        return !folder.is_empty() && path.starts_with(&folder);
    }
    let mut path = path.iter();
    let mut any = false;
    for part in folder.split('/').filter(|part| !part.is_empty()) {
        any = true;
        if path.next().map(String::as_str) != Some(part) {
            return false;
        }
    }
    any
}
