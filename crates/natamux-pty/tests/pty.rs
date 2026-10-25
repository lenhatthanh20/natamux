//! Real shells in real pseudo terminals, on every OS CI runs.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use natamux_pty::{Env, Pty, PtyEvents, Signal, SpawnOptions, spawn_shell};

const WAIT: Duration = Duration::from_secs(15);

struct Shell {
    pty: Box<dyn Pty>,
    output: Arc<Mutex<Vec<u8>>>,
    exit: Receiver<Option<i32>>,
    home: PathBuf,
}

impl Drop for Shell {
    fn drop(&mut self) {
        self.pty.kill(Signal::Kill);
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

fn shell(name: &str, extra: &[(&str, &str)]) -> Shell {
    let home = std::env::temp_dir().join(format!("natamux-pty-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let mut env: Env = Env::new();
    if cfg!(windows) {
        for name in [
            "SystemRoot",
            "PATH",
            "TEMP",
            "TMP",
            "USERPROFILE",
            "COMSPEC",
        ] {
            if let Ok(value) = std::env::var(name) {
                env.insert(name.into(), value);
            }
        }
    } else {
        env.insert("PATH".into(), "/usr/bin:/bin".into());
        env.insert("SHELL".into(), "/bin/sh".into());
        env.insert("HOME".into(), home.display().to_string());
        env.insert("TERM".into(), "xterm-256color".into());
        env.insert("PS1".into(), "$ ".into());
    }
    for (name, value) in extra {
        env.insert(name.to_string(), value.to_string());
    }
    let options = SpawnOptions {
        cols: 80,
        rows: 24,
        cwd: home.clone(),
        env,
    };
    let mut pty = spawn_shell(options).unwrap();
    let output = Arc::new(Mutex::new(Vec::new()));
    let (exited, exit) = mpsc::channel();
    let sink = Arc::clone(&output);
    pty.start(PtyEvents {
        on_data: Box::new(move |data| sink.lock().unwrap().extend(data)),
        on_exit: Box::new(move |code| {
            let _ = exited.send(code);
        }),
    });
    Shell {
        pty,
        output,
        exit,
        home,
    }
}

impl Shell {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.output.lock().unwrap()).into_owned()
    }

    fn run(&mut self, line: &str) {
        let end = if cfg!(windows) { "\r" } else { "\n" };
        self.pty.write(format!("{line}{end}").as_bytes());
    }

    fn wait_for(&self, wanted: &str) -> String {
        let start = Instant::now();
        loop {
            let text = self.text();
            if text.contains(wanted) {
                return text;
            }
            assert!(start.elapsed() < WAIT, "no {wanted:?} in {text:?}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

#[test]
fn reads_bytes_from_the_pty_and_text_stays_utf8() {
    let mut shell = shell("utf8", &[("LANG", "C.UTF-8")]);
    if cfg!(windows) {
        shell.run("Write-Output ('Ti' + [char]0x1EBF + 'ng-ok')");
        shell.wait_for("Tiếng-ok");
    } else {
        shell.run("printf 'Ti\\341\\272\\277ng Vi\\341\\273\\207t\\n'");
        shell.wait_for("Tiếng Việt");
    }
}

#[test]
fn tells_the_exit_code_after_the_last_output() {
    let mut shell = shell("exit", &[]);
    shell.run("echo bye; exit 3");
    assert_eq!(shell.exit.recv_timeout(WAIT).unwrap(), Some(3));
    assert!(shell.text().contains("bye"));
}

#[cfg(unix)]
#[test]
fn the_shell_gets_exactly_the_environment_given() {
    let mut shell = shell("env", &[("NATAMUX_TERMINAL", "2")]);
    shell.run("env; echo END-ENV");
    let text = shell.wait_for("\nEND-ENV");
    assert!(text.contains("NATAMUX_TERMINAL=2"), "{text}");
    assert!(text.contains("TERM=xterm-256color"), "{text}");
    assert!(
        text.contains(&format!("PWD={}", shell.home.display())),
        "{text}"
    );
    for leak in ["WINDOWID", "ALACRITTY_WINDOW_ID", "CARGO_"] {
        assert!(!text.contains(leak), "{leak} leaked: {text}");
    }
}

/// Without close-on-exec on the pty, every later shell would hold the master and the slave of
/// the shells started before it. Linux names the targets of a process's fds in /proc.
#[cfg(target_os = "linux")]
#[test]
fn a_shell_holds_no_terminal_of_another_shell() {
    let _first = shell("fd-first", &[]);
    let mut second = shell("fd-second", &[]);
    // The marker is computed by the shell, so the echo of the typed line does not count.
    second.run("echo FDS $(readlink /proc/$$/fd/*) E$((1))ND");
    let text = second.wait_for("E1ND");
    let line = text
        .lines()
        .find(|line| line.contains("FDS ") && line.contains("E1ND"))
        .unwrap();
    let terminals: std::collections::BTreeSet<&str> = line
        .split_whitespace()
        .filter(|target| target.starts_with("/dev/pts/") || target.contains("ptmx"))
        .collect();
    assert_eq!(terminals.len(), 1, "{line}");
    assert!(!line.contains("ptmx"), "{line}");
}

#[cfg(unix)]
#[test]
fn a_resize_reaches_the_programs() {
    let mut shell = shell("resize", &[]);
    shell.pty.resize(100, 30);
    shell.run("stty size");
    shell.wait_for("30 100");
}

#[cfg(unix)]
#[test]
fn kills_a_program_that_ignored_sighup_after_the_shell_ended() {
    let mut shell = shell("kill", &[]);
    shell.run("sh -c 'trap \"\" HUP; sleep 300' & echo PID=$!");
    // The prompt may come before the output: find "PID=" followed by the number.
    let start = Instant::now();
    let pid: i32 = loop {
        let text = shell.text();
        let found = text.match_indices("PID=").find_map(|(at, _)| {
            let digits: String = text[at + 4..]
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            digits.parse().ok()
        });
        if let Some(pid) = found {
            break pid;
        }
        assert!(start.elapsed() < WAIT, "no pid in {text:?}");
        std::thread::sleep(Duration::from_millis(20));
    };
    std::thread::sleep(Duration::from_millis(300));

    shell.pty.kill(Signal::Hup);
    shell.exit.recv_timeout(WAIT).unwrap();
    assert!(
        shell.pty.leftover(),
        "the program that ignored SIGHUP still runs"
    );

    shell.pty.kill(Signal::Kill);
    let start = Instant::now();
    while shell.pty.leftover() {
        assert!(start.elapsed() < WAIT, "{pid} still runs after SIGKILL");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(unix)]
#[test]
fn names_the_program_in_the_foreground() {
    // bash, as dash runs a command in its own process group only with job control on.
    let mut shell = shell("foreground", &[("SHELL", "/bin/bash")]);
    shell.wait_for("$ ");
    let name = shell.pty.foreground().unwrap();
    assert_eq!(name.rsplit('/').next(), Some("bash"));
    shell.run("sleep 5");
    let start = Instant::now();
    let base =
        |name: Option<String>| name.map(|name| name.rsplit('/').next().unwrap_or("").to_string());
    while base(shell.pty.foreground()).as_deref() != Some("sleep") {
        assert!(
            start.elapsed() < WAIT,
            "{:?} {}",
            shell.pty.foreground(),
            shell.text()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    let commands = shell.pty.commands().unwrap();
    assert!(
        commands.iter().any(|line| line == "sleep 5"),
        "{commands:?}"
    );
}

// Regression: the daemon answered ConPTY's attributes query at spawn, and a dashboard that saw the
// query in the output answered it again, as typed text.
#[cfg(windows)]
#[test]
fn a_dashboard_that_answers_da1_adds_no_input() {
    let mut shell = shell("da1", &[]);
    let queries = |text: &str| text.matches("\x1b[c").count() + text.matches("\x1b[0c").count();
    let mut answered = 0;
    let start = Instant::now();
    // As xterm.js does: each attributes query in the output gets an answer.
    while start.elapsed() < Duration::from_secs(3) {
        while answered < queries(&shell.text()) {
            shell.pty.write(b"\x1b[?1;2c");
            answered += 1;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    shell.run("Write-Output ('da1' + '-ok')");
    let text = shell.wait_for("da1-ok\r");
    assert_eq!(answered, 0, "{text:?}");
    assert!(!text.contains("1;2c"), "{text:?}");
}
