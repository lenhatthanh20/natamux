//! Projects (`packages/core/src/projects/repo.ts` and `project-settings.ts`).

use serde::{Deserialize, Serialize};

/// `CONTEXT_SOURCE_TYPES`: where a task's context can come from.
pub const CONTEXT_SOURCE_TYPES: [&str; 4] = ["manual", "github-issue", "skill", "mcp"];

/// A project as `GET /repos` sends it.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Repo {
    pub id: String,
    pub name: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktrees_dir: Option<String>,
}

/// The body of `GET /repos`.
#[derive(Clone, Debug, Deserialize)]
pub struct RepoList {
    pub repos: Vec<Repo>,
}

/// `repoIdSchema`: lower case words of letters and digits joined by "-", at most 60 characters.
pub fn is_repo_id(value: &str) -> bool {
    value.len() <= 60
        && value.split('-').all(|word| {
            !word.is_empty()
                && word
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_ids_are_lower_case_words_joined_by_dashes() {
        for id in ["my-app", "natamux", "app2", "a-b-c"] {
            assert!(is_repo_id(id), "{id}");
        }
        for id in [
            "",
            "My-app",
            "my--app",
            "-app",
            "app-",
            "my_app",
            &"a".repeat(61),
        ] {
            assert!(!is_repo_id(id), "{id}");
        }
    }
}
