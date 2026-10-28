//! Starts the daemon beside the packaged app and keeps it alive until the app quits.

use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use natamux_client::Config;
use natamux_client::config::save_access;

use crate::app::view_state;

const START_WAIT: Duration = Duration::from_secs(30);
const HEALTH_POLL: Duration = Duration::from_millis(20);

/// The daemon's stdin. Closing it stops the daemon; the login PATH thread writes to it too.
type ParentPipe = Arc<Mutex<Option<std::process::ChildStdin>>>;

pub struct OwnedDaemon {
    child: Arc<Mutex<Child>>,
    parent_pipe: ParentPipe,
    config: Config,
    port_file: PathBuf,
    port: u16,
}

/// Where a started value goes. Once closed (the app quits), a value whose start ends later is
/// given back to be stopped, not kept.
pub struct Slot<T> {
    value: Option<T>,
    closed: bool,
}

pub type DaemonSlot = Slot<OwnedDaemon>;

impl<T> Default for Slot<T> {
    fn default() -> Self {
        Slot {
            value: None,
            closed: false,
        }
    }
}

impl<T> Slot<T> {
    /// Keeps `value`, or gives it back when the app already quit.
    pub(super) fn put(&mut self, value: T) -> Option<T> {
        if self.closed {
            return Some(value);
        }
        self.value = Some(value);
        None
    }

    /// The app quits: the caller stops the value it gets, outside the lock.
    pub fn close(&mut self) -> Option<T> {
        self.closed = true;
        self.value.take()
    }

    pub(super) fn is_closed(&self) -> bool {
        self.closed
    }
}

impl OwnedDaemon {
    pub fn start() -> io::Result<Self> {
        let folder = view_state::folder().ok_or_else(|| io::Error::other("No app data folder"))?;
        let binary = sibling_daemon(&std::env::current_exe()?);
        let token = token_at(&folder.join("daemon-token"))?;
        let port_file = folder.join("daemon-port");
        if let Some(port) = fs::read_to_string(&port_file)
            .ok()
            .and_then(|text| text.trim().parse().ok())
            && probe_daemon(port, &token)
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Natamux already has a running daemon for this data folder",
            ));
        }
        let port = pick_port(4317)?;
        let log_path = folder.join("logs/daemon.log");
        fs::create_dir_all(
            log_path
                .parent()
                .ok_or_else(|| io::Error::other("No log folder"))?,
        )?;
        let log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)?;
        let mut command = Command::new(binary);
        command
            .env("NATAMUX_HOST", "127.0.0.1")
            .env("NATAMUX_PORT", port.to_string())
            .env("NATAMUX_TOKEN", &token)
            .env("NATAMUX_PARENT_PIPE", "1")
            .env("NATAMUX_DATA_DIR", folder.join("data"))
            .env(
                "NATAMUX_BIN_DIR",
                std::env::current_exe()?
                    .parent()
                    .ok_or_else(|| io::Error::other("No binary folder"))?,
            )
            .stdout(Stdio::from(log.try_clone()?))
            .stderr(Stdio::from(log))
            .stdin(Stdio::piped());
        #[cfg(unix)]
        command.env("NATAMUX_PATH_PENDING", "1");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        if let Some(repo) = std::env::var_os("NATAMUX_REPO_DIR") {
            command.env("NATAMUX_REPO_DIR", repo);
        }
        let mut child = command.spawn()?;
        let parent_pipe: ParentPipe = Arc::new(Mutex::new(child.stdin.take()));
        // A slow shell rc holds up neither the health polls nor a quit, which closes the pipe.
        #[cfg(unix)]
        let _ = send_path_later(Arc::clone(&parent_pipe), || {
            login_shell_path().unwrap_or_else(|| std::env::var("PATH").unwrap_or_default())
        });
        wait_for_healthy_child(&mut child, port, &token, &log_path, START_WAIT)?;
        if let Err(error) = save_access(&port_file, &port.to_string()) {
            let mut pipe = take_pipe(&parent_pipe);
            stop_child(&mut child, &mut pipe, Duration::from_secs(5));
            return Err(error);
        }
        Ok(Self {
            child: Arc::new(Mutex::new(child)),
            parent_pipe,
            config: Config {
                base_url: format!("http://127.0.0.1:{port}"),
                token,
            },
            port_file,
            port,
        })
    }

    pub fn config(&self) -> Config {
        self.config.clone()
    }

    pub fn child(&self) -> Arc<Mutex<Child>> {
        Arc::clone(&self.child)
    }
}

impl Drop for OwnedDaemon {
    fn drop(&mut self) {
        clear_port_file(&self.port_file, self.port);
        let Ok(mut child) = self.child.lock() else {
            return;
        };
        let mut pipe = take_pipe(&self.parent_pipe);
        stop_child(&mut child, &mut pipe, Duration::from_secs(5));
    }
}

fn take_pipe<W>(pipe: &Mutex<Option<W>>) -> Option<W> {
    pipe.lock().unwrap_or_else(PoisonError::into_inner).take()
}

