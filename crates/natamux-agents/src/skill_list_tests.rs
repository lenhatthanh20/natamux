use super::*;
use crate::test_support::{Fixture, skill_help};

#[test]
fn merges_the_skills_of_all_agents_by_name_the_shared_ones_first() {
    let f = Fixture::new();
    skill_help(&f.home.join(".agents/skills"), "mine");
    skill_help(&f.repo.join(".claude/skills"), "review");
    skill_help(&f.repo.join(".agents/skills"), "review");
    let list = list_skills(&f.repo, &f.home);
    assert_eq!(
        list.skills,
        [
            SkillEntry {
                name: "review".into(),
                description: "review help".into(),
                origin: SkillOrigin::Project,
                agents: ["claude-code", "codex", "gemini", "copilot", "cursor"]
                    .map(String::from)
                    .to_vec(),
                path: ".claude/skills/review/SKILL.md".into(),
            },
            SkillEntry {
                name: "mine".into(),
                description: "mine help".into(),
                origin: SkillOrigin::User,
                agents: ["codex", "gemini", "copilot", "cursor"]
                    .map(String::from)
                    .to_vec(),
                path: f
                    .home
                    .join(".agents/skills")
                    .join("mine")
                    .join("SKILL.md")
                    .to_string_lossy()
                    .into_owned(),
            },
        ]
    );
    assert!(list.notices.is_empty());
}

#[test]
fn sorts_names_like_locale_compare() {
    let order = |a: &str, b: &str| locale_compare(a, b);
    assert_eq!(order("a-b", "a:b"), Ordering::Less);
    assert_eq!(order("a:b", "a.b"), Ordering::Less);
    assert_eq!(order("a_b", "a1"), Ordering::Less);
    assert_eq!(order("a1", "ab"), Ordering::Less);
    assert_eq!(order("B", "a"), Ordering::Greater);
    assert_eq!(order("a", "A"), Ordering::Less);
}
