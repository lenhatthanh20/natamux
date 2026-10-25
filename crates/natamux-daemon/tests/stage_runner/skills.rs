//! `describe('skills in stage prompts')` of `stage-runner.test.ts`.

use std::fs;

use serde_json::json;

use crate::Fixture;
use crate::context::{change_stage, last_start, make_skill};

fn with_spec_prompt(f: &Fixture, prompt: &str) {
    change_stage(f, "spec", |stage| stage.prompt = Some(prompt.into()));
}

#[test]
fn calls_a_skill_the_agents_way_and_names_a_skill_that_is_missing_on_the_stage() {
    let f = Fixture::new();
    let skill = make_skill(&f.home, "review-pr");
    fs::write(
        skill.join("SKILL.md"),
        "---\nname: review-pr\ndescription: Reviews\n---\n",
    )
    .unwrap();
    with_spec_prompt(&f, "Use {skill:review-pr}, then {skill:gone}.");
    let waiting = f.created(json!({ "title": "Export tasks", "agent": "claude-code" }));
    let task = f.with_workspace(&waiting.id);

    assert!(
        f.launch_script(&task.id, "1")
            .contains("Use /review-pr, then the gone skill.")
    );
    let started = last_start(&f, &task.id, "spec");
    assert_eq!(started["missingSkills"], json!(["gone"]));
    assert_eq!(started["agent"], "claude-code");
}

#[test]
fn reads_a_plugin_that_only_the_untracked_settings_of_the_main_checkout_turn_on() {
    let f = Fixture::new();
    let install = f.home.join(".claude/plugins/cache/m/acme/1.0");
    // As the daemon joins them, so Windows gives the same path.
    let skill = install.join("skills").join("worktree");
    fs::create_dir_all(&skill).unwrap();
    fs::write(skill.join("SKILL.md"), "---\nname: worktree\n---\n").unwrap();
    let installed = json!({
        "plugins": { "acme@m": [{
            "scope": "local",
            "projectPath": f.repo.to_string_lossy(),
            "installPath": install.to_string_lossy(),
        }] }
    });
    fs::write(
        f.home.join(".claude/plugins/installed_plugins.json"),
        installed.to_string(),
    )
    .unwrap();
    fs::create_dir_all(f.repo.join(".claude")).unwrap();
    fs::write(
        f.repo.join(".claude/settings.local.json"),
        json!({ "enabledPlugins": { "acme@m": true } }).to_string(),
    )
    .unwrap();
    with_spec_prompt(&f, "Use {skill:acme:worktree}.");
    let waiting = f.created(json!({ "title": "Export tasks", "agent": "claude-code" }));
    let task = f.with_workspace(&waiting.id);

    let file = skill.join("SKILL.md");
    assert!(f.launch_script(&task.id, "1").contains(&format!(
        "Use the skill in {} (read that file and follow it).",
        file.display()
    )));
    assert!(
        last_start(&f, &task.id, "spec")
            .get("missingSkills")
            .is_none()
    );
}

#[test]
fn gives_an_agent_without_the_skill_its_file_to_read() {
    let f = Fixture::new();
    let skill = make_skill(&f.home, "review-pr");
    with_spec_prompt(&f, "Use {skill:review-pr}.");
    let waiting = f.created(json!({ "title": "Export tasks", "agent": "codex" }));
    let task = f.with_workspace(&waiting.id);

    let file = skill.join("SKILL.md");
    assert!(f.launch_script(&task.id, "1").contains(&format!(
        "Use the skill in {} (read that file and follow it).",
        file.display()
    )));
}
