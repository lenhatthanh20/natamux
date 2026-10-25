//! Background jobs (`Job` in `packages/daemon/src/shared/event-bus.ts`): work the daemon does
//! that a person waits on, such as a push. Oldest first in `GET /jobs` and in `jobs.changed`.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub kind: String,
    pub label: String,
    /// None: the job is about no one task.
    #[serde(default)]
    pub task_id: Option<String>,
    pub started_at: String,
}

/// The body of `GET /jobs`, and the payload of `jobs.changed`.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct JobList {
    #[serde(default)]
    pub jobs: Vec<Job>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_jobs_in_the_daemons_order_and_ignores_other_fields() {
        let list: JobList = serde_json::from_str(
            r#"{"jobs":[
                {"id":"j1","kind":"push","label":"Pushing T-0001","taskId":"T-0001",
                 "startedAt":"2026-10-22T01:00:00.000Z","later":true},
                {"id":"j2","kind":"ci","label":"CI","taskId":null,"startedAt":"2026-10-22T01:00:05.000Z"}
            ]}"#,
        )
        .unwrap();
        assert_eq!(list.jobs.len(), 2);
        assert_eq!(list.jobs[0].task_id.as_deref(), Some("T-0001"));
        assert_eq!(list.jobs[1].task_id, None);
    }

    #[test]
    fn a_jobs_changed_message_reads_as_a_list() {
        let list: JobList = serde_json::from_str(r#"{"kind":"jobs.changed","jobs":[]}"#).unwrap();
        assert!(list.jobs.is_empty());
    }
}
