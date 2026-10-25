//! `packages/daemon/src/agents/agent-skills.ts`, the finding part: where each agent keeps its
//! skills. Checked against each agent's docs on 2026-10-11; see
//! `docs/agent-adapters.md`. Agents change quickly: check again before relying on it.

use std::fs;
use std::path::{Path, PathBuf};

use natamux_core::js::{is_js_space, js_trim};
use natamux_core::json::parse_js;
use serde_json::{Map, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkillOrigin {
    Project,
    Plugin,
    User,
}

impl SkillOrigin {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Plugin => "plugin",
            Self::User => "user",
        }
    }

    pub(crate) fn rank(self) -> u8 {
        self as u8
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoundSkill {
    pub name: String,
    pub description: String,
    pub origin: SkillOrigin,
    /// The SKILL.md file.
    pub path: PathBuf,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Found {
    pub skills: Vec<FoundSkill>,
    pub notices: Vec<String>,
}

pub struct SkillSource {
    /// Folders inside the repository, each with one folder per skill.
    pub project: &'static [&'static str],
    /// Folders inside the home folder.
    pub user: &'static [&'static str],
    pub plugins: Option<fn(&Path, &Path) -> Found>,
    /// How a prompt calls the skill, or None when the agent has no sure way: then it reads the
    /// file.
    pub call: fn(&str) -> Option<String>,
}

fn slash(name: &str) -> Option<String> {
    Some(format!("/{name}"))
}

fn dollar(name: &str) -> Option<String> {
    Some(format!("${name}"))
}

fn no_call(_name: &str) -> Option<String> {
    None
}

const CURSOR_FOLDERS: &[&str] = &[
    ".agents/skills",
    ".cursor/skills",
    ".claude/skills",
    ".codex/skills",
];

/// In the order of the launchable agents.
pub const SKILL_SOURCES: [(&str, SkillSource); 5] = [
    (
        "claude-code",
        SkillSource {
            project: &[".claude/skills"],
            user: &[".claude/skills"],
            plugins: Some(claude_plugins),
            call: slash,
        },
    ),
    (
        "codex",
        SkillSource {
            project: &[".agents/skills"],
            user: &[".agents/skills"],
            plugins: None,
            call: dollar,
        },
    ),
    // Gemini CLI picks a skill by its description; nothing in a prompt calls one.
    (
        "gemini",
        SkillSource {
            project: &[".gemini/skills", ".agents/skills"],
            user: &[".gemini/skills", ".agents/skills"],
            plugins: Some(gemini_extensions),
            call: no_call,
        },
    ),
    (
        "copilot",
        SkillSource {
            project: &[".github/skills", ".claude/skills", ".agents/skills"],
            user: &[".copilot/skills", ".agents/skills"],
            plugins: None,
            call: slash,
        },
    ),
    // The Cursor docs do not say that the CLI calls skills by name, so it reads the file.
    (
        "cursor",
        SkillSource {
            project: CURSOR_FOLDERS,
            user: CURSOR_FOLDERS,
            plugins: None,
            call: no_call,
        },
    ),
];

pub fn skill_source(agent: &str) -> Option<&'static SkillSource> {
    SKILL_SOURCES
        .iter()
        .find(|(name, _)| *name == agent)
        .map(|(_, source)| source)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FrontMatter {
    pub name: Option<String>,
    pub description: Option<String>,
}

/// Where the front matter ends: the text of `^---\r?\n([\s\S]*?)\r?\n---`, lazily.
fn front_matter_block(text: &str) -> Option<&str> {
    let rest = text.strip_prefix("---")?;
    let start = rest
        .strip_prefix("\r\n")
        .or_else(|| rest.strip_prefix('\n'))?;
    (0..=start.len())
        .filter(|index| start.is_char_boundary(*index))
        .find_map(|index| {
            let tail = &start[index..];
            let after = tail
                .strip_prefix("\r\n")
                .or_else(|| tail.strip_prefix('\n'))?;
            after.starts_with("---").then_some(&start[..index])
        })
}

