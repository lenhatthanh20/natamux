//! A child process with a timeout and an output limit, on std only. Node's `execFile` with
//! `timeout` and `maxBuffer`.

use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const POLL: Duration = Duration::from_millis(20);
// How long to wait for the pipes after a kill: a grandchild may still hold them.
const PIPE_GRACE: Duration = Duration::from_millis(500);

pub(crate) struct Captured {
    /// None when the process was killed by a signal or did not start.
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    /// The timeout ended it.
    pub timed_out: bool,
    /// More output than the limit: the child is killed, as Node's maxBuffer does.
    pub overflow: bool,
    /// The program was not found.
    pub not_found: bool,
}

impl Captured {
    fn failed(not_found: bool) -> Captured {
        Captured {
            code: None,
            stdout: String::new(),
            stderr: String::new(),
            timed_out: false,
            overflow: false,
            not_found,
        }
    }

    /// Node's `error.killed`: a kill by the timeout or by the output limit.
    pub fn killed(&self) -> bool {
        self.timed_out || self.overflow
    }
}

type Buffer = Arc<Mutex<Vec<u8>>>;

fn read_pipe<R: Read + Send + 'static>(
    mut pipe: R,
    buffer: Buffer,
    total: Arc<AtomicUsize>,
    overflow: Arc<AtomicBool>,
    limit: usize,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut chunk = [0u8; 16 * 1024];
        while let Ok(read) = pipe.read(&mut chunk) {
            if read == 0 {
                return;
            }
            let seen = total.fetch_add(read, Ordering::SeqCst) + read;
            let keep = if seen > limit {
                overflow.store(true, Ordering::SeqCst);
                read.saturating_sub(seen - limit)
            } else {
                read
            };
            if let Ok(mut held) = buffer.lock() {
                held.extend_from_slice(&chunk[..keep]);
            }
            if seen > limit {
                return;
            }
        }
    })
}

fn text_of(buffer: &Buffer) -> String {
    buffer
        .lock()
        .map(|held| String::from_utf8_lossy(&held).into_owned())
        .unwrap_or_default()
}

/// Runs `command` (program, args, cwd and env already set) to its end, the timeout or the limit.
pub(crate) fn capture(mut command: Command, timeout: Duration, limit: usize) -> Captured {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => return Captured::failed(error.kind() == std::io::ErrorKind::NotFound),
    };
    let (out, err) = (Buffer::default(), Buffer::default());
    let total = Arc::new(AtomicUsize::new(0));
    let overflow = Arc::new(AtomicBool::new(false));
    let mut readers = Vec::new();
    if let Some(pipe) = child.stdout.take() {
        readers.push(read_pipe(
            pipe,
            out.clone(),
            total.clone(),
            overflow.clone(),
            limit,
        ));
    }
    if let Some(pipe) = child.stderr.take() {
        readers.push(read_pipe(
            pipe,
            err.clone(),
            total.clone(),
            overflow.clone(),
            limit,
        ));
    }
    let start = Instant::now();
    let mut timed_out = false;
    let mut killed = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {}
            Err(_) => break None,
        }
        if overflow.load(Ordering::SeqCst) {
            killed = true;
        } else if start.elapsed() >= timeout {
            timed_out = true;
            killed = true;
        }
        if killed {
            let _ = child.kill();
            break child.wait().ok();
        }
        thread::sleep(POLL);
    };
    // After a kill, a grandchild may keep the pipes open: wait a little, then take what came.
    let wait_until = Instant::now() + if killed { PIPE_GRACE } else { timeout };
    while readers.iter().any(|reader| !reader.is_finished()) && Instant::now() < wait_until {
        thread::sleep(Duration::from_millis(5));
    }
    Captured {
        code: status.and_then(|status| status.code()),
        stdout: text_of(&out),
        stderr: text_of(&err),
        timed_out,
        overflow: overflow.load(Ordering::SeqCst),
        not_found: false,
    }
}
