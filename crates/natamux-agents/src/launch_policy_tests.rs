use super::*;
use crate::launch::{AUTO_FLAGS, LAUNCHABLE_AGENTS};

fn word(agent: &str, words: &str) -> Option<String> {
    forbidden_word(agent, words).map(|hit| hit.word)
}

#[test]
fn every_auto_flag_is_forbidden_alone_and_inline() {
    for (agent, flags) in AUTO_FLAGS {
        for rule in auto_rules(agent) {
            let (alone, inline) = match rule {
                (flag, Unsafe::When(value)) if flag.starts_with("--") => {
                    (format!("{flag} {value}"), format!("{flag}={value}"))
                }
                (flag, Unsafe::When(value)) => {
                    (format!("{flag} {value}"), format!("{flag} '{value}'"))
                }
                (flag, _) => (flag.to_string(), format!("'{flag}'")),
            };
            assert_eq!(word(agent, &alone), Some(alone.clone()), "{agent}: {alone}");
            assert!(word(agent, &inline).is_some(), "{agent}: {inline}");
        }
        assert!(word(agent, &flags.join(" ")).is_some(), "{agent}");
    }
}

#[test]
fn the_extra_flags_are_forbidden_in_every_form() {
    let cases: [(&str, &str); 24] = [
        ("claude-code", "--dangerously-skip-permissions"),
        ("claude-code", "--allow-dangerously-skip-permissions"),
        ("claude-code", "--permission-mode bypassPermissions"),
        ("claude-code", "--permission-mode=acceptEdits"),
        ("claude-code", "\"--permission-mode\" 'bypassPermissions'"),
        ("codex", "--dangerously-bypass-approvals-and-sandbox"),
        ("codex", "--yolo"),
        ("codex", "--full-auto"),
        ("codex", "--ask-for-approval never"),
        ("codex", "--sandbox=danger-full-access"),
        ("gemini", "--yolo"),
        ("gemini", "--approval-mode auto_edit"),
        ("qwen", "--approval-mode=yolo"),
        ("copilot", "--allow-all"),
        ("copilot", "--allow-all-paths"),
        ("cursor", "--force"),
        ("cursor", "--yolo"),
        ("droid", "--skip-permissions-unsafe"),
        ("kimi", "--afk"),
        ("cline", "--yolo"),
        ("kiro", "--trust-tools"),
        ("kiro", "--trust-tools=fs_read"),
        ("goose", "GOOSE_MODE=auto"),
        ("codex", "--yo\"\"lo"),
    ];
    for (agent, words) in cases {
        let line = format!("--model x {words} --verbose");
        let hit = forbidden_word(agent, &line).unwrap_or_else(|| panic!("{agent}: {words}"));
        assert!(hit.reason.contains(&hit.word), "{}", hit.reason);
        assert!(hit.reason.contains(agent), "{}", hit.reason);
    }
}

#[test]
fn a_goose_mode_word_is_forbidden_for_every_agent() {
    for agent in LAUNCHABLE_AGENTS {
        assert_eq!(
            word(agent, "'GOOSE_MODE=smart_approve'"),
            Some("GOOSE_MODE=smart_approve".into()),
            "{agent}"
        );
    }
}

#[test]
fn plain_words_and_the_ask_values_pass() {
    for agent in LAUNCHABLE_AGENTS {
        assert_eq!(word(agent, "--model x"), None, "{agent}");
        assert_eq!(word(agent, "--model 'gpt-5' --verbose"), None, "{agent}");
        assert_eq!(word(agent, ""), None, "{agent}");
    }
    assert_eq!(word("claude-code", "--permission-mode default"), None);
    assert_eq!(word("claude-code", "--permission-mode=plan"), None);
    assert_eq!(word("codex", "--ask-for-approval on-request"), None);
    assert_eq!(word("gemini", "--approval-mode default"), None);
    // A flag of one agent is no flag of another.
    assert_eq!(word("claude-code", "--force"), None);
}

#[test]
fn a_mode_flag_without_its_value_is_forbidden() {
    assert_eq!(
        word("claude-code", "--permission-mode --verbose"),
        Some("--permission-mode".into())
    );
    assert_eq!(
        word("claude-code", "--permission-mode"),
        Some("--permission-mode".into())
    );
}

#[test]
fn splits_words_as_a_posix_shell() {
    assert_eq!(
        split_words(r#" a 'b c' "d\"e" f\ g '' "#),
        vec!["a", "b c", "d\"e", "f g", ""]
    );
    assert_eq!(split_words("'it''s'"), vec!["its"]);
}

// T-0373 Part 6: codex config overrides are words too.
#[test]
fn codex_config_overrides_that_open_approvals_are_forbidden() {
    for words in [
        "-c approval_policy=never",
        "-c 'approval_policy=\"never\"'",
        "--config sandbox_mode=danger-full-access",
        "--config=sandbox_mode=danger-full-access",
        "-c sandbox_workspace_write.network_access=true",
    ] {
        assert!(word("codex", words).is_some(), "{words}");
    }
    for words in [
        "-c approval_policy=on-request",
        "-c model=o3",
        "--config sandbox_mode=read-only",
    ] {
        assert_eq!(word("codex", words), None, "{words}");
    }
    assert_eq!(word("claude-code", "-c approval_policy=never"), None);
}
