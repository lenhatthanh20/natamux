//! Opening a terminal: the restored screen after a restart, the save at shutdown, and the first
//! output of a real login shell.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use natamux_daemon::shared::scheduler::Scheduler;
use natamux_daemon::terminals::service::{AttachOptions, TerminalOptions, TerminalService};
use natamux_daemon::terminals::shell_files::{OpenShell, save_open_shells};
use natamux_daemon::terminals::types::{ClientRef, Frame, TerminalClient};
use natamux_proto::terminals::history::CATCH_UP_HISTORY_ROWS;
use natamux_proto::terminals::messages::ServerMessage;
use natamux_pty::{Env, Pty, PtyEvents, Signal, SpawnOptions, spawn_shell};
use natamux_screen::Screen;

use super::{BenchPty, COLS, ROWS, Sink, Sinks, TASK, emit, history, ms, open, size};

const RUNS: usize = 10;
const SAVED_SHELLS: usize = 30;
const SHELL_RUNS: usize = 5;
/// Generous: a restored screen is one file read and a prefill.
const RESTORE_MAX_MS: f64 = 50.0;
const SAVE_MAX_MS: f64 = 1000.0;

pub struct Opens {
    pub restore_ms: f64,
    pub save_ms: f64,
    /// None when the shell did not start here.
    pub first_output_ms: Option<f64>,
}

impl Opens {
    pub fn first_output_text(&self) -> String {
        self.first_output_ms
            .map_or_else(|| "skipped".to_string(), |value| format!("{value} ms"))
    }
}

pub fn measure() -> Opens {
    let restore_ms = restore_snapshot();
    let save_ms = screen_save();
    assert!(
        restore_ms <= RESTORE_MAX_MS,
        "restore {restore_ms} ms is over {RESTORE_MAX_MS} ms"
    );
    assert!(
        save_ms <= SAVE_MAX_MS,
        "save of {SAVED_SHELLS} screens {save_ms} ms is over {SAVE_MAX_MS} ms"
    );
    Opens {
        restore_ms,
        save_ms,
        first_output_ms: shell_first_output(),
    }
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos());
        let path = std::env::temp_dir().join(format!("natamux-bench-{name}-{nanos}"));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Like the bench's `setup`, with the screens read from and saved to `data_dir`.
fn service_in(data_dir: &Path) -> (TerminalService, Sinks) {
    let sinks: Sinks = Arc::default();
    let shared = Arc::clone(&sinks);
    let spawn = move |_: SpawnOptions| -> std::io::Result<Box<dyn Pty>> {
        let sink: Sink = Arc::default();
        shared.lock().unwrap().push(Arc::clone(&sink));
        Ok(Box::new(BenchPty(sink)))
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
        screens_in: Some(data_dir.to_path_buf()),
    });
    (service, sinks)
}

fn list_open(data_dir: &Path, names: &[String]) {
    let shells: Vec<OpenShell> = names
        .iter()
        .map(|name| OpenShell {
            name: name.clone(),
            agent: None,
            session: None,
            paused: false,
        })
        .collect();
    save_open_shells(data_dir, TASK, &shells).unwrap();
}

/// A screen as the daemon saves it: a full one with the newest history rows.
fn saved_screen() -> Vec<u8> {
    let mut screen = Screen::new(COLS, ROWS);
    screen.write(&history());
    screen.snapshot_tail(CATCH_UP_HISTORY_ROWS)
}

/// Tells when the snapshot bytes of the attach come.
struct FirstSnapshot {
    snapshot: AtomicBool,
    got: Mutex<Sender<Instant>>,
}

impl TerminalClient for FirstSnapshot {
    fn send(&self, frame: Frame) {
        match frame {
            Frame::Message(ServerMessage::Snapshot { .. }) => {
                self.snapshot.store(true, Ordering::SeqCst);
            }
            Frame::Bytes(_) if self.snapshot.swap(false, Ordering::SeqCst) => {
                self.got.lock().unwrap().send(Instant::now()).ok();
            }
            _ => {}
        }
    }
}

/// p50 in ms from `attach` to the first snapshot bytes of a shell that has a saved screen.
fn restore_snapshot() -> f64 {
    let screen = saved_screen();
    let mut times = Vec::with_capacity(RUNS);
    for _ in 0..RUNS {
        let dir = TempDir::new("restore");
        list_open(&dir.0, &["1".to_string()]);
        natamux_daemon::terminals::screen_files::save_screen(&dir.0, TASK, "1", &screen).unwrap();
        let (service, _sinks) = service_in(&dir.0);
        let (got, arrived) = channel();
        let client: ClientRef = Arc::new(FirstSnapshot {
            snapshot: AtomicBool::new(false),
            got: Mutex::new(got),
        });
        let start = Instant::now();
        let detach = service
            .attach(TASK, client, size(), "1", AttachOptions::default())
            .unwrap();
        let at = arrived
            .recv_timeout(Duration::from_secs(2))
            .expect("no snapshot frame");
        times.push(ms(at.duration_since(start)));
        drop(detach);
        service.close_all();
    }
    median(times)
}

/// ms of `save_screens` with 30 shells that each hold a full screen and printed since.
fn screen_save() -> f64 {
    let dir = TempDir::new("save");
    let (service, sinks) = service_in(&dir.0);
    let names: Vec<String> = (1..=SAVED_SHELLS).map(|index| index.to_string()).collect();
    for name in &names {
        open(&service, name);
    }
    list_open(&dir.0, &names);
    let full = history();
    for sink in sinks.lock().unwrap().iter() {
        for piece in full.chunks(64 * 1024) {
            emit(sink, piece);
        }
        emit(sink, b"new output\r\n");
    }
    for name in &names {
        service.settled(TASK, name);
    }
    let start = Instant::now();
    service.save_screens();
    let took = start.elapsed();
    service.close_all();
    ms(took)
}

/// Median ms from the spawn of the user's login shell to its first byte. It depends on the
/// shell and its startup files, so it is only reported.
fn shell_first_output() -> Option<f64> {
    if cfg!(windows) {
        eprintln!("shell_first_output_ms: skipped on Windows");
        return None;
    }
    let dir = TempDir::new("shell");
    let env: Env = ["PATH", "HOME", "SHELL", "USER", "LANG"]
        .into_iter()
        .filter_map(|key| Some((key.to_string(), std::env::var(key).ok()?)))
        .chain([("TERM".to_string(), "xterm-256color".to_string())])
        .collect();
    let mut times = Vec::with_capacity(SHELL_RUNS);
    for _ in 0..SHELL_RUNS {
        let options = SpawnOptions {
            cols: COLS,
            rows: ROWS,
            cwd: dir.0.clone(),
            env: env.clone(),
        };
        let start = Instant::now();
        let mut pty = match spawn_shell(options) {
            Ok(pty) => pty,
            Err(error) => {
                eprintln!("shell_first_output_ms: skipped, spawn failed: {error}");
                return None;
            }
        };
        let (first, heard) = channel();
        pty.start(PtyEvents {
            on_data: Box::new(move |_| {
                first.send(Instant::now()).ok();
            }),
            on_exit: Box::new(|_| {}),
        });
        let heard_at = heard.recv_timeout(Duration::from_secs(5));
        pty.kill(Signal::Kill);
        let Ok(at) = heard_at else {
            eprintln!("shell_first_output_ms: skipped, no output in 5 s");
            return None;
        };
        times.push(ms(at.duration_since(start)));
    }
    Some(median(times))
}

fn median(mut times: Vec<f64>) -> f64 {
    times.sort_by(f64::total_cmp);
    times[times.len() / 2]
}
