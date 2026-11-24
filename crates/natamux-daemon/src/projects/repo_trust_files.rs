//! What repo trust hashes: the setup scripts and the agent config files of a folder (the project
//! folder or a task's worktree), and the digest over them with the project's commands.

use std::collections::{BTreeSet, HashSet};
use std::fs::Metadata;
use std::path::{Component, Path, PathBuf};

use natamux_agents::hook_origin::is_natamux_file;
use natamux_proto::projects::repo_trust::{RepoCommand, SetupScript};
use sha2::{Digest, Sha256};

use crate::workspaces::setup_script::{SETUP_SCRIPT, SETUP_SCRIPT_WINDOWS};

/// Files by their path from the folder, with their bytes.
pub type Files = Vec<(String, Vec<u8>)>;

/// Agent config files any adapter reads from a folder: one list for all of them. They hold hooks,
/// MCP servers and rules, which an agent runs with no prompt once the folder is trusted.
pub const CONFIG_FILES: [&str; 18] = [
    ".claude/settings.json",
    ".claude/settings.local.json",
    ".mcp.json",
    ".codex/config.toml",
    ".codex/hooks.json",
    ".cursor/mcp.json",
    ".cursor/hooks.json",
    ".cursor/cli.json",
    ".gemini/settings.json",
    ".qwen/settings.json",
    ".factory/hooks.json",
    ".factory/settings.json",
    ".factory/mcp.json",
    "opencode.json",
    "opencode.jsonc",
    ".aider.conf.yml",
    "crush.json",
    ".amp/settings.json",
];

/// Folders of agent config, read whole. `.github/hooks` counts only its `*.json` files.
pub const CONFIG_DIRS: [&str; 9] = [
    ".codex/rules",
    ".gemini/extensions",
    ".qwen/extensions",
    ".opencode",
    ".claude/agents",
    ".claude/commands",
    ".claude/skills",
    ".claude/hooks",
    GITHUB_HOOKS,
];

const GITHUB_HOOKS: &str = ".github/hooks";
/// The user's own Claude allow list when untracked: hashed in Strict mode only.
const CLAUDE_LOCAL: &str = ".claude/settings.local.json";

pub const MAX_FILES: usize = 500;
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;
pub const MAX_TOTAL_BYTES: u64 = 8 * 1024 * 1024;
const MAX_DEPTH: usize = 16;

/// Files of a folder as bytes. One that is there but cannot be read still counts, so it never
/// looks like no file.
fn read_files(folder: &Path, names: &[&str]) -> Files {
    names
        .iter()
        .filter_map(|&name| {
            let path = folder.join(name);
            match std::fs::read(&path) {
                Ok(bytes) => Some((name.to_string(), bytes)),
                Err(_) if !path.exists() => None,
                Err(error) => Some((name.to_string(), error.to_string().into_bytes())),
            }
        })
        .collect()
}

/// The setup scripts of a folder as bytes.
pub fn setup_files(folder: &Path) -> Files {
    read_files(folder, &[SETUP_SCRIPT, SETUP_SCRIPT_WINDOWS])
}

/// What the walk found: the config files, the links inside the worktree with their target text,
/// and the real paths already followed, so a loop of links is gone through once.
#[derive(Default)]
struct Found {
    paths: Vec<String>,
    links: Vec<(String, String)>,
    followed: HashSet<PathBuf>,
}

impl Found {
    /// Notes a link whose real path stays inside the worktree; one that leaves it (or leads
    /// nowhere) fails. Gives whether its target is new, so a folder is gone into once.
    fn link(&mut self, folder: &Path, rel: &str) -> Result<bool, String> {
        let path = folder.join(rel);
        let target = std::fs::read_link(&path).map_err(|error| unreadable(rel, &error))?;
        let root = folder
            .canonicalize()
            .map_err(|error| unreadable(rel, &error))?;
        let real = path
            .canonicalize()
            .map_err(|_| format!("{rel} is a link that leads nowhere"))?;
        if !real.starts_with(&root) {
            return Err(format!("{rel} is a link out of the worktree"));
        }
        if !self.links.iter().any(|(link, _)| link == rel) {
            let target = target.to_string_lossy().into_owned();
            self.links.push((rel.to_string(), target));
        }
        Ok(self.followed.insert(real))
    }
}

