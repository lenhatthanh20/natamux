use std::sync::mpsc::channel;
use std::time::Duration;

use natamux_pty::{Pty, PtyEvents, Signal, SpawnOptions};

use super::*;
use crate::terminals::types::TerminalClient;

struct StillPty;

impl Pty for StillPty {
    fn start(&mut self, _events: PtyEvents) {}
    fn write(&mut self, _data: &[u8]) {}
    fn resize(&mut self, _cols: u16, _rows: u16) {}
    fn pause(&mut self) {}
    fn resume(&mut self) {}
    fn kill(&mut self, _signal: Signal) {}
}

struct Nobody;

impl TerminalClient for Nobody {
    fn send(&self, _frame: Frame) {}
}

fn spawn(_: SpawnOptions) -> std::io::Result<Box<dyn Pty>> {
    Ok(Box::new(StillPty))
}

fn service() -> TerminalService {
    TerminalService::new(TerminalOptions {
        spawn: Arc::new(spawn),
        workspace_of: Arc::new(|task| Ok(format!("/work/{task}").into())),
        env: Default::default(),
        scheduler: Scheduler::manual(),
        on_output: None,
        is_ended: None,
        bus: None,
        on_output_idle: None,
        on_exit: None,
        trace_input: false,
    })
}

// T-0210: the quiet check of one task locked every session of the daemon, also one a parser
// held, so the flush timers of all shells waited behind it.
#[test]
fn the_quiet_check_of_a_task_never_waits_for_a_session_of_another_task() {
    let service = service();
    let size = Size { cols: 80, rows: 24 };
    let mut detaches = Vec::new();
    for task in ["T-0001", "T-00010", "T-0002"] {
        let client: ClientRef = Arc::new(Nobody);
        let options = AttachOptions::default();
        let detach = service.attach(task, client, size, "1", options).unwrap();
        detaches.push(detach);
    }
    let other = service.inner.get("T-0002", "1").unwrap();
    let (locked, is_locked) = channel();
    let (release, released) = channel::<()>();
    let holder = std::thread::spawn(move || {
        let _guard = lock(&other);
        locked.send(()).unwrap();
        released.recv().ok();
    });
    is_locked.recv().unwrap();

    let (done, finished) = channel();
    let inner = Arc::clone(&service.inner);
    std::thread::spawn(move || {
        inner.note_foreground("T-0001");
        done.send(inner.sessions_of_task("T-0001").len()).ok();
    });
    let found = finished.recv_timeout(Duration::from_secs(5));
    release.send(()).unwrap();
    holder.join().unwrap();
    // Only its own shell: "T-00010" starts with "T-0001" but is another task.
    assert_eq!(found, Ok(1));
}
