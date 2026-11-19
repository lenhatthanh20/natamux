//! `setup-script.ts`: a repository can hold a script that prepares a new workspace (install
//! packages, copy env files). It runs in the first terminal of the task, so its output can be read
//! there.

use std::path::Path;

pub const SETUP_SCRIPT: &str = ".natamux-setup";
pub const SETUP_SCRIPT_WINDOWS: &str = ".natamux-setup.ps1";
/// The task event of a setup script that did not run: the project's commands are not approved.
pub const WORKSPACE_WARNING_EVENT: &str = "workspace.warning";
pub const SETUP_SKIPPED: &str = "Setup script skipped: repo commands are not approved";

/// The command line that runs the setup script of a worktree, or None when it has none.
pub fn setup_command(worktree: &Path, windows: bool) -> Option<String> {
    if windows {
        return regular_file(&worktree.join(SETUP_SCRIPT_WINDOWS))
            .then(|| format!("& ./{SETUP_SCRIPT_WINDOWS}"));
    }
    regular_file(&worktree.join(SETUP_SCRIPT)).then(|| format!("sh ./{SETUP_SCRIPT}"))
}

/// A link could point somewhere else between the trust check and the run, so only a regular
/// file runs.
fn regular_file(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|meta| meta.is_file())
}

#[cfg(test)]
mod tests {
    use natamux_git::test_support::TempDir;

    use super::*;

    #[test]
    fn runs_the_script_with_sh_when_the_worktree_has_one() {
        let folder = TempDir::new("setup");
        assert_eq!(setup_command(folder.path(), false), None);

        std::fs::write(folder.path().join(SETUP_SCRIPT), "pnpm install\n").unwrap();

        assert_eq!(
            setup_command(folder.path(), false).as_deref(),
            Some("sh ./.natamux-setup")
        );
    }

    #[test]
    fn runs_the_power_shell_script_on_windows() {
        let folder = TempDir::new("setup");
        std::fs::write(folder.path().join(SETUP_SCRIPT), "pnpm install\n").unwrap();
        assert_eq!(setup_command(folder.path(), true), None);

        std::fs::write(folder.path().join(SETUP_SCRIPT_WINDOWS), "pnpm install\n").unwrap();
        assert_eq!(
            setup_command(folder.path(), true).as_deref(),
            Some("& ./.natamux-setup.ps1")
        );
    }

    // A regression test: a linked script was run through the link.
    #[cfg(unix)]
    #[test]
    fn never_runs_a_script_that_is_a_link() {
        let folder = TempDir::new("setup");
        let target = folder.path().join("real-setup");
        std::fs::write(&target, "pnpm install\n").unwrap();
        std::os::unix::fs::symlink(&target, folder.path().join(SETUP_SCRIPT)).unwrap();
        std::os::unix::fs::symlink(&target, folder.path().join(SETUP_SCRIPT_WINDOWS)).unwrap();

        assert_eq!(setup_command(folder.path(), false), None);
        assert_eq!(setup_command(folder.path(), true), None);
    }
}
