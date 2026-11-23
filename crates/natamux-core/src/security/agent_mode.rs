//! `agentAutoMode` and `terminalInputTrace`. Strict sets them to `forbid` and `off`, and a key
//! can only be stricter than the Strict default, so in Strict mode both are always closed.

use super::mode;

/// Why a save of `permissionMode: auto` is refused.
pub const AUTO_FORBID_REASON: &str =
    "Strict mode forbids the auto permission mode (agentAutoMode: forbid).";

/// The one line logged when the env asks for an input trace that Strict mode blocks.
pub const INPUT_TRACE_BLOCKED: &str = "input trace blocked by Strict mode";

/// Whether a project may run its agents in the `auto` permission mode.
pub fn auto_allowed() -> bool {
    !mode().is_strict()
}

/// Whether terminal input may be traced. Asked at start and again on each input, so tracing
/// stops on the next input once Strict mode is on.
pub fn input_trace_allowed() -> bool {
    !mode().is_strict()
}

#[cfg(test)]
mod tests {
    use super::super::{SecurityMode, StrictSettings, force};
    use super::*;

    #[test]
    fn strict_closes_auto_and_the_input_trace() {
        assert!(auto_allowed());
        assert!(input_trace_allowed());
        let _strict = force(SecurityMode::Strict(StrictSettings::default()));
        assert!(!auto_allowed());
        assert!(!input_trace_allowed());
    }
}
