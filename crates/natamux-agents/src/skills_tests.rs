use super::*;
use crate::test_support::{Fixture, skill, skill_help, write_json};
use serde_json::json;

fn names(found: &Found) -> Vec<&str> {
    found
        .skills
        .iter()
        .map(|skill| skill.name.as_str())
        .collect()
}

#[test]
fn reads_one_line_and_folded_values_without_quotes() {
    let text = [
        "---",
        "name: \"fetch-ticket\"",
        "description: >",
        "  Reads a ticket",
        "  with its comments.",
        "license: MIT",
        "---",
        "Body",
    ]
    .join("\n");
    assert_eq!(
        read_front_matter(&text),
        FrontMatter {
            name: Some("fetch-ticket".into()),
            description: Some("Reads a ticket with its comments.".into()),
        }
    );
}

#[test]
fn gives_nothing_for_a_file_without_front_matter() {
    assert_eq!(read_front_matter("# Just text"), FrontMatter::default());
}

#[test]
fn has_a_skill_source_for_every_agent_natamux_starts() {
    let mut sources: Vec<&str> = SKILL_SOURCES.iter().map(|(name, _)| *name).collect();
    let mut agents: Vec<&str> = natamux_proto::agents::start::LAUNCHABLE_AGENTS.to_vec();
    sources.sort_unstable();
    agents.sort_unstable();
    assert_eq!(sources, agents);
}

#[test]
fn reads_each_agents_own_folders() {
    let f = Fixture::new();
    skill_help(&f.repo.join(".claude/skills"), "claude-only");
    skill_help(&f.repo.join(".agents/skills"), "shared");
    skill_help(&f.repo.join(".github/skills"), "copilot-only");
    skill_help(&f.home.join(".gemini/skills"), "gemini-mine");
    let of = |agent: &str| {
        agent_skills(agent, &f.repo, &f.home)
            .skills
            .into_iter()
            .map(|skill| skill.name)
            .collect::<Vec<_>>()
    };

    assert_eq!(of("claude-code"), ["claude-only"]);
    assert_eq!(of("codex"), ["shared"]);
    assert_eq!(of("gemini"), ["shared", "gemini-mine"]);
    assert_eq!(of("copilot"), ["copilot-only", "claude-only", "shared"]);
    assert_eq!(of("cursor"), ["shared", "claude-only"]);
    assert!(of("aider").is_empty());
}

#[test]
fn prefers_a_skill_of_the_repository_to_one_of_the_same_name_in_the_home_folder() {
    let f = Fixture::new();
    skill(&f.repo.join(".claude/skills"), "review", "Project review");
    skill(&f.home.join(".claude/skills"), "review", "My review");
    let found = agent_skills("claude-code", &f.repo, &f.home).skills;
    assert_eq!(found[0].origin, SkillOrigin::Project);
    assert_eq!(found[0].description, "Project review");
}

#[test]
fn reads_the_skills_of_claude_code_plugins_that_are_on_and_installed() {
    let f = Fixture::new();
    let cache = f.home.join(".claude/plugins/cache/m");
    let shared = cache.join("design/1.0");
    let mine = cache.join("notes/2.0");
    skill_help(&shared.join("skills"), "frontend");
    skill_help(&mine.join("skills"), "jot");
    write_json(
        &f.repo.join(".claude/settings.json"),
        &json!({ "enabledPlugins": { "design@m": true, "off@m": false, "gone@m": true } }),
    );
    write_json(
        &f.home.join(".claude/settings.json"),
        &json!({ "enabledPlugins": { "notes@m": true } }),
    );
    write_json(
        &f.home.join(".claude/plugins/installed_plugins.json"),
        &json!({
            "version": 2,
            "plugins": {
                "design@m": [{ "scope": "project", "projectPath": f.repo, "installPath": shared }],
                "notes@m": [{ "scope": "user", "installPath": mine }],
            },
        }),
    );

    let found = agent_skills("claude-code", &f.repo, &f.home);
    let pairs: Vec<(&str, SkillOrigin)> = found
        .skills
        .iter()
        .map(|skill| (skill.name.as_str(), skill.origin))
        .collect();
    assert_eq!(
        pairs,
        [
            ("design:frontend", SkillOrigin::Plugin),
            ("notes:jot", SkillOrigin::User)
        ]
    );
    assert_eq!(
        found.notices,
        ["The Claude Code plugin gone is on, but not installed on this machine"]
    );
}

