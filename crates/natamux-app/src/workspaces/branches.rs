//! Port of `branchOptions` in `packages/web/src/features/workspaces/branches.ts`. The last used
//! branch is kept in the app's view file, not here.

/// The branches that Natamux made for tasks. They fill the list, so they are folded away.
pub const TASK_BRANCH_GROUP: &str = "Task branches";
const TASK_BRANCH_PREFIX: &str = "natamux/";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BranchOption {
    pub value: String,
    pub label: String,
    pub group: &'static str,
}

fn label_of(name: &str, is_default: bool, is_current: bool) -> String {
    let marks: Vec<&str> = [(is_default, "default"), (is_current, "current")]
        .into_iter()
        .filter_map(|(on, mark)| on.then_some(mark))
        .collect();
    if marks.is_empty() {
        name.to_string()
    } else {
        format!("{name} ({})", marks.join(", "))
    }
}

/// The list of the "Branch from" picker: the default branch, the branch of the checkout and the
/// one used last come first, then the other branches, then the task branches.
pub fn branch_options(
    branches: &[String],
    current: Option<&str>,
    last_used: Option<&str>,
    default_branch: Option<&str>,
) -> Vec<BranchOption> {
    let mut recent: Vec<&str> = Vec::new();
    for name in [default_branch, current, last_used].into_iter().flatten() {
        if branches.iter().any(|b| b == name) && !recent.contains(&name) {
            recent.push(name);
        }
    }
    let rest: Vec<&str> = branches
        .iter()
        .map(String::as_str)
        .filter(|name| !recent.contains(name))
        .collect();
    let option = |value: &str, label: String, group| BranchOption {
        value: value.to_string(),
        label,
        group,
    };
    let recent_options = recent.iter().map(|&name| {
        let label = label_of(name, Some(name) == default_branch, Some(name) == current);
        option(name, label, "Recent")
    });
    let plain = rest
        .iter()
        .filter(|name| !name.starts_with(TASK_BRANCH_PREFIX))
        .map(|&name| option(name, name.to_string(), "Branches"));
    let task = rest
        .iter()
        .filter(|name| name.starts_with(TASK_BRANCH_PREFIX))
        .map(|&name| option(name, name.to_string(), TASK_BRANCH_GROUP));
    recent_options.chain(plain).chain(task).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn branches() -> Vec<String> {
        [
            "main",
            "dev",
            "natamux/t-0001-a",
            "natamux/t-0002-b",
            "feature/x",
        ]
        .map(String::from)
        .to_vec()
    }

    fn pairs(options: &[BranchOption]) -> Vec<(&str, &str)> {
        options
            .iter()
            .map(|o| (o.group, o.value.as_str()))
            .collect()
    }

    #[test]
    fn puts_the_current_and_the_last_used_branch_first_then_branches_then_task_branches() {
        let options = branch_options(&branches(), Some("main"), Some("feature/x"), None);
        assert_eq!(
            pairs(&options),
            [
                ("Recent", "main"),
                ("Recent", "feature/x"),
                ("Branches", "dev"),
                (TASK_BRANCH_GROUP, "natamux/t-0001-a"),
                (TASK_BRANCH_GROUP, "natamux/t-0002-b"),
            ]
        );
        assert_eq!(options[0].label, "main (current)");
    }

    #[test]
    fn lists_a_branch_once_and_ignores_a_last_used_branch_that_is_gone() {
        let same = branch_options(&branches(), Some("main"), Some("main"), None);
        assert_eq!(same.iter().filter(|o| o.value == "main").count(), 1);
        let gone = branch_options(&branches(), Some("main"), Some("deleted"), None);
        assert!(!gone.iter().any(|o| o.value == "deleted"));
    }

    #[test]
    fn puts_the_default_branch_first_then_the_current_and_the_last_used_one() {
        let options = branch_options(&branches(), Some("dev"), Some("feature/x"), Some("main"));
        assert_eq!(
            pairs(&options),
            [
                ("Recent", "main"),
                ("Recent", "dev"),
                ("Recent", "feature/x"),
                (TASK_BRANCH_GROUP, "natamux/t-0001-a"),
                (TASK_BRANCH_GROUP, "natamux/t-0002-b"),
            ]
        );
        assert_eq!(options[0].label, "main (default)");
        assert_eq!(options[1].label, "dev (current)");
    }

    #[test]
    fn lists_the_default_branch_once_when_it_is_the_current_one_too() {
        let options = branch_options(&branches(), Some("main"), None, Some("main"));
        let mains: Vec<_> = options.iter().filter(|o| o.value == "main").collect();
        assert_eq!(
            mains,
            [&BranchOption {
                value: "main".into(),
                label: "main (default, current)".into(),
                group: "Recent"
            }]
        );
    }

    #[test]
    fn works_without_a_current_branch() {
        assert_eq!(
            branch_options(&["a".to_string()], None, None, None),
            [BranchOption {
                value: "a".into(),
                label: "a".into(),
                group: "Branches"
            }]
        );
    }
}
