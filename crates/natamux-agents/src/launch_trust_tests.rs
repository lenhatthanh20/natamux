use super::*;

/// The launch of an agent in a project whose commands are not approved.
fn untrusted(platform: Platform) -> LaunchOptions {
    LaunchOptions {
        program: "x".into(),
        extra_args: String::new(),
        prompt: String::new(),
        session: None,
        platform,
        claude_settings_path: "/data/claude.json".into(),
        gemini_settings_path: "/data/gemini.json".into(),
        auto: false,
        workspace: Some("/w/T-0001".into()),
        trust: false,
        workspace_hooks: false,
    }
}

#[test]
fn gives_codex_no_trust_level_for_a_project_that_is_not_approved() {
    let codex = launch_command("codex", &untrusted(Platform::Posix));
    assert!(!codex.contains("trust_level"), "{codex}");
    assert!(codex.contains("hooks.Stop="), "{codex}");
}

#[test]
fn gives_gemini_no_trust_variable_for_a_project_that_is_not_approved() {
    assert_eq!(
        launch_command("gemini", &untrusted(Platform::Posix)),
        "GEMINI_CLI_SYSTEM_SETTINGS_PATH='/data/gemini.json' x"
    );
    assert_eq!(
        launch_command("gemini", &untrusted(Platform::Windows)),
        "$env:GEMINI_CLI_SYSTEM_SETTINGS_PATH='/data/gemini.json'; x"
    );
    let own_hooks = LaunchOptions {
        workspace_hooks: true,
        ..untrusted(Platform::Posix)
    };
    assert_eq!(launch_command("gemini", &own_hooks), "x");
}

// Stage `args` reach the line as quoted words; Codex still sees the user's own flag.
#[test]
fn finds_a_quoted_no_alt_screen_word_among_the_extra_words_of_codex() {
    let options = LaunchOptions {
        extra_args: "'--no-alt-screen'".into(),
        ..untrusted(Platform::Posix)
    };
    let codex = launch_command("codex", &options);
    assert_eq!(codex.matches("--no-alt-screen").count(), 1, "{codex}");
}
