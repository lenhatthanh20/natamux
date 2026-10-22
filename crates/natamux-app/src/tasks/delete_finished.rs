//! Deleting the finished tasks of a column (`features/tasks/delete-finished.ts`).

use crate::components::toast_rules::ToastTone;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeleteOutcome {
    Deleted { id: String },
    Failed { id: String, reason: String },
}

impl DeleteOutcome {
    #[allow(dead_code)]
    pub fn id(&self) -> &str {
        match self {
            DeleteOutcome::Deleted { id } | DeleteOutcome::Failed { id, .. } => id,
        }
    }
}

/// Deletes the tasks one after the other: a task with a workspace runs git work, and git work on
/// the same repository must not race. The caller runs this off the UI thread. A failure is kept
/// as an outcome, so the rest still go. `on_deleted` gets the result of each delete that worked.
pub fn delete_each<R>(
    ids: &[String],
    mut remove: impl FnMut(&str) -> Result<R, String>,
    mut on_deleted: impl FnMut(R),
) -> Vec<DeleteOutcome> {
    ids.iter()
        .map(|id| match remove(id) {
            Ok(result) => {
                on_deleted(result);
                DeleteOutcome::Deleted { id: id.clone() }
            }
            Err(message) => {
                let reason = if message.trim().is_empty() {
                    "the daemon gave no reason".to_string()
                } else {
                    message
                };
                DeleteOutcome::Failed {
                    id: id.clone(),
                    reason,
                }
            }
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeleteSummary {
    pub text: String,
    pub tone: ToastTone,
    pub detail: Option<String>,
}

/// One toast for the whole run.
pub fn delete_summary(outcomes: &[DeleteOutcome]) -> DeleteSummary {
    let failed: Vec<(&str, &str)> = outcomes
        .iter()
        .filter_map(|outcome| match outcome {
            DeleteOutcome::Failed { id, reason } => Some((id.as_str(), reason.as_str())),
            DeleteOutcome::Deleted { .. } => None,
        })
        .collect();
    let tasks = match outcomes.len() {
        1 => "1 task".to_string(),
        count => format!("{count} tasks"),
    };
    if failed.is_empty() {
        return DeleteSummary {
            text: format!("Deleted {tasks}."),
            tone: ToastTone::Success,
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
    DeleteSummary {
        text: format!("Deleted {} of {tasks}.", outcomes.len() - failed.len()),
        tone: ToastTone::Error,
        detail: Some(format!("Not deleted: {}", reasons.join("; "))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIRTY: &str = "T-0002 has 2 changes that are not committed. Commit or discard them first";

    fn strings(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|id| (*id).into()).collect()
    }

    fn ok(id: &str) -> DeleteOutcome {
        DeleteOutcome::Deleted { id: id.into() }
    }

    fn failed(id: &str, reason: &str) -> DeleteOutcome {
        DeleteOutcome::Failed {
            id: id.into(),
            reason: reason.into(),
        }
    }

    #[test]
    fn deletes_one_task_after_the_other_and_goes_on_after_a_failure() {
        let mut order = Vec::new();
        let mut deleted = Vec::new();
        let outcomes = delete_each(
            &strings(&["T-0001", "T-0002", "T-0003"]),
            |id| {
                order.push(id.to_string());
                if id == "T-0002" {
                    Err(DIRTY.to_string())
                } else {
                    Ok(id.to_string())
                }
            },
            |result| deleted.push(result),
        );
        assert_eq!(order, ["T-0001", "T-0002", "T-0003"]);
        assert_eq!(
            outcomes,
            [ok("T-0001"), failed("T-0002", DIRTY), ok("T-0003")]
        );
        assert_eq!(deleted, ["T-0001", "T-0003"]);
    }

    #[test]
    fn gives_a_reason_when_the_error_has_none() {
        let outcomes = delete_each(
            &strings(&["T-0001", "T-0002"]),
            |id| -> Result<(), String> {
                Err(if id == "T-0001" {
                    "  ".into()
                } else {
                    "offline".into()
                })
            },
            |_| {},
        );
        assert_eq!(
            outcomes,
            [
                failed("T-0001", "the daemon gave no reason"),
                failed("T-0002", "offline")
            ]
        );
    }

    #[test]
    fn does_nothing_for_an_empty_list() {
        let mut calls = 0;
        let outcomes = delete_each(
            &[],
            |_| {
                calls += 1;
                Ok(())
            },
            |_| {},
        );
        assert!(outcomes.is_empty());
        assert_eq!(calls, 0);
    }

    #[test]
    fn counts_the_deleted_tasks() {
        assert_eq!(
            delete_summary(&[ok("T-0001")]),
            DeleteSummary {
                text: "Deleted 1 task.".into(),
                tone: ToastTone::Success,
                detail: None
            }
        );
        assert_eq!(
            delete_summary(&[ok("T-0001"), ok("T-0002"), ok("T-0003")]).text,
            "Deleted 3 tasks."
        );
    }

    #[test]
    fn lists_the_tasks_that_were_not_deleted_each_with_its_own_reason() {
        let summary = delete_summary(&[
            ok("T-0001"),
            failed("T-0002", DIRTY),
            failed("T-0003", "Network error"),
        ]);
        assert_eq!(summary.text, "Deleted 1 of 3 tasks.");
        assert_eq!(summary.tone, ToastTone::Error);
        assert_eq!(
            summary.detail,
            Some(format!("Not deleted: {DIRTY}; T-0003: Network error"))
        );
    }

    #[test]
    fn reports_a_run_where_nothing_was_deleted() {
        assert_eq!(
            delete_summary(&[failed("T-0001", "Gone")]),
            DeleteSummary {
                text: "Deleted 0 of 1 task.".into(),
                tone: ToastTone::Error,
                detail: Some("Not deleted: T-0001: Gone".into())
            }
        );
    }
}