fn unreadable(rel: &str, error: &std::io::Error) -> String {
    format!("{rel} cannot be read: {error}")
}

/// The metadata of `rel`, through a link that stays inside the worktree, and whether the walk
/// goes into it (a link target is gone into once).
fn meta_of(folder: &Path, rel: &str, found: &mut Found) -> Result<(Metadata, bool), String> {
    let path = folder.join(rel);
    let meta = std::fs::symlink_metadata(&path).map_err(|error| unreadable(rel, &error))?;
    if !meta.file_type().is_symlink() {
        return Ok((meta, true));
    }
    let new = found.link(folder, rel)?;
    let meta = std::fs::metadata(&path).map_err(|error| unreadable(rel, &error))?;
    Ok((meta, new))
}

/// Whether `rel` is a file or a folder, None when it is not there. A link on the way counts only
/// when it stays inside the worktree.
fn kind_of(folder: &Path, rel: &str, found: &mut Found) -> Result<Option<bool>, String> {
    let parts: Vec<&str> = rel.split('/').collect();
    let mut at = String::new();
    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            at.push('/');
        }
        at.push_str(part);
        if let Err(error) = std::fs::symlink_metadata(folder.join(&at))
            && error.kind() == std::io::ErrorKind::NotFound
        {
            return Ok(None);
        }
        let (meta, _) = meta_of(folder, &at, found)?;
        if index + 1 == parts.len() {
            return Ok(Some(meta.is_dir()));
        }
        if !meta.is_dir() {
            return Ok(None);
        }
    }
    Ok(None)
}

/// The files under `dir`, sorted. A link inside the worktree is followed once; `node_modules`
/// holds packages, not config.
fn walk(folder: &Path, dir: &str, depth: usize, found: &mut Found) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Err(format!("{dir} is nested too deep"));
    }
    let entries = std::fs::read_dir(folder.join(dir))
        .map_err(|error| format!("{dir} cannot be read: {error}"))?;
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    for name in names {
        let rel = format!("{dir}/{name}");
        let flat = dir == GITHUB_HOOKS;
        if (flat && !name.ends_with(".json")) || name == "node_modules" {
            continue;
        }
        let (meta, new) = meta_of(folder, &rel, found)?;
        if meta.is_dir() {
            if !flat && new {
                walk(folder, &rel, depth + 1, found)?;
            }
        } else {
            found.paths.push(rel);
        }
        if found.paths.len() > MAX_FILES {
            return Err(format!("More than {MAX_FILES} agent config files"));
        }
    }
    Ok(())
}

/// Reads one file within the caps.
fn read_capped(path: &Path, rel: &str, total: &mut u64) -> Result<Vec<u8>, String> {
    let too_big = || format!("{rel} is over {} KiB", MAX_FILE_BYTES / 1024);
    let len = std::fs::metadata(path)
        .map_err(|error| format!("{rel} cannot be read: {error}"))?
        .len();
    if len > MAX_FILE_BYTES {
        return Err(too_big());
    }
    let bytes = std::fs::read(path).map_err(|error| format!("{rel} cannot be read: {error}"))?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(too_big());
    }
    *total += bytes.len() as u64;
    if *total > MAX_TOTAL_BYTES {
        return Err(format!(
            "Agent config files are over {} MiB in all",
            MAX_TOTAL_BYTES / 1024 / 1024
        ));
    }
    Ok(bytes)
}

