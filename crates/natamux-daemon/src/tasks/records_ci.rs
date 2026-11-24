//! The rule of `set_ci` under Strict mode: with `ciPush: forbid` a CI check cannot be turned on.

use std::path::Path;

use natamux_core::security::{CI_FORBID_REASON, CiMode, ci_mode};

use crate::tasks::errors::{TaskError, TaskResult};

/// Turning a CI check off always works; turning it on is refused while CI pushes are forbidden,
/// with an audit line (one per blocked action).
pub fn refuse_ci_on(data_dir: &Path, task_id: &str, on: bool) -> TaskResult<()> {
    if on && ci_mode() == CiMode::Forbid {
        crate::privacy::audit::write(data_dir, "ci.blocked", task_id, "blocked");
        return Err(TaskError::Edit(CI_FORBID_REASON.into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use natamux_core::security::{CiPush, SecurityMode, StrictSettings, force};

    use super::*;
    use crate::privacy::audit::audit_path;

    #[test]
    fn a_refused_turn_on_writes_one_audit_line() {
        let dir = std::env::temp_dir().join(format!("natamux-ci-on-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let _forbid = force(SecurityMode::Strict(StrictSettings {
            ci_push: CiPush::Forbid,
            ..StrictSettings::default()
        }));

        assert!(refuse_ci_on(&dir, "T-1", true).is_err());
        assert!(refuse_ci_on(&dir, "T-1", false).is_ok());

        let text = std::fs::read_to_string(audit_path(&dir)).unwrap();
        assert_eq!(text.lines().count(), 1, "{text}");
        assert!(
            text.contains("\"ci.blocked\"") && text.contains("\"T-1\""),
            "{text}"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
