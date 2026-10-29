//! The `ps` behind the commands of a shell and behind its close runs with no session lock held
//! and not in the caller's turn, so a slow `ps` holds no other shell. Real clock.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use natamux_daemon::shared::scheduler::Scheduler;
use natamux_daemon::terminals::limits::KILL_GRACE_MS;
use natamux_daemon::terminals::service::{AttachOptions, TerminalOptions, TerminalService};
use natamux_daemon::terminals::types::{ClientRef, Frame, Size, TerminalClient};
use natamux_pty::{ProcessTable, Pty, PtyEvents, Signal, SpawnOptions, TableReader};

const TASK: &str = "T-0001";
const SLOW_PS: Duration = Duration::from_millis(300);

type Log = Arc<Mutex<Vec<String>>>;

struct PsPty {
    log: Log,
    read: TableReader,
}

impl Pty for PsPty {
    fn start(&mut self, _events: PtyEvents) {}
    fn write(&mut self, _data: &[u8]) {}
    fn resize(&mut self, _cols: u16, _rows: u16) {}
    fn pause(&mut self) {}
    fn resume(&mut self) {}
    fn kill(&mut self, signal: Signal) {
        self.log
            .lock()
            .unwrap()
            .push(format!("{signal:?} with no table"));
    }
    fn processes(&self) -> Option<TableReader> {
        Some(Arc::clone(&self.read))
    }
    fn kill_in(&mut self, signal: Signal, _table: &ProcessTable) {
        self.log.lock().unwrap().push(format!("{signal:?}"));
    }
    fn commands_in(&self, _table: &ProcessTable) -> Option<Vec<String>> {
        self.log.lock().unwrap().push("commands".to_string());
        Some(Vec::new())
    }
}

struct Nobody;

impl TerminalClient for Nobody {
    fn send(&self, _frame: Frame) {}
}

/// Another caller that needs the lock of shell 1 gets it while the `ps` runs.
fn lock_is_free(service: &TerminalService) -> bool {
    let service = service.clone();
    let (done, wait) = channel();
    std::thread::spawn(move || {
        service.foreground(TASK, "1");
        done.send(()).ok();
    });
    wait.recv_timeout(Duration::from_secs(1)).is_ok()
}

fn setup(log: &Log) -> TerminalService {
    let shared: Arc<OnceLock<TerminalService>> = Arc::default();
    let (seen, cell) = (Arc::clone(log), Arc::clone(&shared));
    let read: TableReader = Arc::new(move |_fresh| {
        let free = cell.get().is_none_or(lock_is_free);
        let read = if free { "ps" } else { "ps under a lock" };
        seen.lock().unwrap().push(read.to_string());
        std::thread::sleep(SLOW_PS);
        Arc::new(ProcessTable::default())
    });
    let log = Arc::clone(log);
    let spawn = move |_: SpawnOptions| -> std::io::Result<Box<dyn Pty>> {
        let (log, read) = (Arc::clone(&log), Arc::clone(&read));
        Ok(Box::new(PsPty { log, read }))
    };
    let service = TerminalService::new(TerminalOptions {
        spawn: Arc::new(spawn),
        workspace_of: Arc::new(|task| Ok(format!("/work/{task}").into())),
        env: [("PATH".to_string(), "/bin".to_string())]
            .into_iter()
            .collect(),
        scheduler: Scheduler::real(),
        on_output: None,
        is_ended: None,
        bus: None,
        on_output_idle: None,
        on_exit: None,
        trace_input: false,
    });
    shared.set(service.clone()).ok();
    service
}

fn open(service: &TerminalService) {
    let client: ClientRef = Arc::new(Nobody);
    let size = Size { cols: 80, rows: 24 };
    service
        .attach(TASK, client, size, "1", AttachOptions::default())
        .unwrap()
        .detach();
}

fn wait_for(log: &Log, count: usize) {
    let start = Instant::now();
    while log.lock().unwrap().len() < count && start.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn the_commands_of_a_shell_read_ps_with_its_lock_free() {
    let log: Log = Arc::default();
    let service = setup(&log);
    open(&service);
    assert_eq!(service.commands(TASK, "1"), Some(Vec::new()));
    assert_eq!(*log.lock().unwrap(), ["ps", "commands"]);
}

#[test]
fn closing_a_shell_reads_ps_off_the_callers_turn() {
    let log: Log = Arc::default();
    let service = setup(&log);
    open(&service);
    let start = Instant::now();
    service.close_all();
    assert!(start.elapsed() < SLOW_PS, "close waited for ps");
    std::thread::sleep(Duration::from_millis(KILL_GRACE_MS));
    wait_for(&log, 4);
    // The SIGHUP and the SIGKILL each get a table read just before them.
    assert_eq!(*log.lock().unwrap(), ["ps", "Hup", "ps", "Kill"]);
}
