//! A child process with a timeout and an output limit, on std only. Node's `execFile` with
//! `timeout` and `maxBuffer`.

use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

// The end of a process is seen when its pipes close, at once. This slice only bounds how late a
// timeout, an output limit or a grandchild that keeps the pipes open is noticed.
const SLICE: Duration = Duration::from_millis(20);
// How long to wait for the pipes after a kill: a grandchild may still hold them.
const PIPE_GRACE: Duration = Duration::from_millis(500);

pub struct Captured {
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

/// `Captured` with the stdout as it came, for output that is not text (`git show` of an image).
pub struct CapturedBytes {
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: String,
    pub timed_out: bool,
    pub overflow: bool,
    pub not_found: bool,
}

impl CapturedBytes {
    fn failed(not_found: bool) -> CapturedBytes {
        CapturedBytes {
            code: None,
            stdout: Vec::new(),
            stderr: String::new(),
            timed_out: false,
            overflow: false,
            not_found,
        }
    }
}

impl Captured {
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
    done: Sender<()>,
) {
    thread::spawn(move || {
        let mut chunk = [0u8; 16 * 1024];
        while let Ok(read) = pipe.read(&mut chunk) {
            if read == 0 {
                break;
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
                break;
            }
        }
        let _ = done.send(());
    });
}

fn bytes_of(buffer: &Buffer) -> Vec<u8> {
    buffer.lock().map(|held| held.clone()).unwrap_or_default()
}

/// Runs `command` (program, args, cwd and env already set) to its end, the timeout or the limit.
pub fn capture(command: Command, timeout: Duration, limit: usize) -> Captured {
    let ran = capture_bytes(command, timeout, limit);
    Captured {
        code: ran.code,
        stdout: String::from_utf8_lossy(&ran.stdout).into_owned(),
        stderr: ran.stderr,
        timed_out: ran.timed_out,
        overflow: ran.overflow,
        not_found: ran.not_found,
    }
}

/// `capture` with the stdout as bytes.
pub fn capture_bytes(mut command: Command, timeout: Duration, limit: usize) -> CapturedBytes {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => return CapturedBytes::failed(error.kind() == std::io::ErrorKind::NotFound),
    };
    let (out, err) = (Buffer::default(), Buffer::default());
    let total = Arc::new(AtomicUsize::new(0));
    let overflow = Arc::new(AtomicBool::new(false));
    let (done, finished) = mpsc::channel();
    let mut open = 0;
    if let Some(pipe) = child.stdout.take() {
        read_pipe(
            pipe,
            out.clone(),
            total.clone(),
            overflow.clone(),
            limit,
            done.clone(),
        );
        open += 1;
    }
    if let Some(pipe) = child.stderr.take() {
        read_pipe(
            pipe,
            err.clone(),
            total.clone(),
            overflow.clone(),
            limit,
            done,
        );
        open += 1;
    }
    let start = Instant::now();
    let mut timed_out = false;
    let mut killed = false;
    let status = loop {
        if open > 0 {
            match finished.recv_timeout(SLICE) {
                Ok(()) => open -= 1,
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => open = 0,
            }
        }
        if overflow.load(Ordering::SeqCst) {
            killed = true;
        } else if open == 0 {
            break child.wait().ok();
        } else if let Some(ended) = ended(&mut child) {
            // It ended but a grandchild still holds a pipe: take what came within the grace.
            wait_for_pipes(&finished, open, PIPE_GRACE);
            break ended;
        } else if start.elapsed() >= timeout {
            timed_out = true;
            killed = true;
        }
        if killed {
            let _ = child.kill();
            let status = child.wait().ok();
            wait_for_pipes(&finished, open, PIPE_GRACE);
            break status;
        }
    };
    CapturedBytes {
        code: status.and_then(|status| status.code()),
        stdout: bytes_of(&out),
        stderr: String::from_utf8_lossy(&bytes_of(&err)).into_owned(),
        timed_out,
        overflow: overflow.load(Ordering::SeqCst),
        not_found: false,
    }
}

/// `Some` once the child ended (`Some(None)` when its state cannot be read, as before).
fn ended(child: &mut std::process::Child) -> Option<Option<std::process::ExitStatus>> {
    match child.try_wait() {
        Ok(Some(status)) => Some(Some(status)),
        Ok(None) => None,
        Err(_) => Some(None),
    }
}

/// Waits until `open` pipe readers are done, or `grace` passed.
fn wait_for_pipes(finished: &mpsc::Receiver<()>, mut open: usize, grace: Duration) {
    let until = Instant::now() + grace;
    while open > 0 {
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() || finished.recv_timeout(left).is_err() {
            return;
        }
        open -= 1;
    }
}
