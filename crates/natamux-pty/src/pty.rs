use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;

use crate::process_table::{ProcessTable, TableReader};

/// The environment of a shell, whole: nothing of the daemon's own environment is added to it.
pub type Env = BTreeMap<String, String>;

pub struct SpawnOptions {
    pub cols: u16,
    pub rows: u16,
    pub cwd: PathBuf,
    pub env: Env,
}

/// The signals the daemon sends to a shell. Windows has none: any kill ends the shell and what
/// runs in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signal {
    Hup,
    Kill,
}

/// What a shell tells its owner. `on_data` gets the bytes as the pty read them: a read can stop
/// inside a UTF-8 character. `on_exit` comes once, after the last `on_data`, with the exit code
/// (0 for a shell ended by a signal, as node-pty).
pub struct PtyEvents {
    pub on_data: Box<dyn FnMut(Vec<u8>) + Send>,
    pub on_exit: Box<dyn FnOnce(Option<i32>) + Send>,
}

/// The part of a pseudo terminal the daemon uses (`PtyProcess` in `terminal-types.ts`). Tests give
/// a fake.
pub trait Pty: Send {
    /// Starts reading. Output written before waits in the pty until then.
    fn start(&mut self, events: PtyEvents);
    /// Never blocks: a program that does not read its input must not stop the daemon.
    fn write(&mut self, data: &[u8]);
    fn resize(&mut self, cols: u16, rows: u16);
    /// Stop and go on reading output. Input still goes in while paused.
    fn pause(&mut self);
    fn resume(&mut self);
    fn kill(&mut self, signal: Signal);
    /// True when a process that the last SIGHUP reached still runs, though the shell may have
    /// ended.
    fn leftover(&self) -> bool {
        false
    }
    /// The name of the program in the foreground, such as "bash" or "node".
    fn foreground(&self) -> Option<String> {
        None
    }
    /// The command lines of the processes under the shell, parents first, when the platform tells.
    fn commands(&self) -> Option<Vec<String>> {
        None
    }
    /// How this pty reads the process table for `kill_in`, `leftover_in` and `commands_in`. None
    /// when it needs no table (a fake): then the plain methods run.
    fn processes(&self) -> Option<TableReader> {
        None
    }
    /// `kill` with a table read before, by a caller that must not run `ps` itself.
    fn kill_in(&mut self, signal: Signal, _table: &ProcessTable) {
        self.kill(signal);
    }
    fn leftover_in(&self, _table: &ProcessTable) -> bool {
        self.leftover()
    }
    fn commands_in(&self, _table: &ProcessTable) -> Option<Vec<String>> {
        self.commands()
    }
}

/// The real pseudo terminal, running the shell of `shell_command`.
pub fn spawn_shell(options: SpawnOptions) -> io::Result<Box<dyn Pty>> {
    #[cfg(unix)]
    {
        crate::unix::spawn(options)
    }
    #[cfg(windows)]
    {
        crate::windows::spawn(options)
    }
}
