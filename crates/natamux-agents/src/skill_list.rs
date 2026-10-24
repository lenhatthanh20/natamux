//! `listSkills` of `packages/daemon/src/agents/agent-skills.ts`: the skills of every agent, merged
//! by name, for the skill picker.

use std::cmp::Ordering;
use std::path::Path;

use serde_json::{Value, json};

use crate::skill_resolve::shown_path;
use crate::skills::{SKILL_SOURCES, SkillOrigin, agent_skills};

/// A skill of the picker: the same skill of several agents, merged by name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkillEntry {
    pub name: String,
    pub description: String,
    pub origin: SkillOrigin,
    pub agents: Vec<String>,
    pub path: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SkillListing {
    pub skills: Vec<SkillEntry>,
    pub notices: Vec<String>,
}

impl SkillListing {
    /// The answer of `GET /repos/:id/skills`.
    pub fn to_json(&self) -> Value {
        let skills: Vec<Value> = self
            .skills
            .iter()
            .map(|skill| {
                json!({
                    "name": skill.name,
                    "description": skill.description,
                    "origin": skill.origin.as_str(),
                    "agents": skill.agents,
                    "path": skill.path,
                })
            })
            .collect();
        json!({ "skills": skills, "notices": self.notices })
    }
}

/// The order of `localeCompare` for the characters of skill names, close to ICU's root collation:
/// punctuation, then digits, then letters without regard to case; lower case first on a tie.
fn locale_key(c: char) -> (u32, u32) {
    const PUNCTUATION: &str = "_-,;:!?.'\"()[]{}@*/\\&#%`^+<=>|~$";
    if let Some(place) = PUNCTUATION.find(c) {
        return (1, place as u32);
    }
    if c.is_whitespace() {
        return (0, c as u32);
    }
    if c.is_ascii_digit() {
        return (2, c as u32);
    }
    let lower = c.to_lowercase().next().unwrap_or(c);
    (3, lower as u32)
}

fn locale_compare(a: &str, b: &str) -> Ordering {
    let primary = a.chars().map(locale_key).cmp(b.chars().map(locale_key));
    // Same letters: lower case sorts before upper case.
    primary.then_with(|| {
        a.chars()
            .zip(b.chars())
            .map(|(x, y)| (x.is_uppercase()).cmp(&y.is_uppercase()))
            .find(|order| order.is_ne())
            .unwrap_or(Ordering::Equal)
    })
}

/// The skills of every agent, merged by name, for the skill picker.
pub fn list_skills(repo: &Path, home: &Path) -> SkillListing {
    let mut merged: Vec<SkillEntry> = Vec::new();
    let mut notices: Vec<String> = Vec::new();
    for (agent, _) in &SKILL_SOURCES {
        let found = agent_skills(agent, repo, home);
        for notice in found.notices {
            if !notices.contains(&notice) {
                notices.push(notice);
            }
        }
        for skill in found.skills {
            let shown = shown_path(&skill.path, repo);
            let Some(known) = merged.iter_mut().find(|each| each.name == skill.name) else {
                merged.push(SkillEntry {
                    name: skill.name,
                    description: skill.description,
                    origin: skill.origin,
                    agents: vec![(*agent).to_string()],
                    path: shown,
                });
                continue;
            };
            known.agents.push((*agent).to_string());
            if skill.origin.rank() < known.origin.rank() {
                known.origin = skill.origin;
                known.path = shown;
            }
            if known.description.is_empty() {
                known.description = skill.description;
            }
        }
    }
    merged.sort_by(|a, b| {
        a.origin
            .rank()
            .cmp(&b.origin.rank())
            .then_with(|| locale_compare(&a.name, &b.name))
    });
    SkillListing {
        skills: merged,
        notices,
    }
}

#[cfg(test)]
#[path = "skill_list_tests.rs"]
mod tests;
