//! The Windows pseudo terminal: ConPTY through `alacritty_terminal`. One thread owns it, reads its
//! output, writes the input and resizes it, and tells when the shell ended.

use std::collections::HashMap;
use std::io::{self, ErrorKind, Read, Write};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;
use std::time::Duration;

use alacritty_terminal::event::{OnResize, WindowSize};
use alacritty_terminal::tty::{self, ChildEvent, EventedPty, EventedReadWrite, Options, Shell};

use crate::da1::{DA1_ANSWER, Da1Filter};
use crate::pty::{Pty, PtyEvents, Signal, SpawnOptions};
use crate::shell_command::shell_command;

/// The ConPTY output pipe does not block, and waiting on it needs a poller. A short sleep when it
/// is empty keeps one shell cheap. shortcut: up to IDLE_POLL of extra delay on the first output
/// after a quiet moment; use alacritty's poller if typing feels slow on Windows.
const BUSY_POLL: Duration = Duration::from_millis(1);
const IDLE_POLL: Duration = Duration::from_millis(5);

enum Input {
    Write(Vec<u8>),
    Resize(u16, u16),
}

pub fn spawn(options: SpawnOptions) -> io::Result<Box<dyn Pty>> {
    let (program, args) = shell_command(&options.env, true, &|_| false);
    let config = Options {
        shell: Some(Shell::new(program, args)),
        working_directory: Some(options.cwd.clone()),
        drain_on_exit: true,
        env: options.env.clone().into_iter().collect::<HashMap<_, _>>(),
        escape_args: true,
    };
    let mut pty = tty::new(&config, size(options.cols, options.rows), 0)?;
    let pid = pty.child_watcher().pid().map(|pid| pid.get());
    let (input, inputs) = mpsc::channel();
    Ok(Box::new(WindowsPty {
        pty: Some((pty, inputs)),
        input,
        pid,
        paused: Arc::new(AtomicBool::new(false)),
    }))
}

fn size(cols: u16, rows: u16) -> WindowSize {
    WindowSize {
        num_lines: rows,
        num_cols: cols,
        cell_width: 1,
        cell_height: 1,
    }
}

struct WindowsPty {
    pty: Option<(tty::Pty, Receiver<Input>)>,
    input: Sender<Input>,
    pid: Option<u32>,
    paused: Arc<AtomicBool>,
}

impl Pty for WindowsPty {
    fn start(&mut self, events: PtyEvents) {
        let Some((pty, inputs)) = self.pty.take() else {
            return;
        };
        let paused = Arc::clone(&self.paused);
        thread::spawn(move || run(pty, inputs, events, &paused));
    }

    fn write(&mut self, data: &[u8]) {
        let _ = self.input.send(Input::Write(data.to_vec()));
    }

    fn resize(&mut self, cols: u16, rows: u16) {
        let _ = self.input.send(Input::Resize(cols, rows));
    }

    fn pause(&mut self) {
        self.paused.store(true, Ordering::SeqCst);
    }

    fn resume(&mut self) {
        self.paused.store(false, Ordering::SeqCst);
    }

    // Windows has no signals; the shell and what runs in it are ended outright.
    fn kill(&mut self, _signal: Signal) {
        if let Some(pid) = self.pid {
            let _ = Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
}

fn run(mut pty: tty::Pty, inputs: Receiver<Input>, events: PtyEvents, paused: &AtomicBool) {
    let PtyEvents {
        mut on_data,
        on_exit,
    } = events;
    let mut buffer = vec![0u8; 64 * 1024];
    let mut pending: Vec<u8> = Vec::new();
    let mut da1 = Da1Filter::default();
    let mut exited: Option<Option<i32>> = None;
    loop {
        let mut busy = false;
        loop {
            match inputs.try_recv() {
                Ok(Input::Write(data)) => pending.extend(data),
                Ok(Input::Resize(cols, rows)) => pty.on_resize(size(cols, rows)),
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        while !pending.is_empty() {
            match pty.writer().write(&pending) {
                Ok(0) => break,
                Ok(written) => {
                    pending.drain(..written);
                    busy = true;
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                Err(_) => {
                    pending.clear();
                }
            }
        }
        while !paused.load(Ordering::SeqCst) {
            match pty.reader().read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    let (data, answer) = da1.feed(&buffer[..read]);
                    if answer {
                        pending.extend_from_slice(DA1_ANSWER);
                    }
                    if !data.is_empty() {
                        on_data(data);
                    }
                    busy = true;
                }
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
        if exited.is_none() {
            if let Some(ChildEvent::Exited(status)) = pty.next_child_event() {
                exited = Some(status.and_then(|status| status.code()));
                // One more pass reads what the shell printed last.
                continue;
            }
        } else if !busy {
            break;
        }
        thread::sleep(if busy { BUSY_POLL } else { IDLE_POLL });
    }
    let held = da1.flush();
    if !held.is_empty() {
        on_data(held);
    }
    on_exit(exited.flatten());
}
