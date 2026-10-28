use super::*;
use crate::agents::agent_choices::AGENT_CHOICES;

fn repo(id: &str, name: &str) -> Repo {
    Repo {
        id: id.to_owned(),
        name: name.to_owned(),
        path: format!("/work/{name}"),
        worktrees_dir: None,
    }
}

#[test]
fn project_line_is_none_without_a_project() {
    assert_eq!(project_line(&[]), None);
}

#[test]
fn project_line_names_one_project() {
    assert_eq!(
        project_line(&[repo("r1", "web")]).as_deref(),
        Some("web is ready.")
    );
}

#[test]
fn project_line_counts_the_others() {
    let repos = [repo("r1", "web"), repo("r2", "api"), repo("r3", "cli")];
    assert_eq!(
        project_line(&repos).as_deref(),
        Some("web and 2 more are ready.")
    );
}

#[test]
fn settings_target_is_the_first_id() {
    assert_eq!(
        settings_target(&[repo("r1", "web"), repo("r2", "api")]),
        Some("r1")
    );
    assert_eq!(settings_target(&[]), None);
}

#[test]
fn sign_ins_follow_the_agent_choices() {
    assert_eq!(SIGN_INS.len(), AGENT_CHOICES.len());
    for (row, choice) in SIGN_INS.iter().zip(AGENT_CHOICES.iter()) {
        assert_eq!(row.agent, choice.id);
        assert_eq!(row.label, choice.label);
    }
}

#[test]
fn sign_ins_have_the_exact_commands_and_notes() {
    let got: Vec<_> = SIGN_INS.iter().map(|row| (row.command, row.note)).collect();
    assert_eq!(
        got,
        vec![
            ("claude", Some("then /login")),
            ("codex login", None),
            ("gemini", Some("then pick a sign-in")),
            ("copilot", Some("then /login")),
            ("cursor-agent login", None),
        ]
    );
}

#[test]
fn texts_are_as_specified() {
    assert_eq!(added_text("web"), "Added web.");
    assert_eq!(copied_text("Codex"), "Copied Codex command.");
    assert_eq!(NO_PROJECT_TIP, "Add a project first");
}
