//! The program a shell runs (`shell-command.ts`).

use crate::pty::Env;

/// A login shell, like a Terminal window: many users set up the PATH (Homebrew, mise, nvm) in
/// ~/.zprofile or ~/.bash_profile, which only a login shell reads. PowerShell on Windows.
pub fn shell_command(
    env: &Env,
    windows: bool,
    exists: &dyn Fn(&str) -> bool,
) -> (String, Vec<String>) {
    if windows {
        return ("powershell.exe".into(), Vec::new());
    }
    let login = vec!["-l".to_string()];
    match env.get("SHELL") {
        Some(shell) if !shell.is_empty() && exists(shell) => (shell.clone(), login),
        _ if exists("/bin/bash") => ("/bin/bash".into(), login),
        _ => ("/bin/sh".into(), login),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn only(files: &[&str]) -> impl Fn(&str) -> bool {
        let files: Vec<String> = files.iter().map(|file| file.to_string()).collect();
        move |file| files.iter().any(|known| known == file)
    }

    fn env(shell: Option<&str>) -> Env {
        shell
            .map(|shell| ("SHELL".to_string(), shell.to_string()))
            .into_iter()
            .collect()
    }

    fn login(file: &str) -> (String, Vec<String>) {
        (file.to_string(), vec!["-l".to_string()])
    }

    #[test]
    fn starts_a_login_shell_on_darwin_and_linux() {
        assert_eq!(
            shell_command(&env(Some("/bin/zsh")), false, &only(&["/bin/zsh"])),
            login("/bin/zsh")
        );
    }

    #[test]
    fn passes_the_shell_as_one_file_with_the_login_flag() {
        for shell in ["/bin/bash", "/usr/local/bin/fish", "/Users/me/My Tools/zsh"] {
            assert_eq!(
                shell_command(&env(Some(shell)), false, &only(&[shell])),
                login(shell)
            );
        }
    }

    #[test]
    fn falls_back_to_bash_when_shell_is_blank() {
        for shell in ["", "   "] {
            let found = shell_command(&env(Some(shell)), false, &only(&["/bin/bash"]));
            assert_eq!(found, login("/bin/bash"));
        }
    }

    #[test]
    fn falls_back_to_bash_then_sh_when_shell_is_not_there() {
        assert_eq!(
            shell_command(&env(None), false, &only(&["/bin/bash"])),
            login("/bin/bash")
        );
        let gone = env(Some("/gone/zsh"));
        assert_eq!(
            shell_command(&gone, false, &only(&["/bin/bash"])),
            login("/bin/bash")
        );
        assert_eq!(shell_command(&gone, false, &only(&[])), login("/bin/sh"));
    }

    #[test]
    fn starts_powershell_with_no_arguments_on_windows() {
        let found = shell_command(&env(Some("/bin/zsh")), true, &only(&["/bin/zsh"]));
        assert_eq!(found, ("powershell.exe".to_string(), Vec::new()));
    }

    // The fallback shells must accept the flag. A fresh HOME keeps the machine's dotfiles out.
    #[cfg(unix)]
    #[test]
    fn the_fallback_shells_accept_the_login_flag() -> std::io::Result<()> {
        let home = std::env::temp_dir().join(format!("natamux-shell-{}", std::process::id()));
        std::fs::create_dir_all(&home)?;
        for file in ["/bin/sh", "/bin/bash"]
            .into_iter()
            .filter(|file| std::path::Path::new(file).exists())
        {
            let out = std::process::Command::new(file)
                .args(["-l", "-c", "echo ok"])
                .env_clear()
                .env("HOME", &home)
                .env("PATH", "/usr/bin:/bin")
                .output()?;
            assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "ok", "{file}");
        }
        std::fs::remove_dir_all(&home)
    }
}
