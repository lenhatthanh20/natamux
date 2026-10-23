//! Rules of the skill picker (`lib/skill-picker.ts`, `SkillPicker.tsx`): the groups, the filter,
//! and the lines under a skill. The agent labels come in as arguments, so these rules need no
//! window and no agent table.

use natamux_proto::projects::repo::SkillInfo;

use crate::stages::editor_labels::join_names;

/// The groups of the skill picker: shared with the team first, then this machine only. Each is the
/// origin of a skill and the heading.
pub const SKILL_GROUPS: [(&str, &str); 3] = [
    ("project", "Project"),
    ("plugin", "Plugins"),
    ("user", "Only on this machine"),
];

/// The skills whose name and description hold every word of the query.
pub fn filter_skills<'a>(skills: &'a [SkillInfo], query: &str) -> Vec<&'a SkillInfo> {
    let query = query.to_lowercase();
    let words: Vec<&str> = query.split_whitespace().collect();
    skills
        .iter()
        .filter(|skill| {
            let text = format!("{} {}", skill.name, skill.description).to_lowercase();
            words.iter().all(|word| text.contains(word))
        })
        .collect()
}

/// The groups that have a skill, in the order of `SKILL_GROUPS`, as (heading, skills).
pub fn group_skills<'a>(shown: &[&'a SkillInfo]) -> Vec<(&'static str, Vec<&'a SkillInfo>)> {
    SKILL_GROUPS
        .iter()
        .map(|(origin, label)| {
            let own = shown
                .iter()
                .copied()
                .filter(|skill| skill.origin == *origin);
            (*label, own.collect::<Vec<_>>())
        })
        .filter(|(_, own)| !own.is_empty())
        .collect()
}

// For the search of the skill picker; the app's menu has none yet.
#[cfg_attr(not(test), allow(dead_code))]
/// What the picker says when no skill is shown.
pub fn no_skill_text(query: &str) -> String {
    format!("No skill matches \u{201c}{query}\u{201d}.")
}

pub fn skill_agents(skill: &SkillInfo, agent_label: impl Fn(&str) -> String) -> String {
    if skill.agents.is_empty() {
        return "No agent has it".to_string();
    }
    join_names(
        &skill
            .agents
            .iter()
            .map(|agent| agent_label(agent))
            .collect::<Vec<_>>(),
    )
}

/// Why a skill may not work the same for everyone: agents without it get its file to read, and a
/// skill of this machine is not there for the team that shares `.natamux.json`. `choices` are the
/// agents as (id, label), in the order the pickers show them.
pub fn skill_warnings(skill: &SkillInfo, choices: &[(&str, &str)]) -> Vec<String> {
    let missing: Vec<String> = choices
        .iter()
        .filter(|(id, _)| !skill.agents.iter().any(|agent| agent == id))
        .map(|(_, label)| label.to_string())
        .collect();
    let mut warnings = Vec::new();
    match missing.as_slice() {
        [] => {}
        [one] => warnings.push(format!(
            "{one} does not have this skill; it gets its file to read instead."
        )),
        _ => warnings.push(format!(
            "{} do not have this skill; they get its file to read instead.",
            join_names(&missing)
        )),
    }
    if skill.origin == "user" {
        warnings.push("Your team does not have this skill; .natamux.json is shared.".to_string());
    }
    warnings
}

#[cfg(test)]
#[path = "skill_rules_tests.rs"]
mod tests;