/// The agent config files that are there, and the links on the way to them.
fn config_paths(folder: &Path) -> Result<Found, String> {
    let mut found = Found::default();
    // A worktree that is not there (yet, or any more) holds no config.
    if !folder.is_dir() {
        return Ok(found);
    }
    for rel in CONFIG_FILES {
        if kind_of(folder, rel, &mut found)? == Some(false) {
            found.paths.push(rel.to_string());
        }
    }
    for dir in CONFIG_DIRS {
        if kind_of(folder, dir, &mut found)? == Some(true) {
            walk(folder, dir, 0, &mut found)?;
        }
    }
    if found.paths.len() > MAX_FILES {
        return Err(format!("More than {MAX_FILES} agent config files"));
    }
    Ok(found)
}

/// Whether some folder on the way to `rel` is a link: git lists no such path (it fails on one).
fn beyond_link(rel: &str, links: &[(String, String)]) -> bool {
    links
        .iter()
        .any(|(link, _)| rel.starts_with(&format!("{link}/")))
}

/// The path git knows `rel` by: beyond a link, its real path in the worktree (the walk checked it
/// stays inside). None when that cannot be told; such a file always counts.
fn git_path(folder: &Path, rel: &str, links: &[(String, String)]) -> Option<String> {
    if !beyond_link(rel, links) {
        return Some(rel.to_string());
    }
    let root = folder.canonicalize().ok()?;
    let real = folder.join(rel).canonicalize().ok()?;
    let parts = real
        .strip_prefix(&root)
        .ok()?
        .components()
        .map(|part| match part {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect::<Option<Vec<&str>>>()?;
    Some(parts.join("/"))
}

/// Whether `folder` is in a git work tree, also one whose `.git` is broken.
fn in_git(folder: &Path) -> bool {
    folder
        .ancestors()
        .any(|dir| std::fs::symlink_metadata(dir.join(".git")).is_ok())
}

/// The agent config of a folder, or why it cannot be checked (then nothing is approved). A file
/// git tracks came from the repo and always counts. An untracked one is Natamux's own only when
/// it is exactly what Natamux writes and kept out of git.
pub fn config_sources(folder: &Path, strict: bool) -> Result<Files, String> {
    let Found { paths, links, .. } = config_paths(folder)?;
    let paths: Vec<(String, Option<String>)> = paths
        .into_iter()
        .map(|rel| {
            let git = git_path(folder, &rel, &links);
            (rel, git)
        })
        .collect();
    let plain: Vec<String> = paths.iter().filter_map(|(_, git)| git.clone()).collect();
    // Not a git folder: nothing is tracked, so nothing is the repo's by git. A git error in one
    // is no answer, so nothing there can be approved.
    let tracked = match natamux_git::tracked_paths(folder, &plain) {
        Ok(tracked) => tracked,
        Err(_) if !in_git(folder) => HashSet::new(),
        Err(error) => return Err(format!("git cannot list the tracked files: {error}")),
    };
    let mut total = 0;
    let mut files = Vec::new();
    for (rel, git) in paths {
        let untracked = git.is_some_and(|git| !tracked.contains(&git));
        if untracked && rel == CLAUDE_LOCAL && !strict {
            continue;
        }
        let bytes = read_capped(&folder.join(&rel), &rel, &mut total)?;
        if untracked && is_natamux_file(folder, &rel, &bytes) {
            continue;
        }
        files.push((rel, bytes));
    }
    // The target of a link counts too: the same bytes can come from another file.
    for (rel, target) in links {
        files.push((format!("{rel} (link)"), target.into_bytes()));
    }
    for (rel, path) in referenced_files(folder, &files) {
        if files.len() >= MAX_FILES {
            return Err(format!("More than {MAX_FILES} agent config files"));
        }
        let bytes = read_capped(&path, &rel, &mut total)?;
        files.push((rel, bytes));
    }
    Ok(files)
}

/// Config texts that can name a script: hooks and MCP servers.
const CONFIG_TEXT: [&str; 5] = [".json", ".jsonc", ".toml", ".yml", ".yaml"];
const SCRIPT_ENDS: [&str; 9] = [
    ".sh", ".bash", ".zsh", ".py", ".js", ".mjs", ".cjs", ".ts", ".ps1",
];

/// A word of a config text as a path from the folder, if it may name a file there.
fn as_relative(folder: &Path, word: &str) -> Option<String> {
    let word = ["$CLAUDE_PROJECT_DIR/", "${CLAUDE_PROJECT_DIR}/"]
        .iter()
        .find_map(|lead| word.strip_prefix(lead))
        .unwrap_or(word);
    let scripty = word.contains('/') || SCRIPT_ENDS.iter().any(|end| word.ends_with(end));
    if !scripty || word.contains('$') {
        return None;
    }
    let path = Path::new(word);
    let rel = if path.is_absolute() {
        path.strip_prefix(folder).ok()?
    } else {
        path
    };
    let mut parts = Vec::new();
    for part in rel.components() {
        match part {
            Component::Normal(name) => parts.push(name.to_str()?.to_string()),
            Component::CurDir => {}
            _ => return None,
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// Files that hook commands or MCP `command`/`args` name, inside the folder (best effort): a
/// hook that runs `./scripts/check.sh` runs what that file holds. Gives each with the real path
/// to read; a link is followed only when it stays inside the folder.
fn referenced_files(folder: &Path, files: &Files) -> Vec<(String, PathBuf)> {
    let Ok(root) = folder.canonicalize() else {
        return Vec::new();
    };
    let known: HashSet<&str> = files.iter().map(|(rel, _)| rel.as_str()).collect();
    let mut found = BTreeSet::new();
    for (rel, bytes) in files {
        if !CONFIG_TEXT.iter().any(|end| rel.ends_with(end)) {
            continue;
        }
        let text = String::from_utf8_lossy(bytes);
        let words = text.split(|c: char| {
            !(c.is_ascii_alphanumeric() || matches!(c, '.' | '/' | '_' | '-' | '$' | '{' | '}'))
        });
        for word in words.filter_map(|word| as_relative(folder, word)) {
            if !known.contains(word.as_str()) {
                found.insert(word);
            }
        }
    }
    found
        .into_iter()
        .filter_map(|rel| {
            let real = folder.join(&rel).canonicalize().ok()?;
            (real.starts_with(&root) && real.is_file()).then_some((rel, real))
        })
        .collect()
}

/// Each part goes in with its length, so no two lists give the same bytes.
pub fn feed(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

pub fn hex(hasher: Sha256) -> String {
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn files_hash(files: &[(String, Vec<u8>)]) -> String {
    let mut hasher = Sha256::new();
    for (file, bytes) in files {
        feed(&mut hasher, file.as_bytes());
        feed(&mut hasher, bytes);
    }
    hex(hasher)
}

/// The hash of one file, for the list of changed files.
pub fn file_hash(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex(hasher)
}

/// Whether a settings save would approve files the user has not seen.
pub fn unseen(files: &[(String, Vec<u8>)], approved: Option<&str>) -> bool {
    !files.is_empty() && approved != Some(files_hash(files).as_str())
}

/// SHA-256 over the commands, then the setup scripts, then the agent files, in a fixed order.
pub fn hash_of(
    commands: &[RepoCommand],
    setup: &[(String, Vec<u8>)],
    agents: &[(String, Vec<u8>)],
) -> String {
    let mut hasher = Sha256::new();
    feed(&mut hasher, b"commands");
    for command in commands {
        feed(&mut hasher, command.place.as_bytes());
        feed(&mut hasher, command.text.as_bytes());
    }
    feed(&mut hasher, b"setup");
    feed(&mut hasher, files_hash(setup).as_bytes());
    // Only when there are some, so the approvals of a repo without them stay valid.
    if !agents.is_empty() {
        feed(&mut hasher, b"agents");
        feed(&mut hasher, files_hash(agents).as_bytes());
    }
    hex(hasher)
}

pub fn shown(files: Files) -> Vec<SetupScript> {
    files
        .into_iter()
        .map(|(file, bytes)| SetupScript {
            file,
            text: String::from_utf8_lossy(&bytes).into_owned(),
        })
        .collect()
}
