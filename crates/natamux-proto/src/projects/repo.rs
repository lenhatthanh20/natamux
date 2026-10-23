//! Projects (`packages/core/src/projects/repo.ts` and `project-settings.ts`).

use serde::{Deserialize, Serialize};

use crate::stages::schema::Step;

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

/// The body of `GET /repos`. `worktrees_dir` is the daemon's folder for worktrees, for a project
/// with none of its own.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoList {
    pub repos: Vec<Repo>,
    #[serde(default)]
    pub worktrees_dir: Option<String>,
}

/// The branches a new workspace can start from (`BranchList`, `GET /git/branches`).
/// `default_branch`: where a workspace starts when no branch is picked.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchList {
    #[serde(default)]
    pub branches: Vec<String>,
    #[serde(default)]
    pub current: Option<String>,
    #[serde(default)]
    pub default_branch: Option<String>,
}

/// One entry of `context.sources` in `.natamux.json` (`contextSourceSchema`). `kind` is one of
/// `CONTEXT_SOURCE_TYPES`; `name` and `skill` belong to a skill source, `name` and `prompt` to an
/// MCP one.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct ContextSource {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub skill: Option<String>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub prompt: Option<String>,
}

impl ContextSource {
    fn plain(kind: &str) -> ContextSource {
        ContextSource {
            kind: kind.into(),
            name: None,
            skill: None,
            agent: None,
            prompt: None,
        }
    }
}

/// The part of a project's settings the app reads: context sources, stages and the tracker. Agent
/// settings are left to the dashboard. The daemon fills the default pipeline in `stages`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct ProjectSettings {
    #[serde(default)]
    pub context: ContextSettings,
    #[serde(default)]
    pub stages: Vec<Step>,
    #[serde(default)]
    pub tracker: TrackerSettings,
}

/// `trackerSettingsSchema`: `kind` is `github` or `none`; `create_issue` is one of `never`,
/// `manual`, `on-spec-approved`, `on-pr`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackerSettings {
    #[serde(rename = "type", default = "github")]
    pub kind: String,
    #[serde(default = "manual")]
    pub create_issue: String,
    #[serde(default = "yes")]
    pub link_in_pr: bool,
}

fn github() -> String {
    "github".into()
}

fn manual() -> String {
    "manual".into()
}

fn yes() -> bool {
    true
}

impl Default for TrackerSettings {
    fn default() -> Self {
        TrackerSettings {
            kind: github(),
            create_issue: manual(),
            link_in_pr: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct ContextSettings {
    #[serde(default = "default_sources")]
    pub sources: Vec<ContextSource>,
}

impl Default for ContextSettings {
    fn default() -> Self {
        ContextSettings {
            sources: default_sources(),
        }
    }
}

/// `DEFAULT_PROJECT_SETTINGS`: manual and GitHub issue.
fn default_sources() -> Vec<ContextSource> {
    vec![
        ContextSource::plain("manual"),
        ContextSource::plain("github-issue"),
    ]
}

/// The answer of `GET /repos/:id/settings`. `text` is the file as it is (None: there is no file),
/// `version` goes back in `If-Match` with a save.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct LoadedSettings {
    #[serde(default)]
    pub settings: ProjectSettings,
    /// Why the file could not be read, if so.
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub found: bool,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
}

/// The answer of `PUT /repos/:id/settings`: the settings as saved, and the version of the file now.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct SavedSettings {
    #[serde(default)]
    pub settings: ProjectSettings,
    #[serde(default)]
    pub version: Option<String>,
}

/// A skill an agent can use (`SkillInfo` in `packages/core/src/agents/skills.ts`).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct SkillInfo {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// `project`, `plugin` or `user`.
    #[serde(default)]
    pub origin: String,
    /// The agents that have the skill.
    #[serde(default)]
    pub agents: Vec<String>,
    #[serde(default)]
    pub path: String,
}

