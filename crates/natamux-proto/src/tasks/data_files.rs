//! Names of a task's files in the data folder. The daemon's task delete and the CLI's
//! `purge-data` both match them, so a file of another kind is never removed by mistake.

/// The scripts the daemon writes into the data folder for a task.
pub const SCRIPT_PREFIXES: [&str; 2] = ["agent-launch-", "stage-command-"];

/// The leading `T-0001` of a name, so `T-1` is never read as part of `T-10`.
pub fn id_prefix(name: &str) -> Option<(&str, &str)> {
    let letters = name.bytes().take_while(u8::is_ascii_uppercase).count();
    let after = name.get(letters..)?.strip_prefix('-')?;
    let digits = after.bytes().take_while(u8::is_ascii_digit).count();
    if letters == 0 || digits == 0 {
        return None;
    }
    let end = letters + 1 + digits;
    Some((name.get(..end)?, name.get(end..)?))
}

/// `agent-launch-T-0001.sh`, `agent-launch-T-0001-2.sh`, `stage-command-T-0001.sh`.
pub fn task_of_script(name: &str) -> Option<&str> {
    let rest = SCRIPT_PREFIXES
        .iter()
        .find_map(|prefix| name.strip_prefix(prefix))?;
    let (id, after) = id_prefix(rest)?;
    let tail = after.strip_prefix('-').map_or(after, |terminal| {
        terminal.trim_start_matches(|char: char| char.is_ascii_alphanumeric())
    });
    (tail == ".sh").then_some(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_script_names_its_task_and_no_other_file_does() {
        for (name, task) in [
            ("agent-launch-T-1.sh", Some("T-1")),
            ("agent-launch-T-1-2.sh", Some("T-1")),
            ("stage-command-T-1.sh", Some("T-1")),
            ("agent-launch-T-10.sh", Some("T-10")),
            ("agent-launch-T-1.txt", None),
            ("agent-launch-T-1-2.sh.bak", None),
            ("agent-launch-notes.txt", None),
            ("repo-trust.json", None),
            ("T-1.md", None),
        ] {
            assert_eq!(task_of_script(name), task, "{name}");
        }
    }
}
