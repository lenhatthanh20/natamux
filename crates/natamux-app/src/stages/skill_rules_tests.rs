use natamux_proto::projects::repo::SkillInfo;

use super::*;

const ALL: [&str; 5] = ["claude-code", "codex", "gemini", "copilot", "cursor"];
const CHOICES: [(&str, &str); 5] = [
    ("claude-code", "Claude Code"),
    ("codex", "Codex"),
    ("gemini", "Gemini CLI"),
    ("copilot", "GitHub Copilot"),
    ("cursor", "Cursor"),
];

fn label(id: &str) -> String {
    CHOICES
        .iter()
        .find(|(choice, _)| *choice == id)
        .map_or_else(|| id.to_string(), |(_, label)| label.to_string())
}

fn skill(name: &str, description: &str, origin: &str, agents: &[&str]) -> SkillInfo {
    SkillInfo {
        name: name.into(),
        description: description.into(),
        origin: origin.into(),
        agents: agents.iter().map(|agent| agent.to_string()).collect(),
        path: format!(".claude/skills/{name}/SKILL.md"),
    }
}

fn plain(name: &str) -> SkillInfo {
    skill(name, "", "project", &ALL)
}

#[test]
fn finds_skills_by_name_and_description() {
    let skills = vec![
        skill("fetch-ticket", "Reads a Jira ticket", "project", &ALL),
        plain("deploy"),
    ];
    let names = |found: Vec<&SkillInfo>| found.iter().map(|s| s.name.clone()).collect::<Vec<_>>();
    assert_eq!(names(filter_skills(&skills, "jira")), ["fetch-ticket"]);
    assert_eq!(filter_skills(&skills, "").len(), 2);
}

#[test]
fn names_the_agents_that_have_a_skill() {
    let skill = skill("a", "", "project", &["claude-code", "codex"]);
    assert_eq!(skill_agents(&skill, label), "Claude Code and Codex");
    let none = skill_agents(&plain_with(&[]), label);
    assert_eq!(none, "No agent has it");
}

fn plain_with(agents: &[&str]) -> SkillInfo {
    skill("a", "", "project", agents)
}

#[test]
fn warns_when_some_agents_do_not_have_the_skill_or_the_team_does_not() {
    assert!(skill_warnings(&plain("a"), &CHOICES).is_empty());
    let some = plain_with(&["claude-code", "copilot", "gemini"]);
    assert_eq!(
        skill_warnings(&some, &CHOICES),
        ["Codex and Cursor do not have this skill; they get its file to read instead."]
    );
    let user = skill("a", "", "user", &ALL);
    assert_eq!(
        skill_warnings(&user, &CHOICES),
        ["Your team does not have this skill; .natamux.json is shared."]
    );
    let one = plain_with(&["claude-code", "codex", "gemini", "copilot"]);
    assert_eq!(
        skill_warnings(&one, &CHOICES),
        ["Cursor does not have this skill; it gets its file to read instead."]
    );
}

#[test]
fn groups_the_skills_by_source_and_hides_a_group_with_none() {
    let skills = vec![
        skill("mine", "", "user", &ALL),
        skill("team", "", "project", &ALL),
        skill("plug:one", "", "plugin", &ALL),
    ];
    let shown = filter_skills(&skills, "");
    let groups = group_skills(&shown);
    let heads: Vec<&str> = groups.iter().map(|(head, _)| *head).collect();
    assert_eq!(heads, ["Project", "Plugins", "Only on this machine"]);
    let only_user = filter_skills(&skills, "mine");
    let groups = group_skills(&only_user);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].0, "Only on this machine");
}

#[test]
fn says_when_no_skill_matches() {
    assert_eq!(
        no_skill_text("zzz"),
        "No skill matches \u{201c}zzz\u{201d}."
    );
}
