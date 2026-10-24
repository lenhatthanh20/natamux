use super::*;
use crate::skills::SkillOrigin;
use crate::test_support::{Fixture, skill_help, write_json};
use serde_json::json;
use std::fs;
use std::path::Path;

fn context(f: &Fixture, agent: &str) -> SkillContext {
    SkillContext {
        agent: agent.into(),
        workspace: f.repo.clone(),
        repo: f.repo.clone(),
        home: f.home.clone(),
    }
}

fn call(text: &str) -> SkillResolution {
    SkillResolution::Call(text.into())
}

fn file(path: impl AsRef<Path>) -> SkillResolution {
    SkillResolution::File(path.as_ref().to_string_lossy().into_owned())
}

fn filled(text: &str, context: &SkillContext) -> (String, Vec<String>) {
    let result = fill_skills(text, context);
    (result.text, result.missing)
}

#[test]
fn uses_the_agents_own_call_when_it_has_the_skill() {
    let f = Fixture::new();
    for folder in [".claude/skills", ".agents/skills", ".github/skills"] {
        skill_help(&f.repo.join(folder), "review");
    }
    assert_eq!(
        resolve_skill("review", &context(&f, "claude-code")),
        call("/review")
    );
    assert_eq!(
        resolve_skill("review", &context(&f, "codex")),
        call("$review")
    );
    assert_eq!(
        resolve_skill("review", &context(&f, "copilot")),
        call("/review")
    );
}

#[test]
fn gives_the_file_to_read_to_an_agent_that_has_no_call_or_not_the_skill() {
    let f = Fixture::new();
    skill_help(&f.repo.join(".claude/skills"), "review");
    let want = file(".claude/skills/review/SKILL.md");
    for agent in ["gemini", "cursor", "codex", "aider"] {
        assert_eq!(
            resolve_skill("review", &context(&f, agent)),
            want,
            "{agent}"
        );
    }
}

fn install_acme(f: &Fixture, scope: &str) -> std::path::PathBuf {
    let install = f.home.join(".claude/plugins/cache/m/acme/1.0");
    skill_help(&install.join("skills"), "lint-fix");
    write_json(
        &f.home.join(".claude/plugins/installed_plugins.json"),
        &json!({ "plugins": { "acme@m": [{
            "scope": scope, "projectPath": f.repo, "installPath": install,
        }] } }),
    );
    install
}

#[test]
fn calls_a_plugin_skill_by_its_plugin_name() {
    let f = Fixture::new();
    install_acme(&f, "user");
    write_json(
        &f.repo.join(".claude/settings.json"),
        &json!({ "enabledPlugins": { "acme@m": true } }),
    );
    assert_eq!(
        resolve_skill("acme:lint-fix", &context(&f, "claude-code")),
        call("/acme:lint-fix")
    );
}

#[test]
fn prefers_the_skill_of_the_worktree_to_one_of_the_same_name_in_the_home_folder() {
    let f = Fixture::new();
    skill_help(&f.repo.join(".claude/skills"), "review");
    skill_help(&f.home.join(".claude/skills"), "review");
    assert_eq!(
        resolve_skill("review", &context(&f, "gemini")),
        file(".claude/skills/review/SKILL.md")
    );
}

#[test]
fn prints_a_path_with_spaces_whole() {
    let f = Fixture::new();
    let spaced = f.root.path().join("my home");
    let path = skill_help(&spaced.join(".claude/skills"), "review");
    let other = SkillContext {
        home: spaced,
        ..context(&f, "codex")
    };
    assert_eq!(
        filled("Use {skill:review}.", &other),
        (
            format!(
                "Use the skill in {} (read that file and follow it).",
                path.display()
            ),
            vec![]
        )
    );
}

#[test]
fn leaves_a_placeholder_with_no_name_or_other_characters_as_it_is() {
    let f = Fixture::new();
    let text = "{skill:} {skill:a b} {skill:ok?}";
    assert_eq!(
        filled(text, &context(&f, "codex")),
        (text.to_string(), vec![])
    );
}

#[test]
fn says_when_no_agent_has_the_skill() {
    let f = Fixture::new();
    assert_eq!(
        resolve_skill("gone", &context(&f, "claude-code")),
        SkillResolution::Missing
    );
}

#[test]
fn fills_each_placeholder_of_a_prompt_and_lists_the_missing_skills_once() {
    let f = Fixture::new();
    skill_help(&f.repo.join(".agents/skills"), "review");
    skill_help(&f.repo.join(".claude/skills"), "design");
    let got = filled(
        "Use {skill:review}, then {skill:design}, then {skill:gone} and {skill:gone}.",
        &context(&f, "codex"),
    );
    assert_eq!(
        got,
        (
            "Use $review, then the skill in .claude/skills/design/SKILL.md (read that file and follow it), then the gone skill and the gone skill.".to_string(),
            vec!["gone".to_string()]
        )
    );
}

