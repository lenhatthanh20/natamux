//! The background jobs that run now (`jobs.ts`), such as a push or a CI round. In memory only:
//! they end with the daemon.

use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use natamux_core::js::iso_from_millis;
use natamux_proto::tasks::job::Job;

use crate::shared::event_bus::{DaemonMessage, EventBus};

/// Milliseconds since 1970.
pub type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

pub fn system_clock() -> Clock {
    Arc::new(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_millis() as i64)
    })
}

pub struct Jobs {
    jobs: Mutex<Vec<Job>>,
    next: AtomicU64,
    bus: Option<Arc<EventBus>>,
    now: Clock,
}

/// Drops the job when its work ends, also when the work is dropped half way.
struct Running<'a> {
    jobs: &'a Jobs,
    id: String,
}

impl Drop for Running<'_> {
    fn drop(&mut self) {
        if let Ok(mut jobs) = self.jobs.jobs.lock() {
            jobs.retain(|job| job.id != self.id);
        }
        self.jobs.publish();
    }
}

impl Jobs {
    pub fn new(bus: Option<Arc<EventBus>>, now: Clock) -> Jobs {
        Jobs {
            jobs: Mutex::new(Vec::new()),
            next: AtomicU64::new(1),
            bus,
            now,
        }
    }

    /// Lists the job while `work` runs.
    pub async fn run<T>(
        &self,
        kind: &str,
        label: &str,
        task_id: Option<&str>,
        work: impl Future<Output = T>,
    ) -> T {
        let _running = self.begin(kind, label, task_id);
        work.await
    }

    /// `run` for blocking work on a thread of its own, such as the checks of a stage.
    pub fn run_blocking<T>(
        &self,
        kind: &str,
        label: &str,
        task_id: Option<&str>,
        work: impl FnOnce() -> T,
    ) -> T {
        let _running = self.begin(kind, label, task_id);
        work()
    }

    fn begin(&self, kind: &str, label: &str, task_id: Option<&str>) -> Running<'_> {
        let id = format!("job-{}", self.next.fetch_add(1, Ordering::SeqCst));
        let job = Job {
            id: id.clone(),
            kind: kind.into(),
            label: label.into(),
            task_id: task_id.map(str::to_string),
            started_at: iso_from_millis((self.now)()),
        };
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.push(job);
        }
        self.publish();
        Running { jobs: self, id }
    }

    /// Oldest first.
    pub fn list(&self) -> Vec<Job> {
        self.jobs
            .lock()
            .map_or_else(|_| Vec::new(), |jobs| jobs.clone())
    }

    fn publish(&self) {
        if let Some(bus) = &self.bus {
            bus.publish(DaemonMessage::JobsChanged { jobs: self.list() });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicI64;

    use super::*;

    fn setup() -> (Arc<Jobs>, Arc<Mutex<Vec<DaemonMessage>>>) {
        let bus = EventBus::new();
        let messages: Arc<Mutex<Vec<DaemonMessage>>> = Arc::default();
        let sink = Arc::clone(&messages);
        bus.subscribe(move |message| sink.lock().unwrap().push(message.clone()));
        let now = Arc::new(AtomicI64::new(1_792_141_200_000));
        let clock: Clock = Arc::new(move || now.fetch_add(1, Ordering::SeqCst));
        (Arc::new(Jobs::new(Some(bus), clock)), messages)
    }

    fn block_on<T>(work: impl Future<Output = T>) -> T {
        futures::executor::block_on(work)
    }

    #[test]
    fn lists_a_job_while_its_work_runs_and_drops_it_after() {
        let (jobs, messages) = setup();
        let (finish, done) = futures::channel::oneshot::channel::<&str>();
        let inner = Arc::clone(&jobs);
        let mut running = Box::pin(async move {
            inner
                .run("push", "Pushing", Some("T-0001"), async {
                    done.await.unwrap()
                })
                .await
        });
        let waker = futures::task::noop_waker();
        let mut context = std::task::Context::from_waker(&waker);
        assert!(running.as_mut().poll(&mut context).is_pending());
        let list = jobs.list();
        assert_eq!(list.len(), 1);
        assert_eq!(
            (
                list[0].kind.as_str(),
                list[0].label.as_str(),
                list[0].task_id.as_deref()
            ),
            ("push", "Pushing", Some("T-0001"))
        );
        assert_eq!(list[0].started_at, "2026-10-16T09:00:00.000Z");
        assert_eq!(
            *messages.lock().unwrap(),
            [DaemonMessage::JobsChanged { jobs: list }]
        );
        finish.send("done").unwrap();
        assert_eq!(block_on(running), "done");
        assert!(jobs.list().is_empty());
        assert_eq!(messages.lock().unwrap().len(), 2);
        assert_eq!(
            messages.lock().unwrap().last(),
            Some(&DaemonMessage::JobsChanged { jobs: Vec::new() })
        );
    }

    #[test]
    fn drops_a_job_whose_work_throws_and_passes_the_error_on() {
        let (jobs, messages) = setup();
        let result: Result<(), &str> =
            block_on(jobs.run("push", "Pushing", Some("T-0001"), async { Err("rejected") }));
        assert_eq!(result, Err("rejected"));
        assert!(jobs.list().is_empty());
        assert_eq!(
            messages.lock().unwrap().last(),
            Some(&DaemonMessage::JobsChanged { jobs: Vec::new() })
        );
    }

    #[test]
    fn lists_jobs_oldest_first_each_with_its_own_id() {
        let (jobs, _) = setup();
        let waker = futures::task::noop_waker();
        let mut context = std::task::Context::from_waker(&waker);
        let mut first = Box::pin(jobs.run(
            "push",
            "Pushing",
            Some("T-0001"),
            futures::future::pending::<()>(),
        ));
        let mut second = Box::pin(jobs.run(
            "import",
            "Importing issues",
            None,
            futures::future::pending::<()>(),
        ));
        assert!(first.as_mut().poll(&mut context).is_pending());
        assert!(second.as_mut().poll(&mut context).is_pending());
        let list = jobs.list();
        let labels: Vec<&str> = list.iter().map(|job| job.label.as_str()).collect();
        assert_eq!(labels, ["Pushing", "Importing issues"]);
        assert_ne!(list[0].id, list[1].id);
    }
}
