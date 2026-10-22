//! Port of `packages/web/src/features/terminals/terminal-tabs.ts`.

// A name that is no number sorts last, by its text.
fn number_order(name: &str) -> (u64, &str) {
    (name.parse().unwrap_or(u64::MAX), name)
}

/// The terminals to show as tabs: the first one always, the open ones, and the ones just added.
pub fn terminal_names(open: &[String], added: &[String]) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for name in std::iter::once(&"1".to_string()).chain(open).chain(added) {
        if !names.contains(name) {
            names.push(name.clone());
        }
    }
    names.sort_by(|a, b| number_order(a).cmp(&number_order(b)));
    names
}

pub fn next_terminal_name(names: &[String]) -> String {
    let highest = names
        .iter()
        .filter_map(|name| name.parse::<u64>().ok())
        .max()
        .unwrap_or(0);
    (highest + 1).to_string()
}

/// The agent shells that the list of open terminals does not have: the daemon opened them after
/// the list was read, as it does for each stage of a group that starts.
pub fn unlisted_shells(open: &[String], agent_shells: &[String]) -> Vec<String> {
    agent_shells
        .iter()
        .filter(|name| !open.contains(name))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn always_has_the_first_terminal_in_number_order_without_repeats() {
        assert_eq!(terminal_names(&[], &[]), ["1"]);
        assert_eq!(
            terminal_names(&names(&["10", "2"]), &names(&["2", "3"])),
            ["1", "2", "3", "10"]
        );
    }

    #[test]
    fn takes_the_number_after_the_highest() {
        assert_eq!(next_terminal_name(&names(&["1"])), "2");
        assert_eq!(next_terminal_name(&names(&["1", "10", "3"])), "11");
    }

    #[test]
    fn does_not_take_a_name_that_a_stage_shell_or_an_added_shell_has() {
        // Shell 3 is a stage shell, Shell 4 was added by hand.
        let all = terminal_names(&names(&["1", "3"]), &names(&["4"]));
        assert_eq!(next_terminal_name(&all), "5");
        assert_eq!(next_terminal_name(&names(&["1", "3"])), "4");
    }

    #[test]
    fn gives_the_agent_shells_that_the_open_list_does_not_have() {
        assert_eq!(unlisted_shells(&names(&["1"]), &names(&["1", "2"])), ["2"]);
        assert!(unlisted_shells(&names(&["1", "2"]), &names(&["2"])).is_empty());
        assert!(unlisted_shells(&names(&["1"]), &[]).is_empty());
    }
}
