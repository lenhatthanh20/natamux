use super::health_proof::proof_of;
use super::process::{
    clear_port_file, pick_port, probe_daemon_within, sibling_daemon, startup_error, stop_child,
    token_at, wait_for_health, wait_for_healthy_child,
};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

fn child_waiting_for_stdin() -> Child {
    #[cfg(unix)]
    let mut command = {
        let mut command = Command::new("sh");
        command.args(["-c", "cat >/dev/null"]);
        command
    };
    #[cfg(windows)]
    let mut command = {
        let mut command = Command::new("cmd");
        command.args(["/C", "more >NUL"]);
        command
    };
    command
        .stdin(Stdio::piped())
        .spawn()
        .expect("waiting child")
}

fn child_exiting_with_error() -> Child {
    #[cfg(unix)]
    let mut command = {
        let mut command = Command::new("sh");
        command.args(["-c", "exit 23"]);
        command
    };
    #[cfg(windows)]
    let mut command = {
        let mut command = Command::new("cmd");
        command.args(["/C", "exit /B 23"]);
        command
    };
    command.spawn().expect("exiting child")
}

#[test]
fn finds_the_daemon_beside_the_app() {
    assert_eq!(
        sibling_daemon(Path::new(
            "/Applications/Natamux.app/Contents/MacOS/natamux-app"
        )),
        Path::new("/Applications/Natamux.app/Contents/MacOS/natamux-daemon")
    );
    assert_eq!(
        sibling_daemon(Path::new("C:/Natamux/natamux-app.exe")),
        Path::new("C:/Natamux/natamux-daemon.exe")
    );
}

#[test]
fn uses_another_port_when_the_preferred_one_is_busy() {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("listener");
    let occupied = listener.local_addr().expect("address").port();
    let chosen = pick_port(occupied).expect("free port");
    assert_ne!(chosen, occupied);
}

