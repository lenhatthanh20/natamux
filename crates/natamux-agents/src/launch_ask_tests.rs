#![allow(clippy::unwrap_used)]

use natamux_core::security::{SecurityMode, StrictSettings, force};

use super::*;
use crate::launch::LAUNCHABLE_AGENTS;

#[test]
fn every_adapter_and_other_has_a_row() {
    for kind in LAUNCHABLE_AGENTS.iter().chain(&["other"]) {
        assert!(ask_row(kind).is_some(), "{kind} has no ask row");
    }
    assert_eq!(ask_row("nobody"), None);
}

#[test]
fn only_the_agents_with_an_ask_mode_are_guaranteed() {
    let guaranteed: Vec<&str> = LAUNCHABLE_AGENTS
        .into_iter()
        .filter(|kind| ask_guaranteed(kind))
        .collect();
    assert_eq!(
        guaranteed,
        ["claude-code", "codex", "gemini", "qwen", "goose"]
    );
    assert!(!ask_guaranteed("other"));
}

#[test]
fn gives_each_agent_its_ask_words() {
    assert_eq!(ask_flags("claude-code"), ["--permission-mode", "default"]);
    assert_eq!(
        ask_flags("codex"),
        [
            "--ask-for-approval",
            "on-request",
            "--sandbox",
            "workspace-write"
        ]
    );
    assert_eq!(ask_flags("gemini"), ["--approval-mode", "default"]);
    assert_eq!(ask_flags("qwen"), ["--approval-mode", "default"]);
    assert!(ask_flags("goose").is_empty());
    assert_eq!(
        with_ask_env("goose", "goose session", Platform::Posix),
        "GOOSE_MODE=approve goose session"
    );
    assert_eq!(
        with_ask_env("goose", "goose session", Platform::Windows),
        "$env:GOOSE_MODE='approve'; goose session"
    );
    assert_eq!(with_ask_env("codex", "codex", Platform::Posix), "codex");
}

#[test]
fn strict_with_the_guarantee_blocks_the_others_and_one_shot_follows() {
    assert!(allowed_now("cursor"));
    assert!(crate::launch::has_one_shot("cursor"));
    let _strict = force(SecurityMode::Strict(StrictSettings::default()));
    assert!(!allowed_now("cursor"));
    assert!(!allowed_now("other"));
    assert!(allowed_now("claude-code"));
    assert!(!crate::launch::has_one_shot("cursor"));
    assert!(crate::launch::has_one_shot("claude-code"));
}
