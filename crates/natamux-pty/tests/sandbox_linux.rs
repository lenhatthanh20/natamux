//! T-0373 Part 7 (F1): Landlock on a real Linux kernel. These tests fail, never skip, when the
//! kernel has no Landlock: CI must prove the sandbox, not quietly pass without it.

#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use natamux_pty::sandbox_plan::{isolate_command, landlock_abi, own_tty, tcp_confined};
use natamux_pty::sandbox_sweep::holders;
use natamux_pty::{Env, IsolationPlan, PtyEvents, Signal, SpawnOptions, spawn_shell};

const WAIT: Duration = Duration::from_secs(15);

struct Dirs {
    root: PathBuf,
}

impl Dirs {
    fn new(name: &str) -> Dirs {
        let root =
            std::env::temp_dir().join(format!("natamux-sandbox-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for dir in ["a", "b", "home", "data", "tmp-a"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        std::fs::write(root.join("b/sentinel"), "B-SECRET").unwrap();
        std::fs::write(root.join("home/sentinel"), "HOME-SECRET").unwrap();
        std::fs::write(root.join("data/daemon-token"), "DAEMON-SECRET").unwrap();
        std::fs::write(root.join("data/security.json"), "{\"strict\":true}").unwrap();
        Dirs { root }
    }

    fn at(&self, path: &str) -> PathBuf {
        self.root.join(path)
    }

    /// As the daemon grants a task: the system read only, the worktree and tmp read-write.
    fn plan(&self) -> IsolationPlan {
        let system = [
            "/usr", "/bin", "/sbin", "/lib", "/lib64", "/etc", "/proc", "/dev",
        ];
        IsolationPlan {
            read_only: existing(&system),
            read_write: [
                existing(&["/dev/null", "/dev/tty", "/dev/pts", "/dev/ptmx"]),
                vec![self.at("a"), self.at("tmp-a")],
            ]
            .concat(),
            sweep_mark: None,
            max_abi: None,
            tcp_connect: None,
        }
    }
}

impl Drop for Dirs {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn existing(paths: &[&str]) -> Vec<PathBuf> {
    paths
        .iter()
        .map(PathBuf::from)
        .filter(|path| path.exists())
        .collect()
}

/// Runs a shell line under the plan; gives stdout and stderr together.
fn sandboxed(plan: &IsolationPlan, line: &str, cwd: &Path) -> String {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", line]).current_dir(cwd);
    isolate_command(&mut command, plan).unwrap();
    let out = command.output().unwrap();
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn the_kernel_has_landlock() {
    let abi = landlock_abi().unwrap();
    println!("Landlock ABI {abi}");
    assert!(abi >= 1);
}

#[test]
fn a_task_reads_and_writes_its_worktree_and_nothing_of_others() {
    let dirs = Dirs::new("reach");
    let plan = dirs.plan();
    let line = format!(
        "echo hi > own && cat own && echo t > '{}/x' && echo WROTE-OWN",
        dirs.at("tmp-a").display()
    );
    let out = sandboxed(&plan, &line, &dirs.at("a"));
    assert!(out.contains("hi") && out.contains("WROTE-OWN"), "{out}");
    for secret in [
        "b/sentinel",
        "home/sentinel",
        "data/daemon-token",
        "data/security.json",
    ] {
        let line = format!("cat '{}'", dirs.at(secret).display());
        let out = sandboxed(&plan, &line, &dirs.at("a"));
        assert!(out.contains("Permission denied"), "{secret}: {out}");
        assert!(
            !out.contains("SECRET") && !out.contains("strict"),
            "{secret}: {out}"
        );
    }
    for target in ["b/new", "home/new", "data/new"] {
        let line = format!("echo x > '{}'", dirs.at(target).display());
        let out = sandboxed(&plan, &line, &dirs.at("a"));
        assert!(out.contains("Permission denied"), "{target}: {out}");
        assert!(!dirs.at(target).exists(), "{target} was written");
    }
    let tmp = std::env::temp_dir().join(format!("natamux-sandbox-tmp-{}", std::process::id()));
    let out = sandboxed(
        &plan,
        &format!("echo x > '{}'", tmp.display()),
        &dirs.at("a"),
    );
    assert!(out.contains("Permission denied") && !tmp.exists(), "{out}");
}

/// T-0373 Part 8 (F3): under a closed egress a sandboxed shell connects only to the daemon port.
/// Landlock checks TCP ports from ABI 4 on; below that `GET /security` says not confined.
#[test]
fn tcp_reaches_only_the_allowed_port() {
    let abi = landlock_abi().unwrap();
    let dirs = Dirs::new("tcp");
    let daemon = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let other = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let (allowed, refused) = (
        daemon.local_addr().unwrap().port(),
        other.local_addr().unwrap().port(),
    );
    let mut plan = dirs.plan();
    plan.tcp_connect = Some(vec![allowed]);
    if !tcp_confined(&plan, abi) {
        println!("Landlock ABI {abi} < 4: TCP is not confined, so this kernel cannot prove it");
        return;
    }
    let connect = |plan: &IsolationPlan, host: &str, port: u16| {
        let line = format!(
            "bash -c 'exec 3<>/dev/tcp/{host}/{port}' 2>/dev/null && echo REACHED || echo REFUSED"
        );
        sandboxed(plan, &line, &dirs.at("a"))
    };
    assert!(connect(&plan, "127.0.0.1", allowed).contains("REACHED"));
    assert!(connect(&plan, "127.0.0.1", refused).contains("REFUSED"));
    // A non-loopback test address (TEST-NET-1): refused by Landlock before any packet.
    assert!(connect(&plan, "192.0.2.1", 80).contains("REFUSED"));
    plan.tcp_connect = None;
    assert!(connect(&plan, "127.0.0.1", refused).contains("REACHED"));
}

#[test]
fn abi_zero_or_a_bad_path_fails_before_the_shell_runs() {
    let dirs = Dirs::new("fail");
    let marker = dirs.at("a/marker");
    let mut abi_zero = dirs.plan();
    abi_zero.max_abi = Some(0);
    let mut bad_path = dirs.plan();
    bad_path.read_only.push(dirs.at("gone"));
    for plan in [abi_zero, bad_path] {
        let mut env = Env::new();
        env.insert("PATH".into(), "/usr/bin:/bin".into());
        env.insert("SHELL".into(), "/bin/sh".into());
        env.insert("ENV".into(), "/dev/null".into());
        let options = SpawnOptions {
            cols: 80,
            rows: 24,
            cwd: dirs.at("a"),
            env,
            isolation: Some(plan.clone()),
        };
        assert!(spawn_shell(options).is_err());
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", "touch marker"])
            .current_dir(dirs.at("a"));
        assert!(isolate_command(&mut command, &plan).is_err());
        assert!(!marker.exists());
    }
}

struct Shell {
    pty: Box<dyn natamux_pty::Pty>,
    output: Arc<Mutex<Vec<u8>>>,
    exit: mpsc::Receiver<Option<i32>>,
}

impl Shell {
    fn new(dirs: &Dirs, plan: IsolationPlan, mark: &str) -> Shell {
        let mut env = Env::new();
        for (name, value) in [
            ("PATH", "/usr/bin:/bin"),
            ("SHELL", "/bin/sh"),
            ("TERM", "xterm-256color"),
            ("PS1", "$ "),
        ] {
            env.insert(name.into(), value.into());
        }
        env.insert("HOME".into(), dirs.at("home").display().to_string());
        let (name, value) = mark.split_once('=').unwrap();
        env.insert(name.into(), value.into());
        let options = SpawnOptions {
            cols: 80,
            rows: 24,
            cwd: dirs.at("a"),
            env,
            isolation: Some(plan),
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
        Shell { pty, output, exit }
    }

    fn run(&mut self, line: &str) {
        self.pty.write(format!("{line}\n").as_bytes());
    }

    fn wait_for(&self, wanted: &str) -> String {
        let start = Instant::now();
        loop {
            let text = String::from_utf8_lossy(&self.output.lock().unwrap()).into_owned();
            if text.contains(wanted) {
                return text;
            }
            assert!(start.elapsed() < WAIT, "no {wanted:?} in {text:?}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

#[test]
fn an_isolated_pty_shell_writes_its_worktree_and_cannot_read_another() {
    let dirs = Dirs::new("pty");
    let mut shell = Shell::new(&dirs, dirs.plan(), "NATAMUX_SANDBOX=pty-mark");
    shell.run("echo ok > own && echo WROTE-$((40+2))");
    shell.wait_for("WROTE-42");
    let other = dirs.at("b/sentinel").display().to_string();
    shell.run(&format!("cat '{other}' || echo DENIED-$((40+2))"));
    let text = shell.wait_for("DENIED-42");
    assert!(!text.contains("B-SECRET"), "{text}");
    assert_eq!(std::fs::read_to_string(dirs.at("a/own")).unwrap(), "ok\n");
    shell.pty.kill(Signal::Kill);
}

#[test]
fn a_program_that_left_the_shell_with_setsid_is_killed_by_its_mark() {
    let dirs = Dirs::new("sweep");
    let mark = format!("NATAMUX_SANDBOX=sweep-{}", std::process::id());
    let mut plan = dirs.plan();
    plan.sweep_mark = Some(mark.clone());
    let mut shell = Shell::new(&dirs, plan, &mark);
    shell.run("setsid sleep 300 & echo STARTED-$((40+2))");
    shell.wait_for("STARTED-42");
    let start = Instant::now();
    while holders(&mark).len() < 2 {
        assert!(start.elapsed() < WAIT, "the sleep never showed up");
        std::thread::sleep(Duration::from_millis(50));
    }
    shell.pty.kill(Signal::Hup);
    shell.pty.kill(Signal::Kill);
    let _ = shell.exit.recv_timeout(WAIT);
    let start = Instant::now();
    while !holders(&mark).is_empty() {
        assert!(start.elapsed() < WAIT, "left: {:?}", holders(&mark));
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(!shell.pty.leftover());
}

/// T-0375 (R7-3): from Landlock ABI 6 a sandboxed shell cannot signal a process outside the
/// sandbox (the daemon), and still signals its own children.
#[test]
fn a_sandboxed_shell_cannot_signal_out() {
    let abi = landlock_abi().unwrap();
    if abi < 6 {
        println!("Landlock ABI {abi} < 6: signals are not scoped, so this kernel cannot prove it");
        return;
    }
    let dirs = Dirs::new("signal");
    let line = format!(
        "kill -0 {} && echo SENT || echo BLOCKED; sleep 30 & kill $! && echo OWN-KILLED",
        std::process::id()
    );
    let out = sandboxed(&dirs.plan(), &line, &dirs.at("a"));
    assert!(out.contains("BLOCKED") && !out.contains("SENT"), "{out}");
    assert!(out.contains("OWN-KILLED"), "{out}");
}

/// T-0375 (R7-5): an isolated shell writes its own terminal and cannot open another pty of the
/// user, neither to write to it nor to read the keys typed into it.
#[test]
fn an_isolated_shell_reaches_only_its_own_pty() {
    use std::os::fd::AsRawFd;
    let dirs = Dirs::new("own-tty");
    let other = nix::pty::openpty(None::<&nix::pty::Winsize>, None).unwrap();
    let link = format!("/proc/self/fd/{}", other.slave.as_raw_fd());
    let other_tty = std::fs::read_link(link).unwrap().display().to_string();
    let mut shell = Shell::new(&dirs, dirs.plan(), "NATAMUX_SANDBOX=own-tty");
    shell.run("echo OWN-$((40+2)) > \"$(tty)\"");
    shell.wait_for("OWN-42");
    shell.run(&format!(
        "echo x > '{other_tty}' || echo NO-WRITE-$((40+2))"
    ));
    shell.wait_for("NO-WRITE-42");
    shell.run(&format!("( : < '{other_tty}' ) || echo NO-READ-$((40+2))"));
    shell.wait_for("NO-READ-42");
    shell.pty.kill(Signal::Kill);
}

#[test]
fn own_tty_leaves_one_pty_and_no_pts_folder_in_reach() {
    let plan = Dirs::new("own-plan").plan();
    let own = own_tty(&plan, Some(Path::new("/dev/pts/77"))).unwrap();
    assert!(!own.read_only.contains(&PathBuf::from("/dev")));
    assert!(!own.read_write.contains(&PathBuf::from("/dev/pts")));
    assert!(own.read_write.contains(&PathBuf::from("/dev/pts/77")));
    assert!(own.read_write.contains(&PathBuf::from("/dev/ptmx")));
    assert!(own.read_write.contains(&PathBuf::from("/dev/tty")));
    assert!(own.read_only.contains(&PathBuf::from("/dev/null")));
}