/// `^(\w[\w-]*):\s*(.*)$` on a line: the key and the trimmed value.
fn front_field(line: &str) -> Option<(&str, &str)> {
    let mut chars = line.char_indices();
    let (_, first) = chars.next()?;
    if !(first.is_ascii_alphanumeric() || first == '_') {
        return None;
    }
    let colon = line.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))?;
    let rest = line[colon..].strip_prefix(':')?;
    // `.` does not match a line break, and `$` is the end of the text.
    if rest.contains(['\r', '\u{2028}', '\u{2029}']) {
        return None;
    }
    Some((&line[..colon], js_trim(rest)))
}

/// `^\s+\S`: starts with a space and holds some other character.
fn is_indented(line: &str) -> bool {
    line.chars().next().is_some_and(is_js_space) && line.chars().any(|c| !is_js_space(c))
}

/// `^(['"])(.*)\1$`.
fn unquote(value: &str) -> &str {
    for quote in ['\'', '"'] {
        if let Some(inner) = value
            .strip_prefix(quote)
            .and_then(|rest| rest.strip_suffix(quote))
            && !inner.contains(['\n', '\r', '\u{2028}', '\u{2029}'])
            && value.len() >= 2
        {
            return inner;
        }
    }
    value
}

/// The name and description in the front matter of a SKILL.md. Only single-line values and the
/// folded `>` and `|` forms, which is what skills use.
pub fn read_front_matter(text: &str) -> FrontMatter {
    let Some(block) = front_matter_block(text) else {
        return FrontMatter::default();
    };
    let lines: Vec<&str> = block
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect();
    let mut found = FrontMatter::default();
    let mut index = 0;
    while index < lines.len() {
        let Some((key, raw)) = front_field(lines[index]) else {
            index += 1;
            continue;
        };
        let mut value = raw.to_string();
        if matches!(raw, ">" | "|" | ">-" | "|-") {
            let mut parts: Vec<&str> = Vec::new();
            while lines.get(index + 1).is_some_and(|line| is_indented(line)) {
                index += 1;
                parts.push(js_trim(lines[index]));
            }
            value = parts.join(" ");
        }
        let value = unquote(&value).to_string();
        match key {
            "name" => found.name = Some(value),
            "description" => found.description = Some(value),
            _ => {}
        }
        index += 1;
    }
    found
}

/// The first 8000 UTF-16 units of the text, as `slice(0, 8000)`.
fn first_units(text: &str, limit: usize) -> &str {
    let mut units = 0;
    for (index, c) in text.char_indices() {
        units += c.len_utf16();
        if units > limit {
            return &text[..index];
        }
    }
    text
}

fn read_skill(folder: &Path, origin: SkillOrigin, prefix: &str) -> Option<FoundSkill> {
    let path = folder.join("SKILL.md");
    let bytes = fs::read(&path).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let front = read_front_matter(first_units(&text, 8000));
    let name = front.name.unwrap_or_else(|| {
        folder
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    });
    Some(FoundSkill {
        name: format!("{prefix}{name}"),
        description: front.description.unwrap_or_default(),
        origin,
        path,
    })
}

/// JS `sort()` order: by UTF-16 code unit.
fn sort_js(names: &mut [String]) {
    names.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
}

/// The skills in the folders of `folder`, in name order. Links count; a link to nowhere has no
/// SKILL.md and is skipped.
pub fn skills_in(folder: &Path, origin: SkillOrigin, prefix: &str) -> Vec<FoundSkill> {
    let Ok(entries) = fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_type()
                .is_ok_and(|kind| kind.is_dir() || kind.is_symlink())
        })
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    sort_js(&mut names);
    names
        .iter()
        .filter_map(|name| read_skill(&folder.join(name), origin, prefix))
        .collect()
}

fn read_json(path: &Path) -> Option<Value> {
    parse_js(&String::from_utf8_lossy(&fs::read(path).ok()?)).ok()
}

fn enabled_plugins(settings_path: &Path) -> Vec<String> {
    let Some(Value::Object(settings)) = read_json(settings_path) else {
        return Vec::new();
    };
    match settings.get("enabledPlugins") {
        Some(Value::Object(plugins)) => plugins
            .iter()
            .filter(|(_, on)| **on == Value::Bool(true))
            .map(|(id, _)| id.clone())
            .collect(),
        _ => Vec::new(),
    }
}

