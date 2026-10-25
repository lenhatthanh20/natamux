//! Signals the processes under a shell (`signalTree` in `pty.ts`), from the process table.

use crate::process_table::ProcessTable;
use crate::pty::Signal;

/// An agent runs as a job of the shell. Ending only the shell leaves it running with no terminal,
/// so every process under the shell gets the signal too. So do the ones of `others` that still
/// run, and what runs under them: processes an earlier signal reached, which may have outlived the
/// shell. An ended one is skipped, as its pid may belong to another process by now. Gives the pids
/// it signalled. The shell itself is not one of them.
pub fn signal_tree(
    table: &ProcessTable,
    root: i32,
    signal: Signal,
    others: &[i32],
    send: &mut dyn FnMut(i32, Signal),
) -> Vec<i32> {
    let alive = table.still_running(others);
    let mut pids: Vec<i32> = Vec::new();
    for pid in std::iter::once(root).chain(alive.iter().copied()) {
        for child in table.descendants(pid).into_iter().rev() {
            if !pids.contains(&child) {
                pids.push(child);
            }
        }
    }
    for pid in alive {
        if !pids.contains(&pid) {
            pids.push(pid);
        }
    }
    for pid in &pids {
        send(*pid, signal);
    }
    pids
}

/// Remembers the processes the SIGHUP of a close reached. One that ignored it may outlive the
/// shell, so the SIGKILL goes to it too. shortcut: a pid could be reused before the SIGKILL; pids
/// count up, so within the kill grace this is very unlikely.
#[derive(Default)]
pub struct TreeKill {
    hung_up: Vec<i32>,
}

impl TreeKill {
    /// Signals the processes under `root` (and, for SIGKILL, those a SIGHUP reached before). The
    /// caller signals `root` itself.
    pub fn kill(
        &mut self,
        table: &ProcessTable,
        root: i32,
        signal: Signal,
        send: &mut dyn FnMut(i32, Signal),
    ) {
        if signal == Signal::Kill {
            signal_tree(table, root, signal, &self.hung_up, send);
            self.hung_up.clear();
        } else {
            self.hung_up = signal_tree(table, root, signal, &[], send);
        }
    }

    pub fn leftover(&self, table: &ProcessTable) -> bool {
        !self.hung_up.is_empty() && !table.still_running(&self.hung_up).is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recorder(signalled: &mut Vec<(i32, Signal)>) -> impl FnMut(i32, Signal) + '_ {
        move |pid, signal| signalled.push((pid, signal))
    }

    fn ps(text: &str) -> ProcessTable {
        ProcessTable::parse(text)
    }

    // `pty.test.ts`: the kill of a shell, with `ps` and `process.kill` replaced.
    #[test]
    fn kills_a_program_that_ignored_sighup_also_once_the_shell_has_ended() {
        let mut tree = TreeKill::default();
        let mut signalled = Vec::new();
        assert!(!tree.leftover(&ps("")));

        // 11 is an agent that will ignore SIGHUP, 12 its child, 13 a job that ends on SIGHUP.
        let table = ps("10 1 Ss\n11 10 S+\n12 11 S+\n13 10 S\n");
        tree.kill(&table, 10, Signal::Hup, &mut recorder(&mut signalled));
        assert_eq!(
            signalled,
            [(13, Signal::Hup), (12, Signal::Hup), (11, Signal::Hup)]
        );

        // The shell ended. The agent lives on under pid 1; the job is a zombie.
        let table = ps("11 1 S\n12 11 S\n13 1 Z\n");
        assert!(tree.leftover(&table));

        signalled.clear();
        tree.kill(&table, 10, Signal::Kill, &mut recorder(&mut signalled));
        // Not to the ended job: its pid may belong to another process by now.
        assert_eq!(signalled, [(12, Signal::Kill), (11, Signal::Kill)]);
        assert!(!tree.leftover(&ps("")));
    }

    #[test]
    fn sends_no_sigkill_to_a_process_that_is_gone_from_ps() {
        let mut tree = TreeKill::default();
        let mut signalled = Vec::new();
        let table = ps("10 1 Ss\n11 10 S+\n13 10 S\n");
        tree.kill(&table, 10, Signal::Hup, &mut |_, _| {});
        let table = ps("11 1 S\n");
        tree.kill(&table, 10, Signal::Kill, &mut recorder(&mut signalled));
        assert_eq!(signalled, [(11, Signal::Kill)]);
    }

    #[test]
    fn has_no_leftover_when_every_process_ended_on_sighup() {
        let mut tree = TreeKill::default();
        tree.kill(&ps("10 1 Ss\n11 10 S+\n"), 10, Signal::Hup, &mut |_, _| {});
        assert!(!tree.leftover(&ps("11 1 Z\n")));
    }
}
