//! The rule of `set_ci` under Strict mode: with `ciPush: forbid` a CI check cannot be turned on.

use natamux_core::security::{CI_FORBID_REASON, CiMode, ci_mode};

use crate::tasks::errors::{TaskError, TaskResult};

/// Turning a CI check off always works; turning it on is refused while CI pushes are forbidden.
pub fn refuse_ci_on(on: bool) -> TaskResult<()> {
    if on && ci_mode() == CiMode::Forbid {
        return Err(TaskError::Edit(CI_FORBID_REASON.into()));
    }
    Ok(())
}
