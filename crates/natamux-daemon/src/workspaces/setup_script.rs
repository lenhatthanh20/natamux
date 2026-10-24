//! `setup-script.ts`: a repository can hold a script that prepares a new workspace (install
//! packages, copy env files). It runs in the first terminal of the task, so its output can be read
//! there.

use std::path::Path;

pub const SETUP_SCRIPT: &str = ".natamux-setup";
pub const SETUP_SCRIPT_WINDOWS: &str = ".natamux-setup.ps1";

/// The command line that runs the setup script of a worktree, or None when it has none.
pub fn setup_command(worktree: &Path, windows: bool) -> Option<String> {
    if windows {
        return worktree
            .join(SETUP_SCRIPT_WINDOWS)
            .exists()
            .then(|| format!("& ./{SETUP_SCRIPT_WINDOWS}"));
    }
    worktree
        .join(SETUP_SCRIPT)
        .exists()
        .then(|| format!("sh ./{SETUP_SCRIPT}"))
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
}
