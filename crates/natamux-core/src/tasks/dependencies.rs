//! `makesCycle` (`packages/core/src/tasks/dependencies.ts`).

use std::collections::{HashMap, HashSet};

/// True when `task_id` depending on `depends_on` would make a loop: one of them already depends,
/// directly or through others, on `task_id`. `tasks`: each task's id and dependencies.
pub fn makes_cycle(task_id: &str, depends_on: &[String], tasks: &[(String, Vec<String>)]) -> bool {
    let edges: HashMap<&str, &Vec<String>> =
        tasks.iter().map(|(id, deps)| (id.as_str(), deps)).collect();
    let mut seen = HashSet::new();
    let mut stack: Vec<&str> = depends_on.iter().map(String::as_str).collect();
    while let Some(id) = stack.pop() {
        if id == task_id {
            return true;
        }
        if !seen.insert(id) {
            continue;
        }
        if let Some(next) = edges.get(id) {
            stack.extend(next.iter().map(String::as_str));
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tasks(list: &[(&str, &[&str])]) -> Vec<(String, Vec<String>)> {
        list.iter()
            .map(|(id, deps)| (id.to_string(), deps.iter().map(|d| d.to_string()).collect()))
            .collect()
    }

    #[test]
    fn finds_a_loop_in_a_chain_of_three() {
        let all = tasks(&[("T-1", &[]), ("T-2", &["T-1"]), ("T-3", &["T-2"])]);
        assert!(makes_cycle("T-1", &["T-3".into()], &all));
        assert!(!makes_cycle("T-3", &["T-1".into()], &all));
        assert!(makes_cycle("T-1", &["T-1".into()], &all));
    }

    fn chain() -> Vec<(String, Vec<String>)> {
        tasks(&[
            ("T-0001", &["T-0002"]),
            ("T-0002", &["T-0003"]),
            ("T-0003", &[]),
            ("T-0004", &[]),
        ])
    }

    #[test]
    fn finds_a_loop_through_other_tasks() {
        assert!(makes_cycle("T-0003", &["T-0001".into()], &chain()));
        assert!(makes_cycle("T-0001", &["T-0001".into()], &chain()));
    }

    #[test]
    fn allows_a_chain_without_a_loop() {
        assert!(!makes_cycle(
            "T-0004",
            &["T-0001".into(), "T-0003".into()],
            &chain()
        ));
        assert!(!makes_cycle("T-0003", &[], &chain()));
    }
}
