//! Workspace trust of `packages/daemon/src/agents/agent-launch.ts`: Cursor and Copilot ask to
//! trust a folder at start, and have no flag for it in interactive mode.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use natamux_core::js::{is_js_space, iso_now, js_trim};
use natamux_core::json::{parse_js, to_js_string_pretty};
use serde_json::{Value, json};

use crate::paths::relative;

/// Where Cursor and Copilot keep the folders the user trusted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrustPaths {
    pub cursor_projects: PathBuf,
    pub copilot_config: PathBuf,
}

pub fn trust_paths(env: impl Fn(&str) -> Option<String>, home: &Path) -> TrustPaths {
    // An empty or blank variable counts as not set.
    let folder = |name: &str, default: &str| {
        env(name)
            .map(|value| js_trim(&value).to_string())
            .filter(|value| !value.is_empty())
            .map_or_else(|| home.join(default), PathBuf::from)
    };
    TrustPaths {
        cursor_projects: folder("CURSOR_DATA_DIR", ".cursor").join("projects"),
        copilot_config: folder("COPILOT_HOME", ".copilot").join("config.json"),
    }
}

/// The folder name cursor-agent gives a workspace: each run of other characters becomes one `-`,
/// and the ends are trimmed.
fn cursor_slug(workspace: &str) -> String {
    let mut slug = String::new();
    for c in workspace.chars() {
        let c = if c.is_ascii_alphanumeric() { c } else { '-' };
        if c != '-' || !slug.ends_with('-') {
            slug.push(c);
        }
    }
    slug.trim_matches('-').to_string()
}

/// Marks the worktree as trusted, so Cursor and Copilot do not ask at start. Codex and Gemini get
/// theirs on the command line. Best effort: when it fails, the agent asks as before.
pub fn trust_workspace(agent: &str, workspace: &str, paths: &TrustPaths) {
    let _ = match agent {
        "cursor" => trust_in_cursor(workspace, &paths.cursor_projects),
        "copilot" => trust_in_copilot(workspace, &paths.copilot_config),
        _ => Ok(()),
    };
}

fn trust_in_cursor(workspace: &str, projects: &Path) -> std::io::Result<()> {
    // The folder name and marker file are those cursor-agent writes when the user trusts it.
    let dir = projects.join(cursor_slug(workspace));
    fs::create_dir_all(&dir)?;
    let marker = json!({ "trustedAt": iso_now(), "workspacePath": workspace });
    // `create_new`: a marker that is there stays as it is.
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dir.join(".workspace-trusted"))?;
    file.write_all(to_js_string_pretty(&marker).as_bytes())
}

/// The leading comment lines of a config: `^(\s*\/\/[^\n]*\n)*`.
fn comment_header(text: &str) -> &str {
    let mut end = 0;
    loop {
        let rest = &text[end..];
        let body = rest.trim_start_matches(is_js_space);
        if !body.starts_with("//") {
            return &text[..end];
        }
        match body.find('\n') {
            Some(newline) => end += rest.len() - body.len() + newline + 1,
            None => return &text[..end],
        }
    }
}

/// Copilot trusts a listed folder and everything below it. Its config starts with comment lines,
/// kept as they are. A missing or broken config is left alone.
fn trust_in_copilot(workspace: &str, config_path: &Path) -> std::io::Result<()> {
    let bytes = fs::read(config_path)?;
    let text = String::from_utf8_lossy(&bytes);
    let header = comment_header(&text);
    let Ok(Value::Object(mut config)) = parse_js(&text[header.len()..]) else {
        return Ok(());
    };
    let mut folders = match config.get("trustedFolders") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(items)) => items.clone(),
        Some(_) => return Ok(()),
    };
    let covers = |folder: &Value| {
        let Some(folder) = folder.as_str() else {
            return false;
        };
        let path = relative(Path::new(folder), Path::new(workspace));
        path.split('/').next() != Some("..") && !Path::new(&path).is_absolute()
    };
    if folders.iter().any(covers) {
        return Ok(());
    }
    folders.push(Value::String(workspace.to_string()));
    config.insert("trustedFolders".into(), Value::Array(folders));
    let out = format!("{header}{}\n", to_js_string_pretty(&Value::Object(config)));
    fs::write(config_path, out)
}

#[cfg(test)]
#[path = "trust_tests.rs"]
mod tests;