#[test]
fn leaves_a_prompt_without_placeholders_as_it_is() {
    let f = Fixture::new();
    assert_eq!(
        filled("Task: {title}", &context(&f, "codex")),
        ("Task: {title}".to_string(), vec![])
    );
}

// Not in Node's tests: the hand scanner next to the regex it replaces.
#[test]
fn scans_placeholders_next_to_other_braces() {
    let f = Fixture::new();
    let got = filled(
        "{{skill:a}} {skill:a:b} {skill:a:} {skill:",
        &context(&f, "codex"),
    );
    assert_eq!(
        got,
        (
            "{the a skill} the a:b skill {skill:a:} {skill:".to_string(),
            vec!["a".to_string(), "a:b".to_string()]
        )
    );
    assert_eq!(placeholder("{skill:x.y-z_1}!"), Some((15, "x.y-z_1")));
    assert_eq!(placeholder("{skill:\u{e9}}"), None);
}

mod in_a_worktree {
    use super::*;

    struct Tree {
        f: Fixture,
        worktree: std::path::PathBuf,
    }

    impl Tree {
        fn new() -> Self {
            let f = Fixture::new();
            let worktree = f.root.path().join("worktree");
            fs::create_dir(&worktree).unwrap();
            Self { f, worktree }
        }

        fn of(&self, agent: &str) -> SkillContext {
            SkillContext {
                workspace: self.worktree.clone(),
                ..context(&self.f, agent)
            }
        }

        // A plugin `acme@m` with the given skills, installed for the main checkout.
        fn plugin(&self, skills: &[&str]) -> std::path::PathBuf {
            let install = self.f.home.join(".claude/plugins/cache/m/acme/1.0");
            fs::create_dir_all(install.join("skills")).unwrap();
            for name in skills {
                skill_help(&install.join("skills"), name);
            }
            write_json(
                &self.f.home.join(".claude/plugins/installed_plugins.json"),
                &json!({ "plugins": { "acme@m": [{
                    "scope": "local", "projectPath": self.f.repo, "installPath": install,
                }] } }),
            );
            install
        }

        fn enable(&self, settings: &str, on: bool) {
            write_json(
                &self.f.repo.join(".claude").join(settings),
                &json!({ "enabledPlugins": { "acme@m": on } }),
            );
        }
    }

    #[test]
    fn reads_a_plugin_skill_that_only_the_untracked_settings_of_the_main_checkout_turn_on() {
        let t = Tree::new();
        let install = t.plugin(&["lint-fix"]);
        t.enable("settings.local.json", true);
        let path = install.join("skills").join("lint-fix").join("SKILL.md");
        assert_eq!(
            filled("Use {skill:acme:lint-fix}.", &t.of("claude-code")),
            (
                format!(
                    "Use the skill in {} (read that file and follow it).",
                    path.display()
                ),
                vec![]
            )
        );
    }

    #[test]
    fn reads_a_skill_of_the_main_checkout_that_the_worktree_does_not_have() {
        let t = Tree::new();
        let path = skill_help(&t.f.repo.join(".claude/skills"), "review");
        assert_eq!(resolve_skill("review", &t.of("claude-code")), file(path));
    }

    #[test]
    fn prefers_the_skill_of_the_worktree_to_the_one_of_the_main_checkout() {
        let t = Tree::new();
        skill_help(&t.f.repo.join(".claude/skills"), "review");
        skill_help(&t.worktree.join(".claude/skills"), "review");
        assert_eq!(
            resolve_skill("review", &t.of("claude-code")),
            call("/review")
        );
        assert_eq!(
            resolve_skill("review", &t.of("gemini")),
            file(".claude/skills/review/SKILL.md")
        );
    }

    #[test]
    fn says_when_neither_has_the_skill() {
        let t = Tree::new();
        assert_eq!(
            resolve_skill("gone", &t.of("claude-code")),
            SkillResolution::Missing
        );
    }

    #[test]
    fn reads_a_plugin_that_the_committed_settings_of_the_main_checkout_turn_on() {
        let t = Tree::new();
        let install = t.plugin(&["lint-fix"]);
        t.enable("settings.json", true);
        assert_eq!(
            resolve_skill("acme:lint-fix", &t.of("claude-code")),
            file(install.join("skills").join("lint-fix").join("SKILL.md"))
        );
    }

    #[test]
    fn finds_no_plugin_skill_that_is_off_not_installed_or_without_skill_md() {
        let t = Tree::new();
        t.enable("settings.local.json", false);
        t.plugin(&["lint-fix"]);
        let missing = |name: &str| resolve_skill(name, &t.of("claude-code"));
        assert_eq!(missing("acme:lint-fix"), SkillResolution::Missing);

        t.enable("settings.local.json", true);
        assert_eq!(missing("acme:other"), SkillResolution::Missing);

        write_json(
            &t.f.home.join(".claude/plugins/installed_plugins.json"),
            &json!({ "plugins": {} }),
        );
        assert_eq!(missing("acme:lint-fix"), SkillResolution::Missing);
    }

