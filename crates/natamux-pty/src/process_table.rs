//! The process table of `ps -ww -A -o pid=,ppid=,stat=,args=`, parsed once. The kill of a shell,
//! its leftovers and the agent lookups of every shell read the same table, not each their own
//! copy of the text.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Reads the process table: `true` reads it now, `false` takes one a moment old that every shell
/// shares. `ps` may take a moment: a caller that holds a lock reads it before taking the lock,
/// and a timer reads it off the timer thread.
pub type TableReader = Arc<dyn Fn(bool) -> Arc<ProcessTable> + Send + Sync>;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ProcessTable {
    processes: Vec<Process>,
    /// The children of each pid, in the order of `ps`.
    children: HashMap<i32, Vec<i32>>,
    /// Where each pid is in `processes`.
    index: HashMap<i32, usize>,
}

#[derive(Debug, PartialEq, Eq)]
struct Process {
    pid: i32,
    ppid: i32,
    /// A zombie has ended: it only waits for its parent to read its exit.
    ended: bool,
    args: String,
}

impl ProcessTable {
    pub fn parse(text: &str) -> ProcessTable {
        let processes: Vec<Process> = text.lines().filter_map(parse_line).collect();
        let mut children: HashMap<i32, Vec<i32>> = HashMap::new();
        let mut index = HashMap::new();
        for (at, process) in processes.iter().enumerate() {
            children.entry(process.ppid).or_default().push(process.pid);
            index.insert(process.pid, at);
        }
        ProcessTable {
            processes,
            children,
            index,
        }
    }

    /// The processes started under `root`, each after its parent: the deepest come last in the
    /// list, so a caller that walks it backwards signals them first. The table is read while
    /// processes start and end, so its parent links can form a loop: each pid comes once.
    pub fn descendants(&self, root: i32) -> Vec<i32> {
        let mut found = Vec::new();
        let mut seen = HashSet::from([root]);
        let mut next: Vec<i32> = self.children_of(root).rev().collect();
        while let Some(pid) = next.pop() {
            if !seen.insert(pid) {
                continue;
            }
            found.push(pid);
            next.extend(self.children_of(pid).rev());
        }
        found
    }

    fn children_of(&self, pid: i32) -> impl DoubleEndedIterator<Item = i32> + '_ {
        self.children.get(&pid).into_iter().flatten().copied()
    }

    /// The command lines of the processes under `root`, parents first.
    pub fn commands_under(&self, root: i32) -> Vec<String> {
        self.descendants(root)
            .into_iter()
            .filter_map(|pid| {
                let process = self.processes.get(*self.index.get(&pid)?)?;
                (!process.args.is_empty()).then(|| process.args.clone())
            })
            .collect()
    }

    /// The ones of `pids` that still run, in the order of `ps`.
    pub fn still_running(&self, pids: &[i32]) -> Vec<i32> {
        self.processes
            .iter()
            .filter(|process| pids.contains(&process.pid) && !process.ended)
            .map(|process| process.pid)
            .collect()
    }
}

fn parse_line(line: &str) -> Option<Process> {
    let mut rest = line;
    let pid = word(&mut rest).parse().ok()?;
    let ppid = word(&mut rest).parse().ok()?;
    let stat = word(&mut rest);
    Some(Process {
        pid,
        ppid,
        ended: stat.starts_with('Z'),
        args: rest.trim().to_string(),
    })
}

fn word<'a>(rest: &mut &'a str) -> &'a str {
    let text = rest.trim_start();
    let (word, after) = text.split_once(char::is_whitespace).unwrap_or((text, ""));
    *rest = after;
    word
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_processes_under_a_shell_at_any_depth_and_no_others() {
        let ps = "  1     0 Ss\n 10     1 Ss\n 11    10 S\n 12    11 S\n 13    10 S\n 20     1 S\n";
        let table = ProcessTable::parse(ps);
        assert_eq!(table.descendants(10), [11, 12, 13]);
        assert!(table.descendants(12).is_empty());
    }

    #[test]
    fn a_loop_in_the_parent_links_ends_and_gives_each_process_once() {
        let ps = "  5     6 S\n  6     5 S\n  7     6 S\n  8     8 S\n  9     8 S\n";
        let table = ProcessTable::parse(ps);
        assert_eq!(table.descendants(5), [6, 7]);
        assert_eq!(table.descendants(6), [5, 7]);
        // A process that is its own parent.
        assert_eq!(table.descendants(8), [9]);
        assert!(table.descendants(1).is_empty());
    }

    #[test]
    fn a_pid_with_no_parent_in_the_table_is_under_no_shell() {
        let table = ProcessTable::parse(" 10     1 Ss\n 11    10 S\n 30    99 S\n");
        assert_eq!(table.descendants(10), [11]);
        assert_eq!(table.descendants(99), [30]);
        assert!(table.descendants(30).is_empty());
    }

    #[test]
    fn gives_the_command_lines_of_the_processes_under_a_shell_parents_first() {
        let ps = [
            "  1     0 Ss   /sbin/launchd",
            " 10     1 Ss   -zsh",
            " 11    10 S+   node /usr/local/bin/codex --model o3",
            " 12    11 S+   /usr/local/lib/node_modules/@openai/codex/vendor/codex/codex",
            " 20     1 S    vim notes.md",
            "",
        ]
        .join("\n");
        let table = ProcessTable::parse(&ps);
        assert_eq!(
            table.commands_under(10),
            [
                "node /usr/local/bin/codex --model o3",
                "/usr/local/lib/node_modules/@openai/codex/vendor/codex/codex",
            ]
        );
        assert!(table.commands_under(12).is_empty());
    }

    #[test]
    fn keeps_the_processes_that_still_run_a_zombie_or_a_missing_one_has_ended() {
        let table = ProcessTable::parse("  1     0 Ss\n 11     1 S+\n 13     1 Z\n");
        assert_eq!(table.still_running(&[11, 12, 13]), [11]);
        assert!(table.still_running(&[13]).is_empty());
        assert!(table.still_running(&[]).is_empty());
    }

    #[test]
    fn skips_lines_it_cannot_read() {
        let table = ProcessTable::parse("garbage\n\n 5 1 S sleep 9\n");
        assert_eq!(table.commands_under(1), ["sleep 9"]);
        assert_eq!(ProcessTable::parse(""), ProcessTable::default());
    }
}
