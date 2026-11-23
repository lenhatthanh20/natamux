//! After an isolated shell is killed: any process that still has the shell's mark (an exact
//! environment entry) left the shell's process tree (`setsid`, a double fork) and is killed too.
//! Best effort: a process can clear its own environment. Linux only, where `/proc` tells.

/// Whether a `/proc/<pid>/environ` text holds `mark` (`NAME=value`) as a whole entry.
pub fn holds_mark(environ: &[u8], mark: &str) -> bool {
    !mark.is_empty()
        && environ
            .split(|byte| *byte == 0)
            .any(|each| each == mark.as_bytes())
}

/// The processes, other than this one, that hold the mark.
pub fn holders(mark: &str) -> Vec<i32> {
    #[cfg(target_os = "linux")]
    {
        let own = std::process::id() as i32;
        let Ok(entries) = std::fs::read_dir("/proc") else {
            return Vec::new();
        };
        entries
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().to_str()?.parse::<i32>().ok())
            .filter(|pid| *pid != own)
            .filter(|pid| {
                std::fs::read(format!("/proc/{pid}/environ"))
                    .is_ok_and(|environ| holds_mark(&environ, mark))
            })
            .collect()
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = mark;
        Vec::new()
    }
}

/// Kills every holder of the mark. Gives how many it found.
pub fn sweep(mark: &str) -> usize {
    let found = holders(mark);
    #[cfg(unix)]
    for pid in &found {
        // It may have ended meanwhile.
        let _ = nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(*pid),
            nix::sys::signal::Signal::SIGKILL,
        );
    }
    found.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_only_the_exact_entry() {
        let environ = b"PATH=/bin\0NATAMUX_SANDBOX=abc\0HOME=/h\0";
        assert!(holds_mark(environ, "NATAMUX_SANDBOX=abc"));
        assert!(!holds_mark(environ, "NATAMUX_SANDBOX=ab"));
        assert!(!holds_mark(environ, "NATAMUX_SANDBOX=abcd"));
        assert!(!holds_mark(
            b"X_NATAMUX_SANDBOX=abc\0",
            "NATAMUX_SANDBOX=abc"
        ));
        assert!(!holds_mark(environ, ""));
    }
}