#[test]
fn keeps_the_daemon_token_between_starts() {
    let root = std::env::temp_dir().join(format!("natamux-app-token-test-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp folder");
    let path = root.join("daemon-token");
    let first = token_at(&path).expect("new token");
    let second = token_at(&path).expect("saved token");
    assert_eq!(first, second);
    assert!(first.len() >= 32);
    std::fs::remove_file(path).expect("remove token");
    std::fs::remove_dir(root).expect("remove temp folder");
}

#[test]
fn refuses_a_saved_token_that_could_inject_an_http_header() {
    let root =
        std::env::temp_dir().join(format!("natamux-app-token-header-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp folder");
    let path = root.join("daemon-token");
    std::fs::write(&path, "a".repeat(32) + "\r\nX-Forged: yes").expect("bad token");
    let token = token_at(&path).expect("replacement token");
    assert!(!token.contains('\r'));
    assert!(!token.contains('\n'));
    assert_eq!(std::fs::read_to_string(&path).expect("saved"), token);
    std::fs::remove_file(path).expect("remove token");
    std::fs::remove_dir(root).expect("remove temp folder");
}

#[test]
fn startup_error_shows_the_last_daemon_log_line() {
    let root = std::env::temp_dir().join(format!("natamux-app-start-error-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp folder");
    let log = root.join("daemon.log");
    std::fs::write(&log, "starting\nport is already in use\n").expect("log");
    let error = startup_error(&log, "The daemon stopped");
    assert!(error.to_string().contains("port is already in use"));
    std::fs::remove_file(log).expect("remove log");
    std::fs::remove_dir(root).expect("remove temp folder");
}

#[test]
fn startup_timeout_stops_and_reaps_a_child_that_never_answers() {
    let mut child = child_waiting_for_stdin();
    let log = std::env::temp_dir().join("natamux-startup-timeout-unused.log");
    let error = wait_for_healthy_child(&mut child, 0, "token", &log, Duration::ZERO)
        .expect_err("startup must time out");
    assert!(error.to_string().contains("did not answer"));
    assert!(child.try_wait().expect("status").is_some());
}

#[test]
fn startup_exit_reports_the_status_and_last_log_line() {
    let root = std::env::temp_dir().join(format!("natamux-early-exit-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp folder");
    let log = root.join("daemon.log");
    std::fs::write(&log, "started\nbad configuration\n").expect("log");
    let mut child = child_exiting_with_error();
    assert_eq!(child.wait().expect("exit").code(), Some(23));
    let error = wait_for_health(&mut child, 0, "token", &log, Duration::from_secs(1))
        .expect_err("early exit must fail");
    assert!(error.to_string().contains("23"));
    assert!(error.to_string().contains("bad configuration"));
    std::fs::remove_file(log).expect("remove log");
    std::fs::remove_dir(root).expect("remove temp folder");
}

#[test]
fn closing_the_parent_pipe_lets_the_child_exit_cleanly() {
    let mut child = child_waiting_for_stdin();
    let mut pipe = child.stdin.take();
    stop_child(&mut child, &mut pipe, Duration::from_secs(5));
    assert!(pipe.is_none());
    assert!(
        child
            .try_wait()
            .expect("status")
            .expect("reaped child")
            .success()
    );
}

/// A fake listener for two probes. `reply` builds the answer from the request text. Returns the
/// requests it saw.
fn fake_daemon(
    reply: impl Fn(&str) -> String + Send + 'static,
) -> (u16, std::thread::JoinHandle<Vec<String>>) {
    use std::io::{Read, Write};

    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("listener");
    let port = listener.local_addr().expect("address").port();
    let server = std::thread::spawn(move || {
        let mut seen = Vec::new();
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().expect("connection");
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut chunk = [0_u8; 256];
                let size = stream.read(&mut chunk).expect("request");
                assert!(size > 0 && request.len() < 1024);
                request.extend_from_slice(&chunk[..size]);
            }
            let request = String::from_utf8_lossy(&request).into_owned();
            // The probe may give up on a slow runner and close first; its answer is still false.
            let _ = stream.write_all(reply(&request).as_bytes());
            seen.push(request);
        }
        seen
    });
    (port, server)
}

fn nonce_of(request: &str) -> String {
    let start = request.find("nonce=").expect("nonce") + "nonce=".len();
    request[start..start + 32].to_string()
}

fn ok_with(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

#[test]
fn a_listener_that_cannot_prove_itself_never_sees_the_token() {
    // Not the 200 ms of the app: a loaded runner can start the server thread later than that.
    let wait = Duration::from_secs(5);
    // One that says 200 to anything, and one that proves itself with another token.
    let calls = std::sync::atomic::AtomicUsize::new(0);
    let (port, server) = fake_daemon(move |request| {
        if calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
            ok_with(r#"{"status":"ok"}"#)
        } else {
            let proof = proof_of("other-token", &nonce_of(request));
            ok_with(&format!(r#"{{"status":"ok","proof":"{proof}"}}"#))
        }
    });
    assert!(!probe_daemon_within(port, "owned-token", wait));
    assert!(!probe_daemon_within(port, "owned-token", wait));
    for request in server.join().expect("server") {
        assert!(request.starts_with("GET /health?nonce="), "{request}");
        assert!(
            !request.to_lowercase().contains("authorization"),
            "{request}"
        );
        assert!(!request.contains("owned-token"), "{request}");
    }
}

#[test]
fn the_owned_daemon_passes_the_probe_without_the_token_on_the_wire() {
    let wait = Duration::from_secs(5);
    let (port, server) = fake_daemon(|request| {
        let proof = proof_of("owned-token", &nonce_of(request));
        ok_with(&format!(r#"{{"status":"ok","proof":"{proof}"}}"#))
    });
    assert!(probe_daemon_within(port, "owned-token", wait));
    assert!(!probe_daemon_within(port, "other-token", wait));
    for request in server.join().expect("server") {
        assert!(
            !request.to_lowercase().contains("authorization"),
            "{request}"
        );
        assert!(!request.contains("owned-token"), "{request}");
    }
}

#[cfg(unix)]
#[test]
fn a_wide_token_file_becomes_0600() {
    use std::os::unix::fs::PermissionsExt;
    let root = std::env::temp_dir().join(format!("natamux-app-token-mode-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp folder");
    let path = root.join("daemon-token");
    std::fs::write(&path, "a".repeat(40)).expect("old token");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("mode");
    assert_eq!(token_at(&path).expect("saved token"), "a".repeat(40));
    let mode = std::fs::metadata(&path).expect("meta").permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
    std::fs::remove_file(path).expect("remove token");
    std::fs::remove_dir(root).expect("remove temp folder");
}

#[test]
fn clears_only_the_port_owned_by_this_app() {
    let root = std::env::temp_dir().join(format!("natamux-port-test-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp folder");
    let file = root.join("daemon-port");
    std::fs::write(&file, "4318").expect("port");
    clear_port_file(&file, 4317);
    assert!(file.exists());
    clear_port_file(&file, 4318);
    assert!(!file.exists());
    std::fs::remove_dir(root).expect("remove temp folder");
}

#[cfg(unix)]
#[test]
fn the_path_line_comes_later_without_holding_up_the_start() {
    use super::process::send_path_later;
    use std::sync::{Arc, Mutex, mpsc};

    let pipe = Arc::new(Mutex::new(Some(Vec::<u8>::new())));
    let (release, gate) = mpsc::channel::<()>();
    let writer = send_path_later(Arc::clone(&pipe), move || {
        gate.recv().expect("released");
        "/opt/homebrew/bin:/usr/bin".to_owned()
    })
    .expect("thread");
    // The shell has not answered: the start goes on to its health polls meanwhile.
    assert_eq!(pipe.lock().expect("pipe").as_ref().map(Vec::len), Some(0));
    release.send(()).expect("release");
    writer.join().expect("writer");
    let written = pipe.lock().expect("pipe").take().expect("open pipe");
    assert_eq!(
        String::from_utf8(written).expect("text"),
        "{\"path\":\"/opt/homebrew/bin:/usr/bin\"}\n"
    );
}

#[cfg(unix)]
#[test]
fn a_pipe_closed_before_the_shell_answers_gets_no_path_line() {
    use super::process::send_path_later;
    use std::sync::{Arc, Mutex, mpsc};

    let pipe = Arc::new(Mutex::new(Some(Vec::<u8>::new())));
    let (release, gate) = mpsc::channel::<()>();
    let writer = send_path_later(Arc::clone(&pipe), move || {
        gate.recv().expect("released");
        "/usr/bin".to_owned()
    })
    .expect("thread");
    // The app quit while the shell still ran.
    let closed = pipe.lock().expect("pipe").take();
    release.send(()).expect("release");
    writer.join().expect("writer");
    assert_eq!(closed, Some(Vec::new()));
    assert!(pipe.lock().expect("pipe").is_none());
}
