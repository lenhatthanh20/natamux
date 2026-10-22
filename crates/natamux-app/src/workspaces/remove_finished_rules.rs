//! Port of `removeEach` and `removeSummary` in `packages/web/src/features/workspaces/remove-finished.ts`.

use natamux_proto::workspaces::workspace::RemoveWorkspaceAnswer;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoveOutcome {
    Removed { id: String, branch_deleted: bool },
    Failed { id: String, reason: String },
}

/// What a removal answer says about the branch.
pub trait Removal {
    fn branch_deleted(&self) -> bool;
}

impl Removal for RemoveWorkspaceAnswer {
    fn branch_deleted(&self) -> bool {
        self.branch_deleted
    }
}

/// Removes the workspaces one after the other: git work on the same repository must not race. A
/// failure is kept as an outcome, so the rest still go. `on_removed` sees each result before the
/// next removal starts.
pub fn remove_each<R: Removal>(
    ids: &[String],
    mut remove: impl FnMut(&str) -> Result<R, String>,
    mut on_removed: impl FnMut(&R),
) -> Vec<RemoveOutcome> {
    ids.iter()
        .map(|id| match remove(id) {
            Ok(result) => {
                on_removed(&result);
                RemoveOutcome::Removed {
                    id: id.clone(),
                    branch_deleted: result.branch_deleted(),
                }
            }
            Err(message) => {
                let reason = if message.trim().is_empty() {
                    "the daemon gave no reason".to_string()
                } else {
                    message
                };
                RemoveOutcome::Failed {
                    id: id.clone(),
                    reason,
                }
            }
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SummaryTone {
    Success,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoveSummary {
    pub text: String,
    pub tone: SummaryTone,
    pub detail: Option<String>,
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// One toast for the whole run.
pub fn remove_summary(outcomes: &[RemoveOutcome]) -> RemoveSummary {
    let mut removed = 0;
    let mut kept = 0;
    let mut failed: Vec<(&str, &str)> = Vec::new();
    for outcome in outcomes {
        match outcome {
            RemoveOutcome::Removed { branch_deleted, .. } => {
                removed += 1;
                kept += usize::from(!branch_deleted);
            }
            RemoveOutcome::Failed { id, reason } => failed.push((id, reason)),
        }
    }
    let count = if failed.is_empty() {
        format!("Removed {}.", plural(removed, "workspace", "workspaces"))
    } else {
        format!(
            "Removed {removed} of {}.",
            plural(outcomes.len(), "workspace", "workspaces")
        )
    };
    let branches = match kept {
        0 => String::new(),
        1 => " 1 branch kept because it has commits that are not merged.".into(),
        n => format!(" {n} branches kept because they have commits that are not merged."),
    };
    let text = count + &branches;
    if failed.is_empty() {
        return RemoveSummary {
            text,
            tone: SummaryTone::Success,
            detail: None,
        };
    }
    // The daemon's messages already start with the task id.
    let reasons: Vec<String> = failed
        .iter()
        .map(|(id, reason)| {
            if reason.starts_with(id) {
                (*reason).to_string()
            } else {
                format!("{id}: {reason}")
            }
        })
        .collect();
    let detail = Some(format!("Not removed: {}", reasons.join("; ")));
    RemoveSummary {
        text,
        tone: SummaryTone::Error,
        detail,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIRTY: &str = "T-0002 has 2 changes that are not committed. Commit or discard them first";

    struct Result(bool, &'static str);
    impl Removal for Result {
        fn branch_deleted(&self) -> bool {
            self.0
        }
    }

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|id| id.to_string()).collect()
    }

    fn ok(id: &str, branch_deleted: bool) -> RemoveOutcome {
        RemoveOutcome::Removed {
            id: id.into(),
            branch_deleted,
        }
    }

    fn failed(id: &str, reason: &str) -> RemoveOutcome {
        RemoveOutcome::Failed {
            id: id.into(),
            reason: reason.into(),
        }
    }

    #[test]
    fn removes_one_workspace_after_the_other_and_goes_on_after_a_failure() {
        let mut order = Vec::new();
        let mut handed = Vec::new();
        let outcomes = remove_each(
            &ids(&["T-0001", "T-0002", "T-0003"]),
            |id| {
                order.push(id.to_string());
                if id == "T-0002" {
                    Err(DIRTY.into())
                } else {
                    Ok(Result(id != "T-0003", "x"))
                }
            },
            |result| handed.push(result.0),
        );
        assert_eq!(order, ["T-0001", "T-0002", "T-0003"]);
        assert_eq!(
            outcomes,
            [
                ok("T-0001", true),
                failed("T-0002", DIRTY),
                ok("T-0003", false)
            ]
        );
        assert_eq!(handed, [true, false]);
    }

    #[test]
    fn hands_each_result_over_before_the_next_removal_starts() {
        let steps = std::cell::RefCell::new(Vec::new());
        remove_each(
            &ids(&["T-0001", "T-0002"]),
            |id| {
                steps.borrow_mut().push(format!("remove {id}"));
                Ok(Result(
                    true,
                    if id == "T-0001" { "T-0001" } else { "T-0002" },
                ))
            },
            |result| steps.borrow_mut().push(format!("removed {}", result.1)),
        );
        assert_eq!(
            *steps.borrow(),
            [
                "remove T-0001",
                "removed T-0001",
                "remove T-0002",
                "removed T-0002"
            ]
        );
    }

    #[test]
    fn goes_on_when_the_first_or_the_last_one_fails_and_when_all_fail() {
        let all = ids(&["T-0001", "T-0002", "T-0003"]);
        let run = |bad: &[&str]| -> Vec<bool> {
            remove_each(
                &all,
                |id| {
                    if bad.contains(&id) {
                        Err(format!("{id} failed"))
                    } else {
                        Ok(Result(true, ""))
                    }
                },
                |_| {},
            )
            .iter()
            .map(|outcome| matches!(outcome, RemoveOutcome::Removed { .. }))
            .collect()
        };
        assert_eq!(run(&["T-0001"]), [false, true, true]);
        assert_eq!(run(&["T-0003"]), [true, true, false]);
        assert_eq!(run(&["T-0001", "T-0002", "T-0003"]), [false, false, false]);
    }

    #[test]
    fn keeps_the_message_of_any_failure_and_gives_a_reason_when_there_is_none() {
        let outcomes = remove_each::<Result>(
            &ids(&["T-0001", "T-0002", "T-0003"]),
            |id| {
                Err(match id {
                    "T-0001" => "The daemon answered with status 500.".into(),
                    "T-0002" => String::new(),
                    _ => "offline".into(),
                })
            },
            |_| {},
        );
        assert_eq!(
            outcomes,
            [
                failed("T-0001", "The daemon answered with status 500."),
                failed("T-0002", "the daemon gave no reason"),
                failed("T-0003", "offline"),
            ]
        );
    }

    #[test]
    fn does_nothing_for_an_empty_list() {
        let mut called = false;
        let outcomes = remove_each::<Result>(
            &[],
            |_| {
                called = true;
                Err(String::new())
            },
            |_| {},
        );
        assert!(outcomes.is_empty());
        assert!(!called);
    }

    #[test]
    fn counts_the_removed_workspaces() {
        let one = remove_summary(&[ok("T-0001", true)]);
        assert_eq!(one.text, "Removed 1 workspace.");
        assert_eq!((one.tone, one.detail), (SummaryTone::Success, None));
        let three = remove_summary(&[ok("T-0001", true), ok("T-0002", true), ok("T-0003", true)]);
        assert_eq!(three.text, "Removed 3 workspaces.");
    }

    #[test]
    fn says_how_many_branches_stay_and_nothing_about_branches_when_none_do() {
        assert!(
            !remove_summary(&[ok("T-0001", true), ok("T-0002", true)])
                .text
                .contains("branch")
        );
        assert_eq!(
            remove_summary(&[ok("T-0001", false), ok("T-0002", true), ok("T-0003", true)]).text,
            "Removed 3 workspaces. 1 branch kept because it has commits that are not merged."
        );
        assert_eq!(
            remove_summary(&[ok("T-0001", false), ok("T-0002", false), ok("T-0003", true)]).text,
            "Removed 3 workspaces. 2 branches kept because they have commits that are not merged."
        );
    }

    #[test]
    fn lists_the_workspaces_that_were_not_removed_each_with_its_own_reason() {
        let summary = remove_summary(&[
            ok("T-0001", true),
            failed("T-0002", DIRTY),
            failed("T-0003", "Network error"),
            failed("T-0004", "T-0004 has no workspace"),
        ]);
        assert_eq!(summary.text, "Removed 1 of 4 workspaces.");
        assert_eq!(summary.tone, SummaryTone::Error);
        assert_eq!(
            summary.detail.as_deref(),
            Some(
                format!("Not removed: {DIRTY}; T-0003: Network error; T-0004 has no workspace")
                    .as_str()
            )
        );
    }

    #[test]
    fn reports_a_run_where_nothing_was_removed() {
        let summary = remove_summary(&[failed("T-0001", "Gone"), failed("T-0002", "Gone")]);
        assert_eq!(summary.text, "Removed 0 of 2 workspaces.");
        assert_eq!(summary.tone, SummaryTone::Error);
        assert_eq!(
            summary.detail.as_deref(),
            Some("Not removed: T-0001: Gone; T-0002: Gone")
        );
    }

    #[test]
    fn keeps_the_branch_count_when_some_fail() {
        assert_eq!(
            remove_summary(&[ok("T-0001", false), failed("T-0002", "Gone")]).text,
            "Removed 1 of 2 workspaces. 1 branch kept because it has commits that are not merged."
        );
    }
}
