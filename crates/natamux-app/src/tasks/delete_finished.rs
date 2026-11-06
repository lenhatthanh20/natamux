//! Deleting the finished tasks of a column (`features/tasks/delete-finished.ts`), and the queue
//! every delete of the app goes through.

use std::collections::VecDeque;

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

/// One delete the user confirmed. `batch`: the board's Delete all, which ends with one summary
/// toast; else one task with its own toast.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeleteJob {
    pub ids: Vec<String>,
    pub force: Vec<String>,
    pub batch: bool,
}

/// The deletes that wait for the daemon. The views hide the tasks at once; the jobs run one at a
/// time, so git work on the same repository does not race (see `delete_each`).
#[derive(Debug, Default)]
pub struct DeleteQueue {
    waiting: VecDeque<DeleteJob>,
    running: Option<DeleteJob>,
}

impl DeleteQueue {
    /// Queues the ids that are not queued or running yet and gives them back. Nothing is queued
    /// when all are known: a second click, or a task in a single and a bulk delete.
    pub fn push(&mut self, ids: Vec<String>, force: Vec<String>, batch: bool) -> Vec<String> {
        let mut fresh: Vec<String> = Vec::new();
        for id in ids {
            if !self.knows(&id) && !fresh.contains(&id) {
                fresh.push(id);
            }
        }
        if !fresh.is_empty() {
            let force = force.into_iter().filter(|id| fresh.contains(id)).collect();
            self.waiting.push_back(DeleteJob {
                ids: fresh.clone(),
                force,
                batch,
            });
        }
        fresh
    }

    /// The next job, which now runs. None while one runs or when none waits.
    pub fn start_next(&mut self) -> Option<DeleteJob> {
        if self.running.is_some() {
            return None;
        }
        self.running = self.waiting.pop_front();
        self.running.clone()
    }

    pub fn finish(&mut self) {
        self.running = None;
    }

    /// A Delete all waits or runs: the board's trash buttons are off.
    pub fn batch_queued(&self) -> bool {
        self.jobs().any(|job| job.batch)
    }

    fn knows(&self, id: &str) -> bool {
        self.jobs().any(|job| job.ids.iter().any(|each| each == id))
    }

    fn jobs(&self) -> impl Iterator<Item = &DeleteJob> {
        self.running.iter().chain(self.waiting.iter())
    }
}

/// The toast of a single delete.
pub fn delete_toast(outcome: &DeleteOutcome) -> (String, ToastTone) {
    match outcome {
        DeleteOutcome::Deleted { id } => (format!("Deleted {id}."), ToastTone::Info),
        DeleteOutcome::Failed { id, reason } => (
            format!("Could not delete {id}: {reason}"),
            ToastTone::Error,
        ),
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

    #[test]
    fn runs_one_job_at_a_time_in_order() {
        let mut queue = DeleteQueue::default();
        assert_eq!(queue.push(strings(&["T-1"]), Vec::new(), false), ["T-1"]);
        queue.push(strings(&["T-2"]), strings(&["T-2"]), false);
        let first = queue.start_next();
        assert_eq!(first.map(|job| job.ids), Some(strings(&["T-1"])));
        // T-1 still runs.
        assert_eq!(queue.start_next(), None);
        queue.finish();
        assert_eq!(
            queue.start_next(),
            Some(DeleteJob {
                ids: strings(&["T-2"]),
                force: strings(&["T-2"]),
                batch: false
            })
        );
        queue.finish();
        assert_eq!(queue.start_next(), None);
    }

    #[test]
    fn queues_a_task_only_once() {
        let mut queue = DeleteQueue::default();
        queue.push(strings(&["T-1"]), Vec::new(), false);
        queue.start_next();
        // Running, then waiting: both count.
        assert!(queue.push(strings(&["T-1"]), Vec::new(), false).is_empty());
        queue.push(strings(&["T-2"]), Vec::new(), false);
        assert_eq!(
            queue.push(
                strings(&["T-1", "T-2", "T-3", "T-3"]),
                strings(&["T-1", "T-3"]),
                true
            ),
            ["T-3"]
        );
        queue.finish();
        queue.start_next();
        queue.finish();
        assert_eq!(
            queue.start_next(),
            Some(DeleteJob {
                ids: strings(&["T-3"]),
                force: strings(&["T-3"]),
                batch: true
            })
        );
    }

    #[test]
    fn a_batch_counts_while_it_waits_and_while_it_runs() {
        let mut queue = DeleteQueue::default();
        queue.push(strings(&["T-1"]), Vec::new(), false);
        assert!(!queue.batch_queued());
        queue.push(strings(&["T-2", "T-3"]), Vec::new(), true);
        assert!(queue.batch_queued());
        queue.start_next();
        queue.finish();
        queue.start_next();
        assert!(queue.batch_queued());
        queue.finish();
        assert!(!queue.batch_queued());
    }

    #[test]
    fn words_the_toast_of_one_delete() {
        assert_eq!(
            delete_toast(&ok("T-0001")),
            ("Deleted T-0001.".into(), ToastTone::Info)
        );
        assert_eq!(
            delete_toast(&failed("T-0002", "offline")),
            ("Could not delete T-0002: offline".into(), ToastTone::Error)
        );
    }
}
