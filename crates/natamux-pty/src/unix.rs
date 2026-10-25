//! The Unix pseudo terminal: `openpty`, a new session with the pty as its controlling terminal,
//! one thread that reads, one that writes and one that waits for the shell to end.

use std::fs::File;
use std::io::{self, ErrorKind, Read, Write};
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use nix::fcntl::{FcntlArg, FdFlag, fcntl};
use nix::pty::{Winsize, openpty};
use nix::sys::signal::{self, Signal as UnixSignal};
use nix::sys::termios::{self, InputFlags, SetArg};
use nix::unistd::{Pid, setsid, tcgetpgrp};

use crate::process_table::{ProcessTable, TableReader};
use crate::process_tree::TreeKill;
use crate::pty::{Pty, PtyEvents, Signal, SpawnOptions};
use crate::shell_command::shell_command;

/// After the shell ended, how long the reader may take to hand over the last output before the
/// exit is told. A job that keeps the pty open must not hold the exit back.
const DRAIN_WAIT: Duration = Duration::from_millis(200);
/// Each agent pane asks for the commands of its shell on a timer, so one `ps` serves them all for
/// a moment.
const PROCESS_TABLE_TTL: Duration = Duration::from_secs(2);

pub fn spawn(options: SpawnOptions) -> io::Result<Box<dyn Pty>> {
    let (file, args) = shell_command(&options.env, false, &|path| {
        std::path::Path::new(path).exists()
    });
    let size = winsize(options.cols, options.rows);
    let pty = openpty(Some(&size), None).map_err(io::Error::from)?;
    let (master, slave) = (pty.master, pty.slave);
    // openpty does not set close-on-exec: without it every later shell, and what it runs, would
    // hold the terminals of the shells started before it, of any task.
    for fd in [&master, &slave] {
        fcntl(fd, FcntlArg::F_SETFD(FdFlag::FD_CLOEXEC)).map_err(io::Error::from)?;
    }
    // Typed text is UTF-8, so an erase removes a whole character.
    if let Ok(mut settings) = termios::tcgetattr(&master) {
        settings.input_flags.insert(InputFlags::IUTF8);
        let _ = termios::tcsetattr(&master, SetArg::TCSANOW, &settings);
    }
    let mut command = Command::new(&file);
    // As node-pty: the environment given, PWD set to the folder.
    command
        .args(&args)
        .env_clear()
        .envs(&options.env)
        .env("PWD", &options.cwd)
        .current_dir(&options.cwd)
        .stdin(Stdio::from(slave.try_clone()?))
        .stdout(Stdio::from(slave.try_clone()?))
        .stderr(Stdio::from(slave));
    // SAFETY: only async-signal-safe calls (setsid, ioctl) between fork and exec.
    unsafe {
        command.pre_exec(move || {
            setsid().map_err(io::Error::from)?;
            // The pty is stdin by now; it becomes the controlling terminal of the new session.
            if nix::libc::ioctl(0, nix::libc::TIOCSCTTY as _, 0) == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let child = command.spawn()?;
    let pid = child.id() as i32;
    let writer = start_writer(File::from(master.try_clone()?));
    Ok(Box::new(UnixPty {
        reader: Some(File::from(master.try_clone()?)),
        master,
        child: Some(child),
        pid,
        writer,
        paused: Arc::new((Mutex::new(false), Condvar::new())),
        tree: TreeKill::default(),
    }))
}

fn winsize(cols: u16, rows: u16) -> Winsize {
    Winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    }
}

struct UnixPty {
    master: OwnedFd,
    reader: Option<File>,
    child: Option<Child>,
    pid: i32,
    writer: Sender<Vec<u8>>,
    paused: Arc<(Mutex<bool>, Condvar)>,
    tree: TreeKill,
}

fn start_writer(mut file: File) -> Sender<Vec<u8>> {
    let (sender, receiver) = mpsc::channel::<Vec<u8>>();
    thread::spawn(move || {
        for data in receiver {
            if file.write_all(&data).is_err() {
                return;
            }
        }
    });
    sender
}

impl Pty for UnixPty {
    fn start(&mut self, events: PtyEvents) {
        let (Some(reader), Some(child)) = (self.reader.take(), self.child.take()) else {
            return;
        };
        let PtyEvents { on_data, on_exit } = events;
        let (eof, drained) = mpsc::channel();
        let paused = Arc::clone(&self.paused);
        thread::spawn(move || read_loop(reader, on_data, &paused, eof));
        thread::spawn(move || wait_loop(child, on_exit, drained));
    }

    fn write(&mut self, data: &[u8]) {
        let _ = self.writer.send(data.to_vec());
    }

    fn resize(&mut self, cols: u16, rows: u16) {
        let size = winsize(cols, rows);
        // SAFETY: TIOCSWINSZ reads one winsize from the pointer, which lives for the call.
        unsafe {
            nix::libc::ioctl(self.master.as_raw_fd(), nix::libc::TIOCSWINSZ, &size);
        }
    }

    fn pause(&mut self) {
        set_paused(&self.paused, true);
    }

    fn resume(&mut self) {
        set_paused(&self.paused, false);
    }

    fn kill(&mut self, signal: Signal) {
        self.kill_in(signal, &read_processes(true));
    }

    fn kill_in(&mut self, signal: Signal, table: &ProcessTable) {
        self.tree.kill(table, self.pid, signal, &mut |pid, signal| {
            send(pid, signal)
        });
        send(self.pid, signal);
    }

    fn leftover(&self) -> bool {
        self.leftover_in(&read_processes(true))
    }

    fn leftover_in(&self, table: &ProcessTable) -> bool {
        self.tree.leftover(table)
    }

    fn foreground(&self) -> Option<String> {
        let group = tcgetpgrp(&self.master).ok()?;
        process_name(group.as_raw())
    }

    /// A fresh table: a one-off caller wants what runs now. The daemon shares one through
    /// `processes`.
    fn commands(&self) -> Option<Vec<String>> {
        self.commands_in(&read_processes(true))
    }

    fn commands_in(&self, table: &ProcessTable) -> Option<Vec<String>> {
        Some(table.commands_under(self.pid))
    }

    fn processes(&self) -> Option<TableReader> {
        Some(Arc::new(read_processes))
    }
}

fn set_paused(paused: &(Mutex<bool>, Condvar), value: bool) {
    if let Ok(mut flag) = paused.0.lock() {
        *flag = value;
        paused.1.notify_all();
    }
}

/// The most one read of a pty takes.
const READ_BYTES: usize = 64 * 1024;

/// The bytes of a read. A flood fills the buffer: it goes as it is, with no copy, and a new
/// buffer takes its place. A small read is copied, so it does not keep 64 KiB alive.
fn take_read(buffer: &mut Vec<u8>, read: usize) -> Vec<u8> {
    if read * 2 < buffer.len() {
        return buffer[..read].to_vec();
    }
    let mut full = std::mem::replace(buffer, vec![0u8; READ_BYTES]);
    full.truncate(read);
    full
}

fn read_loop(
    mut reader: File,
    mut on_data: Box<dyn FnMut(Vec<u8>) + Send>,
    paused: &(Mutex<bool>, Condvar),
    eof: Sender<()>,
) {
    let mut buffer = vec![0u8; READ_BYTES];
    loop {
        if let Ok(flag) = paused.0.lock() {
            // Not held while reading, so a pause never waits for output.
            drop(paused.1.wait_while(flag, |paused| *paused));
        }
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => on_data(take_read(&mut buffer, read)),
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            // EIO once every process that had the pty open has ended.
            Err(_) => break,
        }
    }
    let _ = eof.send(());
}

fn wait_loop(
    mut child: Child,
    on_exit: Box<dyn FnOnce(Option<i32>) + Send>,
    drained: Receiver<()>,
) {
    let code = child.wait().ok().map(|status| status.code().unwrap_or(0));
    let _ = drained.recv_timeout(DRAIN_WAIT);
    on_exit(code);
}

fn send(pid: i32, signal: Signal) {
    let signal = match signal {
        Signal::Hup => UnixSignal::SIGHUP,
        Signal::Kill => UnixSignal::SIGKILL,
    };
    // It ended already.
    let _ = signal::kill(Pid::from_raw(pid), signal);
}

fn ps(args: &[&str]) -> Option<String> {
    let out = Command::new("ps")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The table every shell shares, and when it was read.
static PROCESS_TABLE: Mutex<Option<(Instant, Arc<ProcessTable>)>> = Mutex::new(None);

/// One `ps` at a time: the callers that come meanwhile take its table. `fresh`: a kill or a
/// leftover check needs the processes of now.
fn read_processes(fresh: bool) -> Arc<ProcessTable> {
    let Ok(mut cached) = PROCESS_TABLE.lock() else {
        return Arc::new(read_table());
    };
    if let Some((at, table)) = cached.as_ref()
        && !fresh
        && at.elapsed() < PROCESS_TABLE_TTL
    {
        return Arc::clone(table);
    }
    let table = Arc::new(read_table());
    *cached = Some((Instant::now(), Arc::clone(&table)));
    table
}

fn read_table() -> ProcessTable {
    let text = ps(&["-ww", "-A", "-o", "pid=,ppid=,stat=,args="]).unwrap_or_default();
    ProcessTable::parse(&text)
}

/// The name node-pty gives the foreground process: on Linux the first word of its command line,
/// on macOS its short name.
#[cfg(target_os = "linux")]
fn process_name(pid: i32) -> Option<String> {
    let line = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    let first = line.split(|byte| *byte == 0).next()?;
    (!first.is_empty()).then(|| String::from_utf8_lossy(first).into_owned())
}

#[cfg(target_os = "macos")]
fn process_name(pid: i32) -> Option<String> {
    let mut buffer = [0u8; 256];
    // SAFETY: proc_name writes at most `buffer.len()` bytes into the buffer.
    let length =
        unsafe { nix::libc::proc_name(pid, buffer.as_mut_ptr().cast(), buffer.len() as u32) };
    (length > 0).then(|| String::from_utf8_lossy(&buffer[..length as usize]).into_owned())
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn process_name(pid: i32) -> Option<String> {
    ps(&["-o", "comm=", "-p", &pid.to_string()]).map(|name| name.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::{READ_BYTES, take_read};

    #[test]
    fn a_full_read_goes_with_no_copy_and_a_small_one_is_copied() {
        let mut buffer = vec![7u8; READ_BYTES];
        let at = buffer.as_ptr();
        let full = take_read(&mut buffer, READ_BYTES - 100);
        assert_eq!(full.as_ptr(), at);
        assert_eq!(full.len(), READ_BYTES - 100);
        assert_eq!(buffer.len(), READ_BYTES);
        assert_ne!(buffer.as_ptr(), at);
        let small = take_read(&mut buffer, 10);
        assert_eq!(small, [0u8; 10]);
        assert!(small.capacity() < READ_BYTES);
        assert_eq!(buffer.len(), READ_BYTES);
    }
}
