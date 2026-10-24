//! Adding and removing projects (`RepoService.add` and `remove` in `repo-service.ts`).

use std::path::{Component, Path, PathBuf};

use natamux_core::zod::{Issues, Rule, ZodError, object, string};
use natamux_git::exec::repository_root;
use natamux_proto::projects::repo::Repo;
use serde_json::{Value, json};

use crate::projects::errors::RepoError;
use crate::projects::repo_store::{basename, canonical, check_repo, real_path};
use crate::projects::repos::{DEFAULT_REPO, Repos};
use crate::shared::slug::slugify;

/// `addRepoInputSchema`.
struct AddInput {
    path: String,
    name: Option<String>,
    worktrees_dir: Option<String>,
}

fn parse_input(raw: &Value) -> Result<AddInput, RepoError> {
    let mut issues = Issues::default();
    let Some(map) = object(Some(raw), &[], &mut issues) else {
        return Err(RepoError::Invalid(ZodError(issues.0)));
    };
    let path_rules = [Rule::Trim, Rule::Min(1)];
    let name_rules = [Rule::Trim, Rule::Min(1), Rule::Max(100)];
    let path = string(map.get("path"), &[json!("path")], &path_rules, &mut issues);
    let name = map
        .get("name")
        .and_then(|value| string(Some(value), &[json!("name")], &name_rules, &mut issues));
    let dir = map.get("worktreesDir").and_then(|value| {
        string(
            Some(value),
            &[json!("worktreesDir")],
            &[Rule::Trim],
            &mut issues,
        )
    });
    let input = path.map(|path| AddInput {
        path,
        name,
        worktrees_dir: dir,
    });
    Ok(issues.into_result(input)?)
}

/// `path.resolve` of an absolute path: `..` and `.` folded, no trailing slash.
fn resolve(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

impl Repos {
    /// Adds the repository at an absolute path. Its id comes from the name (or the folder name).
    pub fn add(&self, raw: &Value) -> Result<Repo, RepoError> {
        let input = parse_input(raw)?;
        let _edit = self
            .edit
            .lock()
            .map_err(|_| RepoError::Failed("The project list failed earlier".into()))?;
        let given = Path::new(&input.path);
        if !given.is_absolute() || !given.exists() {
            return Err(RepoError::Input(format!(
                "{} is not an absolute path to a folder",
                input.path
            )));
        }
        let top = repository_root(given)?;
        let root = real_path(Path::new(&top))?;
        if canonical(&root) != canonical(&input.path) {
            return Err(RepoError::Input(format!(
                "{} is inside a repository; give its top folder {root}",
                input.path
            )));
        }
        let known = self.list()?;
        if known
            .iter()
            .any(|repo| canonical(&repo.path) == canonical(&root))
        {
            return Err(RepoError::Input(format!("{root} is added already")));
        }

        let worktrees_dir = self.own_worktrees_dir(input.worktrees_dir.as_deref())?;

        let name = input.name.unwrap_or_else(|| basename(&root));
        let base = if self.was_default(&root) {
            DEFAULT_REPO.to_string()
        } else {
            slugify(&name)
        };
        let mut id = base.clone();
        let mut number = 2;
        while !self.is_free_for(&id, &root, &known)? {
            id = format!("{base}-{number}");
            number += 1;
        }
        let mut repo = Repo {
            id,
            name,
            path: root,
            worktrees_dir,
        };
        let mut issues = Issues::default();
        check_repo(&mut repo, &[], &mut issues);
        issues.into_result(Some(()))?;
        let mut all = self.list()?;
        all.push(repo.clone());
        self.save_repos(&all)?;
        Ok(repo)
    }

    /// The folder to keep for a new project, made now. None when it is the daemon's own.
    pub(crate) fn own_worktrees_dir(&self, raw: Option<&str>) -> Result<Option<String>, RepoError> {
        let raw = match raw {
            None | Some("") => return Ok(None),
            Some(raw) => raw,
        };
        if !Path::new(raw).is_absolute() {
            // The dashboard shows errors that start with "The worktree folder " under that field.
            return Err(RepoError::Input(format!(
                "The worktree folder {raw} is not an absolute path"
            )));
        }
        let folder = resolve(Path::new(raw));
        if self
            .worktrees_dir
            .as_deref()
            .is_some_and(|own| folder == resolve(own))
        {
            return Ok(None);
        }
        std::fs::create_dir_all(&folder).map_err(|error| {
            RepoError::Input(format!(
                "The worktree folder {} cannot be made: {error}",
                folder.display()
            ))
        })?;
        Ok(Some(folder.to_string_lossy().into_owned()))
    }

    /// The tasks of a removed repository keep its id. Given to another folder, the id would move
    /// them there: their workspaces and pull requests would use the wrong repository. Tasks
    /// without a repository belong to "default", so only the folder first added as "default" gets
    /// it.
    pub(crate) fn is_free_for(
        &self,
        id: &str,
        root: &str,
        known: &[Repo],
    ) -> Result<bool, RepoError> {
        if known.iter().any(|repo| repo.id == id) {
            return Ok(false);
        }
        if id == DEFAULT_REPO {
            return Ok(self.was_default(root));
        }
        // With no removed folder on record, tasks that name the id wait for this repository.
        let removed = self.removed()?;
        let Some(path) = removed.get(id).and_then(Value::as_str) else {
            return Ok(true);
        };
        if canonical(path) == canonical(root) {
            return Ok(true);
        }
        Ok(!self
            .tasks_now()
            .iter()
            .any(|task| task.repo.as_deref() == Some(id)))
    }

    /// Forgets the repository. Its folder, branches and tasks stay. A worktree still on disk would
    /// be lost to Natamux, so its workspaces go first.
    pub fn remove(&self, id: &str) -> Result<(), RepoError> {
        let _edit = self
            .edit
            .lock()
            .map_err(|_| RepoError::Failed("The project list failed earlier".into()))?;
        let stored = self.list()?;
        let Some(removed) = stored.iter().find(|repo| repo.id == id) else {
            return Err(RepoError::NotFound(format!(
                "Repository {id} is not known. Add it first"
            )));
        };
        let in_use = self
            .tasks_now()
            .iter()
            .filter(|task| {
                task.repo.as_deref().unwrap_or(DEFAULT_REPO) == id && task.workspace.is_some()
            })
            .count();
        if in_use > 0 {
            let noun = if in_use == 1 {
                "workspace"
            } else {
                "workspaces"
            };
            return Err(RepoError::InUse(format!(
                "Remove the {in_use} {noun} of {id} first"
            )));
        }
        let mut paths = self.removed()?;
        paths.insert(id.to_string(), Value::String(removed.path.clone()));
        self.save_removed(paths)?;
        let kept: Vec<Repo> = stored
            .iter()
            .filter(|repo| repo.id != id)
            .cloned()
            .collect();
        self.save_repos(&kept)
    }
}
