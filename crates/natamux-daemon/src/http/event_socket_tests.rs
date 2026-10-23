use super::*;

#[derive(Default)]
struct FakeSocket {
    buffered: AtomicUsize,
    closed: AtomicBool,
    sent: Mutex<Vec<String>>,
}

impl EventSocket for FakeSocket {
    fn buffered(&self) -> usize {
        self.buffered.load(Ordering::SeqCst)
    }

    fn is_open(&self) -> bool {
        !self.closed.load(Ordering::SeqCst)
    }

    fn send(&self, text: &str) {
        self.sent.lock().unwrap().push(text.to_string());
    }
}

fn setup() -> (Arc<FakeSocket>, EventSender, Arc<Scheduler>) {
    let socket = Arc::new(FakeSocket::default());
    let scheduler = Scheduler::manual();
    let sender = EventSender::new(socket.clone(), Arc::clone(&scheduler));
    (socket, sender, scheduler)
}

impl FakeSocket {
    fn set(&self, bytes: usize) {
        self.buffered.store(bytes, Ordering::SeqCst);
    }

    fn sent(&self) -> Vec<String> {
        self.sent.lock().unwrap().clone()
    }
}

#[test]
fn skips_a_socket_that_is_too_far_behind_and_sends_it_resync_once_it_drained() {
    let (socket, sender, scheduler) = setup();
    socket.set(EVENTS_HIGH_WATER);
    sender.send("a");
    socket.set(EVENTS_HIGH_WATER + 1);
    sender.send("b");
    socket.set(0);
    sender.send("c");
    assert_eq!(socket.sent(), ["a"]);

    scheduler.advance(DRAIN_CHECK_MS);
    sender.send("d");
    assert_eq!(socket.sent(), ["a", r#"{"kind":"resync"}"#, "d"]);
    scheduler.advance(DRAIN_CHECK_MS * 4);
    assert_eq!(socket.sent().len(), 3);
}

#[test]
fn sends_nothing_to_a_socket_that_closed_while_it_was_skipped() {
    let (socket, sender, scheduler) = setup();
    socket.set(EVENTS_HIGH_WATER + 1);
    sender.send("a");
    socket.closed.store(true, Ordering::SeqCst);
    socket.set(0);
    scheduler.advance(DRAIN_CHECK_MS * 4);
    sender.send("b");
    assert!(socket.sent().is_empty());
    assert!(!sender.is_checking());
}

#[test]
fn stops_looking_at_a_skipped_socket_when_its_dashboard_leaves() {
    let (socket, sender, scheduler) = setup();
    socket.set(EVENTS_HIGH_WATER + 1);
    sender.send("a");
    assert!(sender.is_checking());
    sender.stop();
    socket.set(0);
    scheduler.advance(DRAIN_CHECK_MS * 4);
    assert!(!sender.is_checking());
    assert!(socket.sent().is_empty());
}
