//! What an isolated shell or command may reach. The daemon builds the plan; this crate applies it
//! with Landlock on Linux. Every other OS refuses a plan: isolation never silently does nothing.

use std::io;
use std::path::PathBuf;
use std::process::Command;

pub const UNSUPPORTED: &str = "Isolation needs Linux with Landlock (kernel 5.13 or later)";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IsolationPlan {
    /// Read and run, never write.
    pub read_only: Vec<PathBuf>,
    /// Read, write, make and remove.
    pub read_write: Vec<PathBuf>,
    /// An environment entry (`NAME=value`) that only this shell and what it starts carry. After
    /// a kill, every process that still has it is killed too: a program that left the shell's
    /// process tree (`setsid`) must not outlive it. Per shell, not the task token, so closing one
    /// terminal never kills the others of the task.
    pub sweep_mark: Option<String>,
    /// Use at most this Landlock ABI. None: what the kernel offers. 0 always fails.
    pub max_abi: Option<u32>,
}

/// The Landlock ABI the kernel offers, or why isolation cannot run here.
pub fn landlock_abi() -> Result<u32, String> {
    #[cfg(target_os = "linux")]
    {
        crate::sandbox_landlock::abi().map_err(|error| format!("{UNSUPPORTED}: {error}"))
    }
    #[cfg(not(target_os = "linux"))]
    {
        Err(UNSUPPORTED.to_string())
    }
}

/// A plan made ready before the fork: the rules and the folders they name are opened here, so the
/// child only has to turn them on.
pub struct Prepared {
    #[cfg(target_os = "linux")]
    ruleset: crate::sandbox_landlock::Ruleset,
}

/// Fails, before any process starts, when the plan cannot be applied in full.
pub fn prepare(plan: &IsolationPlan) -> io::Result<Prepared> {
    #[cfg(target_os = "linux")]
    {
        Ok(Prepared {
            ruleset: crate::sandbox_landlock::build(plan)?,
        })
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = plan;
        Err(io::Error::new(io::ErrorKind::Unsupported, UNSUPPORTED))
    }
}

impl Prepared {
    /// Between fork and exec: only async-signal-safe calls.
    pub fn restrict(&self) -> io::Result<()> {
        #[cfg(target_os = "linux")]
        {
            crate::sandbox_landlock::restrict(self.ruleset.raw())
        }
        #[cfg(not(target_os = "linux"))]
        {
            Ok(())
        }
    }
}

/// Runs `command` under the plan. An error means the command must not run.
pub fn isolate_command(command: &mut Command, plan: &IsolationPlan) -> io::Result<()> {
    let prepared = prepare(plan)?;
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: `restrict` makes only prctl and landlock_restrict_self calls.
        unsafe {
            command.pre_exec(move || prepared.restrict());
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (command, prepared);
    }
    Ok(())
}

#[cfg(all(test, not(target_os = "linux")))]
mod tests {
    use super::*;

    #[test]
    fn every_other_os_refuses_a_plan() {
        let refused = prepare(&IsolationPlan::default())
            .err()
            .map(|error| error.kind());
        assert_eq!(refused, Some(io::ErrorKind::Unsupported));
        let mut command = Command::new("true");
        assert!(isolate_command(&mut command, &IsolationPlan::default()).is_err());
        assert!(landlock_abi().is_err());
    }
}