#[test]
fn gives_no_plugin_skills_and_no_error_when_the_plugin_files_are_not_what_it_expects() {
    let f = Fixture::new();
    write_json(
        &f.repo.join(".claude/settings.json"),
        &json!({ "enabledPlugins": ["design@m"] }),
    );
    fs::write(f.repo.join(".claude/settings.local.json"), "{ broken").unwrap();
    write_json(
        &f.home.join(".claude/plugins/installed_plugins.json"),
        &json!({ "plugins": "nope" }),
    );
    assert_eq!(
        agent_skills("claude-code", &f.repo, &f.home),
        Found::default()
    );
}

#[test]
fn names_a_skill_without_front_matter_by_its_folder_and_skips_a_folder_without_skill_md() {
    let f = Fixture::new();
    let skills = f.repo.join(".claude/skills");
    fs::create_dir_all(skills.join("plain")).unwrap();
    fs::write(skills.join("plain/SKILL.md"), "# Plain\n").unwrap();
    fs::create_dir_all(skills.join("empty")).unwrap();
    assert_eq!(
        agent_skills("claude-code", &f.repo, &f.home).skills,
        [FoundSkill {
            name: "plain".into(),
            description: String::new(),
            origin: SkillOrigin::Project,
            path: skills.join("plain/SKILL.md"),
        }]
    );
}

#[cfg(unix)]
#[test]
fn skips_a_skill_folder_that_is_a_broken_link() {
    let f = Fixture::new();
    let skills = f.repo.join(".claude/skills");
    fs::create_dir_all(&skills).unwrap();
    std::os::unix::fs::symlink(f.root.path().join("nowhere"), skills.join("gone")).unwrap();
    skill_help(&skills, "kept");
    assert_eq!(
        names(&agent_skills("claude-code", &f.repo, &f.home)),
        ["kept"]
    );
}

#[test]
fn gives_no_plugin_skills_when_the_list_of_installed_plugins_is_not_json() {
    let f = Fixture::new();
    write_json(
        &f.repo.join(".claude/settings.json"),
        &json!({ "enabledPlugins": { "design@m": true } }),
    );
    fs::create_dir_all(f.home.join(".claude/plugins")).unwrap();
    fs::write(
        f.home.join(".claude/plugins/installed_plugins.json"),
        "nope",
    )
    .unwrap();
    skill_help(&f.repo.join(".claude/skills"), "review");
    let found = agent_skills("claude-code", &f.repo, &f.home);
    assert_eq!(names(&found), ["review"]);
    assert_eq!(
        found.notices,
        ["The Claude Code plugin design is on, but not installed on this machine"]
    );
}

#[test]
fn reads_the_skills_of_gemini_cli_extensions() {
    let f = Fixture::new();
    skill_help(&f.home.join(".gemini/extensions/tools/skills"), "lint-fix");
    assert_eq!(
        names(&agent_skills("gemini", &f.repo, &f.home)),
        ["lint-fix"]
    );
}

// Not in Node's tests: the small rules that Rust has to do by hand.
#[test]
fn reads_front_matter_edge_cases_as_the_regexes_do() {
    let name = |text: &str| read_front_matter(text).name;
    assert_eq!(name("---\r\nname: a\r\n---\r\nBody"), Some("a".into()));
    assert_eq!(name("---\nname: 'it'\n---"), Some("it".into()));
    assert_eq!(name("---\nname:\n---\nx"), Some(String::new()));
    assert_eq!(name("---\nname: x\n"), None);
    assert_eq!(name("--- \nname: x\n---"), None);
    assert_eq!(
        read_front_matter("---\ndescription: |-\n  a\n\n  b\nname: n\n---").description,
        Some("a".into())
    );
}

#[test]
fn cuts_the_text_at_8000_utf16_units() {
    let text = "a".repeat(7999) + "\u{1F600}b";
    assert_eq!(first_units(&text, 8000).len(), 7999);
    assert_eq!(first_units("short", 8000), "short");
}
