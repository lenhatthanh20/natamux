use std::sync::mpsc::channel;
use std::time::Duration;

use natamux_pty::{Pty, PtyEvents, Signal, SpawnOptions};

use super::*;
use crate::terminals::session_lifecycle::random_id;
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
    service_in(None)
}

fn service_in(screens_in: Option<std::path::PathBuf>) -> TerminalService {
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
        screens_in,
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

struct TempDir(std::path::PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp_dir() -> TempDir {
    let name = format!("natamux-screens-{}", random_id(8));
    let path = std::env::temp_dir().join(name);
    std::fs::create_dir_all(&path).unwrap();
    TempDir(path)
}

fn attach_one(service: &TerminalService, terminal: &str) -> Detach {
    let client: ClientRef = Arc::new(Nobody);
    let size = Size { cols: 80, rows: 24 };
    let options = AttachOptions::default();
    service
        .attach("T-0001", client, size, terminal, options)
        .unwrap()
}

#[test]
fn a_listed_shell_starts_with_its_saved_screen_and_the_restored_line() {
    use crate::terminals::screen_files::save_screen;
    use crate::terminals::shell_files::{OpenShell, save_open_shells};

    let dir = temp_dir();
    let listed = OpenShell {
        name: "1".into(),
        agent: None,
        session: None,
        paused: false,
    };
    save_open_shells(&dir.0, "T-0001", &[listed]).unwrap();
    save_screen(&dir.0, "T-0001", "1", b"old agent answer\r\n").unwrap();
    save_screen(&dir.0, "T-0001", "2", b"closed shell\r\n").unwrap();
    let service = service_in(Some(dir.0.clone()));
    let _one = attach_one(&service, "1");
    let _two = attach_one(&service, "2");

    let one = service.last_screen("T-0001", "1", 24).unwrap();
    assert!(one.contains("old agent answer"), "{one}");
    assert!(one.contains("Restored after restart"), "{one}");
    // Not in the open-shells list: a stale file is never shown.
    let two = service.last_screen("T-0001", "2", 24).unwrap();
    assert!(!two.contains("closed shell"), "{two}");
}

#[test]
fn a_quiet_moment_saves_the_screen_of_a_shell_that_printed() {
    use crate::terminals::screen_files::screens_dir;

    let dir = temp_dir();
    let service = service_in(Some(dir.0.clone()));
    let _one = attach_one(&service, "1");
    let session = service.inner.get("T-0001", "1").unwrap();
    {
        let mut guard = lock(&session);
        guard.screen.write(b"build done\r\n");
        guard.offset += 12;
    }
    service.inner.save_screens("T-0001", true);

    let file = screens_dir(&dir.0, "T-0001").join("1.ans");
    let saved = std::fs::read(&file).unwrap();
    assert!(String::from_utf8_lossy(&saved).contains("build done"));
    // Nothing printed since: no second write.
    std::fs::remove_file(&file).unwrap();
    service.inner.save_screens("T-0001", true);
    assert!(!file.exists());
}

#[test]
fn a_busy_shell_is_saved_at_most_once_in_a_while() {
    use crate::terminals::screen_files::screens_dir;

    let dir = temp_dir();
    let service = service_in(Some(dir.0.clone()));
    let _one = attach_one(&service, "1");
    let session = service.inner.get("T-0001", "1").unwrap();
    let print = |text: &[u8]| {
        let mut guard = lock(&session);
        guard.screen.write(text);
        guard.offset += text.len() as u64;
    };
    let file = screens_dir(&dir.0, "T-0001").join("1.ans");

    print(b"first\r\n");
    service.inner.save_screens("T-0001", false);
    assert!(file.exists());
    std::fs::remove_file(&file).unwrap();
    // Still printing, saved a moment ago: skipped.
    print(b"second\r\n");
    service.inner.save_screens("T-0001", false);
    assert!(!file.exists());
    // The quiet moment and the shutdown skip the wait.
    service.inner.save_screens("T-0001", true);
    assert!(file.exists());
    std::fs::remove_file(&file).unwrap();
    print(b"third\r\n");
    service.inner.save_all_screens();
    assert!(file.exists());
}
