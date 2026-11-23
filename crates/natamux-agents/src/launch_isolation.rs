//! Which agents may start while isolation is required. Only Claude Code is checked under the
//! sandbox: its files are known (`~/.claude`, `~/.claude.json`) and granted. Any other agent
//! refuses to start, never runs unconfined.

pub const ISOLATED_AGENT: &str = "claude-code";

pub const ISOLATION_AGENT_REASON: &str = "Isolation is required, and only Claude Code runs \
     isolated. Pick Claude Code, or turn isolation off in security.json and restart Natamux.";

pub fn isolated_adapter_ok(agent: &str) -> bool {
    agent == ISOLATED_AGENT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_claude_code_runs_isolated() {
        assert!(isolated_adapter_ok("claude-code"));
        for agent in natamux_proto::agents::start::LAUNCHABLE_AGENTS {
            assert_eq!(
                isolated_adapter_ok(agent),
                agent == "claude-code",
                "{agent}"
            );
        }
        assert!(!isolated_adapter_ok("other"));
        assert!(!isolated_adapter_ok(""));
    }
}