/// Finds the PATH on its own thread and writes it as one JSON line, unless the pipe was closed
/// meanwhile (the app quit). The pipe is locked only for the write.
#[cfg(unix)]
pub(super) fn send_path_later<W: Write + Send + 'static>(
    pipe: Arc<Mutex<Option<W>>>,
    find_path: impl FnOnce() -> String + Send + 'static,
) -> io::Result<std::thread::JoinHandle<()>> {
    std::thread::Builder::new()
        .name("natamux-login-path".into())
        .spawn(move || {
            let path = find_path();
            let message = serde_json::json!({ "path": path }).to_string();
            let mut pipe = pipe.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(input) = pipe.as_mut() {
                let _ = writeln!(input, "{message}");
                let _ = input.flush();
            }
        })
}

pub(super) fn stop_child(
    child: &mut Child,
    pipe: &mut Option<std::process::ChildStdin>,
    wait: Duration,
) {
    // Closing stdin tells the owned daemon to finish its shells and flush its data.
    pipe.take();
    let until = Instant::now() + wait;
    loop {
        if child.try_wait().ok().flatten().is_some() {
            return;
        }
        if Instant::now() >= until {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    let _ = child.wait();
}

pub(super) fn clear_port_file(file: &Path, port: u16) {
    let owned = port.to_string();
    if fs::read_to_string(file).ok().as_deref().map(str::trim) == Some(owned.as_str()) {
        let _ = fs::remove_file(file);
    }
}

pub(super) fn sibling_daemon(app: &Path) -> PathBuf {
    let name = if app
        .extension()
        .is_some_and(|ext| ext == std::ffi::OsStr::new("exe"))
    {
        "natamux-daemon.exe"
    } else {
        "natamux-daemon"
    };
    app.with_file_name(name)
}

pub(super) fn pick_port(preferred: u16) -> io::Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", preferred))
        .or_else(|_| TcpListener::bind(("127.0.0.1", 0)))?;
    Ok(listener.local_addr()?.port())
}

pub(super) fn token_at(file: &Path) -> io::Result<String> {
    if let Ok(token) = fs::read_to_string(file) {
        let token = token.trim();
        if token.len() >= 32
            && token
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Ok(token.to_string());
        }
    }
    let mut bytes = [0_u8; 24];
    getrandom::fill(&mut bytes).map_err(|error| io::Error::other(error.to_string()))?;
    let token = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    save_access(file, &token)?;
    Ok(token)
}

pub(super) fn wait_for_healthy_child(
    child: &mut Child,
    port: u16,
    token: &str,
    log: &Path,
    wait: Duration,
) -> io::Result<()> {
    let result = wait_for_health(child, port, token, log, wait);
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    result
}

pub(super) fn wait_for_health(
    child: &mut Child,
    port: u16,
    token: &str,
    log: &Path,
    wait: Duration,
) -> io::Result<()> {
    let until = Instant::now() + wait;
    while Instant::now() < until {
        if let Some(status) = child.try_wait()? {
            return Err(startup_error(
                log,
                &format!("The daemon stopped while it started ({status})"),
            ));
        }
        if probe_daemon(port, token) {
            return Ok(());
        }
        std::thread::sleep(HEALTH_POLL);
    }
    Err(startup_error(
        log,
        "The daemon did not answer within 30 seconds",
    ))
}

pub(super) fn probe_daemon(port: u16, token: &str) -> bool {
    probe_daemon_within(port, token, Duration::from_millis(200))
}

// Tests pass a longer timeout: a loaded CI runner can start the test server after 200 ms.
pub(super) fn probe_daemon_within(port: u16, token: &str, timeout: Duration) -> bool {
    let Some(address) = ("127.0.0.1", port)
        .to_socket_addrs()
        .ok()
        .and_then(|mut addresses| addresses.next())
    else {
        return false;
    };
    let Ok(mut stream) = TcpStream::connect_timeout(&address, timeout) else {
        return false;
    };
    if stream.set_read_timeout(Some(timeout)).is_err() {
        return false;
    }
    let request = format!(
        "GET /api/tasks HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {token}\r\nConnection: close\r\n\r\n"
    );
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }
    let mut response = [0_u8; 12];
    stream.read_exact(&mut response).is_ok() && response == *b"HTTP/1.1 200"
}

pub(super) fn startup_error(log: &Path, reason: &str) -> io::Error {
    let detail = fs::read_to_string(log).ok().and_then(|text| {
        text.lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .map(str::to_string)
    });
    io::Error::other(match detail {
        Some(detail) => format!("{reason}. Last daemon log line: {detail}"),
        None => reason.to_string(),
    })
}

#[cfg(unix)]
fn login_shell_path() -> Option<String> {
    let shell = std::env::var_os("SHELL")?;
    let mut child = Command::new(shell)
        .args([
            "-ilc",
            "printf '__NATAMUX_PATH__%s__NATAMUX_PATH__' \"$PATH\"",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        if child.try_wait().ok().flatten().is_some() {
            let mut text = String::new();
            child.stdout.take()?.read_to_string(&mut text).ok()?;
            return text
                .split_once("__NATAMUX_PATH__")?
                .1
                .split_once("__NATAMUX_PATH__")
                .map(|(path, _)| path.trim().to_string())
                .filter(|path| !path.is_empty());
        }
        if Instant::now() >= until {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
