//! A temp folder for tests, removed when it goes out of scope.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;
use std::sync::atomic::{AtomicU32, Ordering};

pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(label: &str) -> Self {
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let name = format!(
            "natamux-{label}-{}-{}",
            std::process::id(),
            COUNT.fetch_add(1, Ordering::Relaxed)
        );
        let path = std::env::temp_dir().join(name);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        // A test may leave a folder read-only on purpose.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A git repository with no commit. None when this machine has no git (the hardened local image
/// has none; CI does).
pub fn git_repo(label: &str) -> Option<TempDir> {
    let has_git = std::process::Command::new("git")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    if !has_git {
        return None;
    }
    let dir = TempDir::new(label);
    let made = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status()
        .unwrap();
    assert!(made.success());
    Some(dir)
}

pub fn git_status(dir: &Path) -> String {
    let output = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(dir)
        .output()
        .unwrap();
    String::from_utf8(output.stdout).unwrap()
}

/// A repository folder and a home folder in one temp folder.
pub struct Fixture {
    pub root: TempDir,
    pub repo: PathBuf,
    pub home: PathBuf,
}

impl Fixture {
    pub fn new() -> Self {
        let root = TempDir::new("skills");
        let repo = root.path().join("repo");
        let home = root.path().join("home");
        fs::create_dir(&repo).unwrap();
        fs::create_dir(&home).unwrap();
        Self { root, repo, home }
    }
}

/// A skill folder `<folder>/<name>/SKILL.md`; returns the file.
pub fn skill(folder: &Path, name: &str, description: &str) -> PathBuf {
    let dir = folder.join(name);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("SKILL.md");
    fs::write(
        &path,
        format!("---\nname: {name}\ndescription: {description}\n---\n\nSteps.\n"),
    )
    .unwrap();
    path
}

pub fn skill_help(folder: &Path, name: &str) -> PathBuf {
    skill(folder, name, &format!("{name} help"))
}

pub fn write_json(path: &Path, value: &Value) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, value.to_string()).unwrap();
}