    #[test]
    fn resolves_the_plugin_skills_and_the_plain_ones_of_one_prompt() {
        let t = Tree::new();
        let install = t.plugin(&["worktree"]);
        t.enable("settings.local.json", true);
        let own = skill_help(&t.f.repo.join(".claude/skills"), "review");
        let got = filled(
            "Use {skill:acme:worktree}, {skill:review} and {skill:gone}.",
            &t.of("claude-code"),
        );
        let plugin_path = install.join("skills").join("worktree").join("SKILL.md");
        assert_eq!(
            got,
            (
                format!(
                    "Use the skill in {} (read that file and follow it), the skill in {} (read that file and follow it) and the gone skill.",
                    plugin_path.display(),
                    own.display()
                ),
                vec!["gone".to_string()]
            )
        );
    }

    #[test]
    fn reads_another_agents_skill_of_the_main_checkout_not_a_folder_without_skill_md() {
        let t = Tree::new();
        let path = skill_help(&t.f.repo.join(".agents/skills"), "review");
        fs::create_dir_all(t.f.repo.join(".claude/skills/empty")).unwrap();
        assert_eq!(resolve_skill("review", &t.of("claude-code")), file(path));
        assert_eq!(
            resolve_skill("empty", &t.of("claude-code")),
            SkillResolution::Missing
        );
    }

    #[test]
    fn shows_a_main_checkout_path_whole_also_with_spaces_or_around_the_worktree() {
        let t = Tree::new();
        let spaced = t.f.root.path().join("Application Support/repo");
        let path = skill_help(&spaced.join(".claude/skills"), "review");
        let nested = spaced.join(".worktrees/T-1");
        fs::create_dir_all(&nested).unwrap();
        for workspace in [t.worktree.clone(), nested] {
            let context = SkillContext {
                agent: "claude-code".into(),
                workspace,
                repo: spaced.clone(),
                home: t.f.home.clone(),
            };
            assert_eq!(
                filled("Use {skill:review}.", &context),
                (
                    format!(
                        "Use the skill in {} (read that file and follow it).",
                        path.display()
                    ),
                    vec![]
                )
            );
        }
    }

    #[test]
    fn keeps_the_call_and_the_relative_paths_of_the_worktree() {
        let t = Tree::new();
        t.plugin(&["lint-fix"]);
        write_json(
            &t.worktree.join(".claude/settings.json"),
            &json!({ "enabledPlugins": { "acme@m": true } }),
        );
        t.enable("settings.local.json", true);
        skill_help(&t.worktree.join(".agents/skills"), "design");
        skill_help(&t.f.repo.join(".claude/skills"), "design");
        assert_eq!(
            resolve_skill("acme:lint-fix", &t.of("claude-code")),
            call("/acme:lint-fix")
        );
        assert_eq!(
            resolve_skill("design", &t.of("claude-code")),
            file(".agents/skills/design/SKILL.md")
        );
    }

    #[test]
    fn gives_no_error_for_a_main_checkout_that_is_gone_or_has_broken_settings() {
        let t = Tree::new();
        t.plugin(&["lint-fix"]);
        fs::create_dir_all(t.f.repo.join(".claude")).unwrap();
        fs::write(t.f.repo.join(".claude/settings.local.json"), "{ not json").unwrap();
        assert_eq!(
            resolve_skill("acme:lint-fix", &t.of("claude-code")),
            SkillResolution::Missing
        );
        let gone = SkillContext {
            repo: t.f.root.path().join("gone"),
            ..t.of("claude-code")
        };
        assert_eq!(
            filled("{skill:review}", &gone),
            ("the review skill".to_string(), vec!["review".to_string()])
        );
    }

    #[test]
    fn resolves_in_the_worktree_every_skill_the_picker_of_the_main_checkout_offers() {
        let t = Tree::new();
        t.plugin(&["lint-fix", "worktree"]);
        t.enable("settings.local.json", true);
        skill_help(&t.f.repo.join(".claude/skills"), "review");
        skill_help(&t.f.repo.join(".agents/skills"), "design");
        let offered: Vec<String> = crate::skill_list::list_skills(&t.f.repo, &t.f.home)
            .skills
            .into_iter()
            .map(|found| found.name)
            .collect();
        for want in ["acme:lint-fix", "acme:worktree", "review"] {
            assert!(offered.iter().any(|name| name == want), "{want}");
        }
        for name in &offered {
            let got = resolve_skill(name, &t.of("claude-code"));
            assert!(matches!(got, SkillResolution::File(_)), "{name}");
        }
        assert_eq!(SkillOrigin::Plugin.as_str(), "plugin");
    }

    #[test]
    fn treats_the_main_checkout_given_with_a_trailing_slash_as_the_worktree() {
        let f = Fixture::new();
        skill_help(&f.repo.join(".claude/skills"), "review");
        let same = SkillContext {
            repo: format!("{}/", f.repo.display()).into(),
            ..context(&f, "claude-code")
        };
        assert_eq!(resolve_skill("review", &same), call("/review"));
    }
}