/// Claude Code plugins that are on in the repository's settings (shared) or the user's (this
/// machine), found through the list of installed plugins. That list is Claude Code's own file, so
/// anything unexpected in it gives no plugin skills rather than an error.
fn claude_plugins(repo: &Path, home: &Path) -> Found {
    let mut shared: Vec<String> = Vec::new();
    for file in ["settings.json", "settings.local.json"] {
        for id in enabled_plugins(&repo.join(".claude").join(file)) {
            if !shared.contains(&id) {
                shared.push(id);
            }
        }
    }
    let own: Vec<String> = enabled_plugins(&home.join(".claude").join("settings.json"))
        .into_iter()
        .filter(|id| !shared.contains(id))
        .collect();
    let installed = read_json(
        &home
            .join(".claude")
            .join("plugins")
            .join("installed_plugins.json"),
    );
    let empty = Map::new();
    let records = match &installed {
        Some(Value::Object(file)) => match file.get("plugins") {
            Some(Value::Object(records)) => records,
            _ => &empty,
        },
        _ => &empty,
    };
    let repo_text = repo.to_string_lossy();
    let mut found = Found::default();
    let ids = shared
        .iter()
        .map(|id| (id, SkillOrigin::Plugin))
        .chain(own.iter().map(|id| (id, SkillOrigin::User)));
    for (id, origin) in ids {
        let installs: Vec<&Map<String, Value>> = match records.get(id) {
            Some(Value::Array(entries)) => entries.iter().filter_map(Value::as_object).collect(),
            _ => Vec::new(),
        };
        let text = |entry: &Map<String, Value>, key: &str| {
            entry.get(key).and_then(Value::as_str).map(str::to_string)
        };
        let install = installs
            .iter()
            .find(|entry| text(entry, "projectPath").as_deref() == Some(&*repo_text))
            .or_else(|| {
                installs
                    .iter()
                    .find(|entry| text(entry, "scope").as_deref() == Some("user"))
            })
            .or(installs.first());
        let path = install.and_then(|entry| text(entry, "installPath"));
        let plugin = id.split('@').next().unwrap_or(id);
        match path {
            Some(path) if Path::new(&path).is_absolute() && Path::new(&path).exists() => {
                found.skills.extend(skills_in(
                    &Path::new(&path).join("skills"),
                    origin,
                    &format!("{plugin}:"),
                ));
            }
            _ => found.notices.push(format!(
                "The Claude Code plugin {plugin} is on, but not installed on this machine"
            )),
        }
    }
    found
}

/// Gemini CLI extensions in the home folder may hold skills. They are this machine's.
fn gemini_extensions(_repo: &Path, home: &Path) -> Found {
    let folder = home.join(".gemini").join("extensions");
    let Ok(entries) = fs::read_dir(&folder) else {
        return Found::default();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    sort_js(&mut names);
    Found {
        skills: names
            .iter()
            .flat_map(|name| skills_in(&folder.join(name).join("skills"), SkillOrigin::User, ""))
            .collect(),
        notices: Vec::new(),
    }
}

/// The skills one agent can use in `repo`, the closest first: a skill of the repository wins over
/// one of the same name in the home folder.
pub fn agent_skills(agent: &str, repo: &Path, home: &Path) -> Found {
    let Some(source) = skill_source(agent) else {
        return Found::default();
    };
    let plugins = source
        .plugins
        .map_or_else(Found::default, |find| find(repo, home));
    let mut all: Vec<FoundSkill> = Vec::new();
    for folder in source.project {
        all.extend(skills_in(&repo.join(folder), SkillOrigin::Project, ""));
    }
    all.extend(plugins.skills);
    for folder in source.user {
        all.extend(skills_in(&home.join(folder), SkillOrigin::User, ""));
    }
    let mut skills: Vec<FoundSkill> = Vec::new();
    for skill in all {
        if !skills.iter().any(|known| known.name == skill.name) {
            skills.push(skill);
        }
    }
    Found {
        skills,
        notices: plugins.notices,
    }
}

#[cfg(test)]
#[path = "skills_tests.rs"]
mod tests;
