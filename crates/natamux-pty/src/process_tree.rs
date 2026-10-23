//! Reads `ps` output to find the processes under a shell (`process-tree.ts`), and signals them
//! (`signalTree` in `pty.ts`).

use crate::pty::Signal;

/// The processes started under `root`, children first: the deepest come last in the list, so
/// they are signalled first. Reads `ps -A -o pid=,ppid=` output.
pub fn descendants(ps_output: &str, root: i32) -> Vec<i32> {
    let mut children: Vec<(i32, i32)> = Vec::new();
    for line in ps_output.lines() {
        let mut fields = line.split_whitespace().map(str::parse::<i32>);
        if let (Some(Ok(pid)), Some(Ok(ppid))) = (fields.next(), fields.next()) {
            children.push((ppid, pid));
        }
    }
    let mut found = Vec::new();
    visit(&children, root, &mut found);
    found
}

fn visit(children: &[(i32, i32)], pid: i32, found: &mut Vec<i32>) {
    for (_, child) in children.iter().filter(|(parent, _)| *parent == pid) {
        found.push(*child);
        visit(children, *child, found);
    }
}

/// The command lines of the processes under `root`, parents first. Reads
/// `ps -A -o pid=,ppid=,args=` output.
pub fn commands_under(ps_output: &str, root: i32) -> Vec<String> {
    let args = |wanted: i32| {
        ps_output.lines().find_map(|line| {
            let line = line.trim_start();
            let (pid, rest) = line.split_once(char::is_whitespace)?;
            let (_, command) = rest.trim_start().split_once(char::is_whitespace)?;
            (pid.parse::<i32>().ok()? == wanted).then(|| command.trim_start().to_string())
        })
    };
    descendants(ps_output, root)
        .into_iter()
        .filter_map(args)
        .collect()
}

/// The ones of `pids` that still run, in the order of `ps`. A zombie has ended: it only waits for
/// its parent to read its exit. Reads `ps -A -o pid=,ppid=,stat=` output.
pub fn still_running(ps_output: &str, pids: &[i32]) -> Vec<i32> {
    ps_output
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let pid = fields.next()?.parse::<i32>().ok()?;
            let stat = fields.nth(1)?;
            (pids.contains(&pid) && !stat.starts_with('Z')).then_some(pid)
        })
        .collect()
}

/// An agent runs as a job of the shell. Ending only the shell leaves it running with no terminal,
/// so every process under the shell gets the signal too. So do the ones of `others` that still
/// run, and what runs under them: processes an earlier signal reached, which may have outlived the
/// shell. An ended one is skipped, as its pid may belong to another process by now. Gives the pids
/// it signalled. The shell itself is not one of them.
pub fn signal_tree(
    table: &str,
    root: i32,
    signal: Signal,
    others: &[i32],
    send: &mut dyn FnMut(i32, Signal),
) -> Vec<i32> {
    let alive = still_running(table, others);
    let mut pids: Vec<i32> = Vec::new();
    for pid in std::iter::once(root).chain(alive.iter().copied()) {
        for child in descendants(table, pid).into_iter().rev() {
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
        table: &str,
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

    pub fn leftover(&self, table: &str) -> bool {
        !self.hung_up.is_empty() && !still_running(table, &self.hung_up).is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_processes_under_a_shell_at_any_depth_and_no_others() {
        let ps = "  1     0\n 10     1\n 11    10\n 12    11\n 13    10\n 20     1\n";
        assert_eq!(descendants(ps, 10), [11, 12, 13]);
        assert!(descendants(ps, 12).is_empty());
    }

    #[test]
    fn gives_the_command_lines_of_the_processes_under_a_shell_parents_first() {
        let ps = [
            "  1     0 /sbin/launchd",
            " 10     1 -zsh",
            " 11    10 node /usr/local/bin/codex --model o3",
            " 12    11 /usr/local/lib/node_modules/@openai/codex/vendor/codex/codex",
            " 20     1 vim notes.md",
            "",
        ]
        .join("\n");
        assert_eq!(
            commands_under(&ps, 10),
            [
                "node /usr/local/bin/codex --model o3",
                "/usr/local/lib/node_modules/@openai/codex/vendor/codex/codex",
            ]
        );
        assert!(commands_under(&ps, 12).is_empty());
    }

    #[test]
    fn keeps_the_processes_that_still_run_a_zombie_or_a_missing_one_has_ended() {
        let ps = "  1     0 Ss\n 11     1 S+\n 13     1 Z\n";
        assert_eq!(still_running(ps, &[11, 12, 13]), [11]);
        assert!(still_running(ps, &[13]).is_empty());
        assert!(still_running(ps, &[]).is_empty());
    }

    fn recorder(signalled: &mut Vec<(i32, Signal)>) -> impl FnMut(i32, Signal) + '_ {
        move |pid, signal| signalled.push((pid, signal))
    }

    // `pty.test.ts`: the kill of a shell, with `ps` and `process.kill` replaced.
    #[test]
    fn kills_a_program_that_ignored_sighup_also_once_the_shell_has_ended() {
        let mut tree = TreeKill::default();
        let mut signalled = Vec::new();
        assert!(!tree.leftover(""));

        // 11 is an agent that will ignore SIGHUP, 12 its child, 13 a job that ends on SIGHUP.
        let table = "10 1 Ss\n11 10 S+\n12 11 S+\n13 10 S\n";
        tree.kill(table, 10, Signal::Hup, &mut recorder(&mut signalled));
        assert_eq!(
            signalled,
            [(13, Signal::Hup), (12, Signal::Hup), (11, Signal::Hup)]
        );

        // The shell ended. The agent lives on under pid 1; the job is a zombie.
        let table = "11 1 S\n12 11 S\n13 1 Z\n";
        assert!(tree.leftover(table));

        signalled.clear();
        tree.kill(table, 10, Signal::Kill, &mut recorder(&mut signalled));
        // Not to the ended job: its pid may belong to another process by now.
        assert_eq!(signalled, [(12, Signal::Kill), (11, Signal::Kill)]);
        assert!(!tree.leftover(""));
    }

    #[test]
    fn sends_no_sigkill_to_a_process_that_is_gone_from_ps() {
        let mut tree = TreeKill::default();
        let mut signalled = Vec::new();
        tree.kill(
            "10 1 Ss\n11 10 S+\n13 10 S\n",
            10,
            Signal::Hup,
            &mut |_, _| {},
        );
        tree.kill("11 1 S\n", 10, Signal::Kill, &mut recorder(&mut signalled));
        assert_eq!(signalled, [(11, Signal::Kill)]);
    }

    #[test]
    fn has_no_leftover_when_every_process_ended_on_sighup() {
        let mut tree = TreeKill::default();
        tree.kill("10 1 Ss\n11 10 S+\n", 10, Signal::Hup, &mut |_, _| {});
        assert!(!tree.leftover("11 1 Z\n"));
    }
}