/// The answer of `GET /repos/:id/skills`. `notices`: why some skills may be missing.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct SkillList {
    #[serde(default)]
    pub skills: Vec<SkillInfo>,
    #[serde(default)]
    pub notices: Vec<String>,
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

    #[test]
    fn the_list_has_the_worktrees_folder_when_the_daemon_sends_it() {
        let list: RepoList = serde_json::from_str(
            r#"{"repos":[{"id":"a","name":"A","path":"/a","worktreesDir":"/w"}],"worktreesDir":"/data/worktrees"}"#,
        )
        .unwrap();
        assert_eq!(list.worktrees_dir.as_deref(), Some("/data/worktrees"));
        assert_eq!(list.repos[0].worktrees_dir.as_deref(), Some("/w"));
        let older: RepoList = serde_json::from_str(r#"{"repos":[]}"#).unwrap();
        assert_eq!(older.worktrees_dir, None);
    }

    #[test]
    fn settings_read_the_stages_and_the_tracker_with_their_defaults() {
        let loaded: LoadedSettings = serde_json::from_str(
            r#"{"settings":{"stages":[{"id":"spec","kind":"agent","gate":"manual","prompt":"x"},
                {"id":"build","parallel":[{"id":"a","kind":"agent","gate":"auto"}]}],
                "tracker":{"createIssue":"on-pr"}}}"#,
        )
        .unwrap();
        assert_eq!(loaded.settings.stages.len(), 2);
        let tracker = &loaded.settings.tracker;
        assert_eq!(
            (
                tracker.kind.as_str(),
                tracker.create_issue.as_str(),
                tracker.link_in_pr
            ),
            ("github", "on-pr", true)
        );
        let plain: LoadedSettings = serde_json::from_str("{}").unwrap();
        assert!(plain.settings.stages.is_empty());
        assert_eq!(plain.settings.tracker, TrackerSettings::default());
        assert_eq!(plain.settings.tracker.create_issue, "manual");
    }

    #[test]
    fn reads_branches() {
        let list: BranchList = serde_json::from_str(
            r#"{"branches":["main","dev"],"current":"dev","defaultBranch":"main"}"#,
        )
        .unwrap();
        assert_eq!(list.branches, ["main", "dev"]);
        assert_eq!(list.default_branch.as_deref(), Some("main"));
        let none: BranchList =
            serde_json::from_str(r#"{"branches":[],"current":null,"defaultBranch":null}"#).unwrap();
        assert_eq!(none.current, None);
    }

    #[test]
    fn settings_read_the_context_sources_and_ignore_the_rest() {
        let loaded: LoadedSettings = serde_json::from_str(
            r#"{"settings":{"stages":[],"context":{"sources":[{"type":"manual"},
                {"type":"skill","name":"jira","skill":"jira-issue","agent":"claude-code"},
                {"type":"mcp","name":"m","prompt":"Read {ref}"}]},
                "tracker":{"type":"github"},"agents":{"permissionMode":"auto"}},
                "error":null,"found":true,"text":"{}","version":"v","path":"/r/.natamux.json"}"#,
        )
        .unwrap();
        let sources = &loaded.settings.context.sources;
        assert_eq!(sources.len(), 3);
        assert_eq!(sources[1].kind, "skill");
        assert_eq!(sources[1].skill.as_deref(), Some("jira-issue"));
        assert_eq!(sources[2].prompt.as_deref(), Some("Read {ref}"));
        assert!(loaded.found);
        assert_eq!(loaded.path, "/r/.natamux.json");
        assert_eq!(loaded.text.as_deref(), Some("{}"));
        assert_eq!(loaded.version.as_deref(), Some("v"));
    }

    #[test]
    fn reads_the_answer_of_a_save() {
        let saved: SavedSettings = serde_json::from_str(
            r#"{"settings":{"stages":[{"id":"spec","kind":"agent","gate":"manual"}]},"version":"v2"}"#,
        )
        .unwrap();
        assert_eq!(saved.settings.stages.len(), 1);
        assert_eq!(saved.version.as_deref(), Some("v2"));
    }

    #[test]
    fn a_project_with_no_file_gets_the_default_sources() {
        let loaded: LoadedSettings = serde_json::from_str(
            r#"{"settings":{"context":{"sources":[{"type":"manual"},{"type":"github-issue"}]}},
                "error":null,"found":false,"path":"/r/.natamux.json","text":null,"version":"none"}"#,
        )
        .unwrap();
        assert_eq!(loaded.settings, ProjectSettings::default());
        assert!(!loaded.found);
        let empty: ProjectSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(empty.context.sources.len(), 2);
    }

    #[test]
    fn reads_skills_and_notices() {
        let list: SkillList = serde_json::from_str(
            r#"{"skills":[{"name":"validate","description":"Run checks","origin":"project",
                "agents":["claude-code","copilot"],"path":".claude/skills/validate/SKILL.md"}],
                "notices":["Plugin x is not installed"]}"#,
        )
        .unwrap();
        assert_eq!(list.skills[0].agents, ["claude-code", "copilot"]);
        assert_eq!(list.notices.len(), 1);
    }
}
