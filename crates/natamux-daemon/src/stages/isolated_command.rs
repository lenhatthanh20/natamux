//! Stage `check` commands under isolation: the same runner, with the sandbox applied last. A
//! command that cannot be isolated fails its check and never runs.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use natamux_stages::checks::{CHECK_TIMEOUT_MS, RunCommand, run_shell_command_with};

use crate::terminals::isolation::Isolation;

pub fn isolated_run_command(isolation: Arc<Isolation>) -> RunCommand {
    Arc::new(move |command: &str, cwd: &Path| {
        let prepare = |process: &mut std::process::Command| isolation.isolate_command(process, cwd);
        run_shell_command_with(
            command,
            cwd,
            Duration::from_millis(CHECK_TIMEOUT_MS),
            &prepare,
        )
    })
}
