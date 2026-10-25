//! Attaches from several threads at once, as sockets do (review of T-0153, finding 1). Node runs
//! on one thread, so these cases have no TypeScript twin.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::time::Duration;

use natamux_daemon::shared::scheduler::Scheduler;
use natamux_daemon::terminals::service::{AttachOptions, TerminalOptions, TerminalService};
use natamux_daemon::terminals::types::ClientRef;
use natamux_pty::{Pty, PtyEvents, Signal, SpawnOptions};

use crate::support::{Recorder, SIZE, hello_id};

type Sink = Arc<Mutex<Vec<Box<dyn FnMut(Vec<u8>) + Send>>>>;

struct SlowPty(Sink);

impl Pty for SlowPty {
    fn start(&mut self, events: PtyEvents) {
        self.0.lock().unwrap().push(events.on_data);
    }
    fn write(&mut self, _data: &[u8]) {}
    fn resize(&mut self, _cols: u16, _rows: u16) {}
    fn pause(&mut self) {}
    fn resume(&mut self) {}
    fn kill(&mut self, _signal: Signal) {}
}

/// A service whose spawns take a while, so a second attach comes while the first one spawns.
fn slow_service() -> (TerminalService, Arc<AtomicUsize>, Sink) {
    let spawned = Arc::new(AtomicUsize::new(0));
    let sink: Sink = Arc::default();
    let (count, shared) = (Arc::clone(&spawned), Arc::clone(&sink));
    let spawn = move |_: SpawnOptions| -> std::io::Result<Box<dyn Pty>> {
        count.fetch_add(1, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(50));
        Ok(Box::new(SlowPty(Arc::clone(&shared))))
    };
    let service = TerminalService::new(TerminalOptions {
        spawn: Arc::new(spawn),
        workspace_of: Arc::new(|task| Ok(format!("/work/{task}").into())),
        env: Default::default(),
        scheduler: Scheduler::manual(),
        on_output: None,
        is_ended: None,
        bus: None,
        on_output_idle: None,
    });
    (service, spawned, sink)
}

#[test]
fn two_attaches_at_once_start_one_shell_and_share_it() {
    let (service, spawned, sink) = slow_service();
    let barrier = Arc::new(Barrier::new(2));
    let clients: Vec<Arc<Recorder>> = (0..2).map(|_| Arc::new(Recorder::default())).collect();
    let threads: Vec<_> = clients
        .iter()
        .map(|recorder| {
            let (service, barrier) = (service.clone(), Arc::clone(&barrier));
            let client: ClientRef = recorder.clone();
            std::thread::spawn(move || {
                barrier.wait();
                service.attach("T-0001", client, SIZE, "1", AttachOptions::default())
            })
        })
        .collect();
    let detaches: Vec<_> = threads
        .into_iter()
        .map(|thread| thread.join().unwrap().unwrap())
        .collect();

    assert_eq!(spawned.load(Ordering::SeqCst), 1);
    let ids: Vec<_> = clients
        .iter()
        .map(|client| hello_id(&client.controls()[0]))
        .collect();
    assert_eq!(ids[0], ids[1]);
    for on_data in sink.lock().unwrap().iter_mut() {
        on_data(b"shared".to_vec());
    }
    service.settled("T-0001", "1");
    for client in &clients {
        assert_eq!(client.output(), "shared");
    }
    drop(detaches);
}

#[test]
fn the_limit_holds_when_attaches_come_at_once() {
    let (service, spawned, _) = slow_service();
    let count = natamux_daemon::terminals::limits::MAX_SESSIONS + 5;
    let barrier = Arc::new(Barrier::new(count));
    let threads: Vec<_> = (0..count)
        .map(|index| {
            let (service, barrier) = (service.clone(), Arc::clone(&barrier));
            std::thread::spawn(move || {
                let client: ClientRef = Arc::new(Recorder::default());
                barrier.wait();
                let terminal = (index + 1).to_string();
                service
                    .attach("T-0001", client, SIZE, &terminal, AttachOptions::default())
                    .is_ok()
            })
        })
        .collect();
    let started = threads
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .filter(|ok| *ok)
        .count();
    assert_eq!(started, natamux_daemon::terminals::limits::MAX_SESSIONS);
    assert_eq!(spawned.load(Ordering::SeqCst), started);
    assert_eq!(service.list("T-0001").len(), started);
}
