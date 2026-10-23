use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use natamux_daemon::shared::event_bus::{DaemonMessage, EventBus};
use natamux_daemon::shared::scheduler::Scheduler;
use natamux_daemon::terminals::open_shells::{
    AgentIn, OpenShells, OpenShellsOptions, SaveShells, ShellsView,
};
use natamux_daemon::terminals::shell_files::{
    OpenShell, open_shells_path, read_open_shells, save_open_shells,
};

/// A fresh folder for one test, removed when the test ends.
pub struct TempDir(pub PathBuf);

impl TempDir {
    pub fn new() -> TempDir {
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let count = COUNT.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "natamux-open-shells-{}-{count}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Shells by task, and the program in the foreground of each.
pub struct FakeTerminals {
    bus: Arc<EventBus>,
    open: Mutex<HashMap<String, Vec<String>>>,
    programs: Mutex<HashMap<String, String>>,
}

impl FakeTerminals {
    fn changed(&self, task: &str) {
        self.bus.publish(DaemonMessage::ShellsChanged {
            task_id: task.into(),
        });
    }

    pub fn start_shell(&self, task: &str, terminal: &str) {
        self.open
            .lock()
            .unwrap()
            .entry(task.into())
            .or_default()
            .push(terminal.into());
        self.changed(task);
    }

    /// An agent starts or quits: the foreground changes, as TerminalService tells.
    pub fn set_program(&self, task: &str, terminal: &str, program: Option<&str>) {
        self.set_program_quiet(task, terminal, program);
        self.changed(task);
    }

    /// The foreground changes with no message on the bus.
    pub fn set_program_quiet(&self, task: &str, terminal: &str, program: Option<&str>) {
        let key = format!("{task}/{terminal}");
        let mut programs = self.programs.lock().unwrap();
        match program {
            Some(program) => programs.insert(key, program.into()),
            None => programs.remove(&key),
        };
    }

    pub fn end_shell(&self, task: &str, terminal: &str) {
        if let Some(shells) = self.open.lock().unwrap().get_mut(task) {
            shells.retain(|name| name != terminal);
        }
        self.programs
            .lock()
            .unwrap()
            .remove(&format!("{task}/{terminal}"));
        self.changed(task);
    }
}

impl ShellsView for FakeTerminals {
    fn list(&self, task_id: &str) -> Vec<String> {
        let mut names = self
            .open
            .lock()
            .unwrap()
            .get(task_id)
            .cloned()
            .unwrap_or_default();
        names.sort_by_key(|name| name.parse::<u32>().unwrap_or(u32::MAX));
        names
    }

    fn is_open(&self, task_id: &str, terminal: &str) -> bool {
        self.list(task_id).iter().any(|name| name == terminal)
    }

    fn foreground(&self, task_id: &str, terminal: &str) -> Option<String> {
        let program = self
            .programs
            .lock()
            .unwrap()
            .get(&format!("{task_id}/{terminal}"))
            .cloned();
        program.or_else(|| self.is_open(task_id, terminal).then(|| "zsh".to_string()))
    }
}

pub struct Setup {
    pub dir: PathBuf,
    pub scheduler: Arc<Scheduler>,
    pub bus: Arc<EventBus>,
    pub terminals: Arc<FakeTerminals>,
    pub shells: Arc<OpenShells>,
    pub writes: Arc<Mutex<Vec<Vec<OpenShell>>>>,
    pub messages: Arc<Mutex<Vec<DaemonMessage>>>,
    pub ended: Arc<Mutex<Vec<String>>>,
}

impl Setup {
    /// Moves the clock of this daemon on.
    pub fn advance(&self, ms: u64) {
        self.scheduler.advance(ms);
    }

    pub fn read(&self, task: &str) -> Vec<OpenShell> {
        read_open_shells(&self.dir, task)
    }

    pub fn file_exists(&self, task: &str) -> bool {
        open_shells_path(&self.dir, task).exists()
    }

    pub fn publish(&self, message: DaemonMessage) {
        self.bus.publish(message);
    }

    /// What the dashboard is told is coming back, by shell name.
    pub fn restoring(&self, task: &str) -> Vec<(String, Option<String>)> {
        let mut restoring = self.shells.restoring(task);
        restoring.sort_by_key(|(name, _)| name.parse::<u32>().unwrap_or(u32::MAX));
        restoring
    }
}

pub fn setup(dir: &Path) -> Setup {
    setup_with(dir, &[], None)
}

pub fn setup_with(dir: &Path, ended_tasks: &[&str], agent_in: Option<Arc<AgentIn>>) -> Setup {
    let scheduler = Scheduler::manual();
    let bus = EventBus::new();
    let terminals = Arc::new(FakeTerminals {
        bus: Arc::clone(&bus),
        open: Mutex::default(),
        programs: Mutex::default(),
    });
    let ended = Arc::new(Mutex::new(
        ended_tasks
            .iter()
            .map(|task| task.to_string())
            .collect::<Vec<_>>(),
    ));
    let writes: Arc<Mutex<Vec<Vec<OpenShell>>>> = Arc::default();
    let sink = Arc::clone(&writes);
    let save: Arc<SaveShells> = Arc::new(move |dir, task, list| {
        sink.lock().unwrap().push(list.to_vec());
        save_open_shells(dir, task, list)
    });
    let running = Arc::clone(&terminals);
    let ended_check = Arc::clone(&ended);
    let shells = OpenShells::new(OpenShellsOptions {
        data_dir: dir.to_path_buf(),
        terminals: Arc::clone(&terminals) as Arc<dyn ShellsView>,
        bus: Arc::clone(&bus),
        scheduler: Arc::clone(&scheduler),
        is_ended: Arc::new(move |task| ended_check.lock().unwrap().iter().any(|t| t == task)),
        // An agent runs where a program other than the shell is in the foreground.
        agent_runs: Some(Arc::new(move |task, terminal| {
            running
                .programs
                .lock()
                .unwrap()
                .contains_key(&format!("{task}/{terminal}"))
        })),
        agent_in,
        save: Some(save),
    });
    shells.start();
    let messages: Arc<Mutex<Vec<DaemonMessage>>> = Arc::default();
    let heard = Arc::clone(&messages);
    bus.subscribe(move |message| heard.lock().unwrap().push(message.clone()));
    Setup {
        dir: dir.to_path_buf(),
        scheduler,
        bus,
        terminals,
        shells,
        writes,
        messages,
        ended,
    }
}

pub fn open(name: &str, agent: Option<&str>, session: Option<&str>) -> OpenShell {
    OpenShell {
        name: name.into(),
        agent: agent.map(String::from),
        session: session.map(String::from),
    }
}

pub fn plain(name: &str) -> OpenShell {
    open(name, None, None)
}

pub fn copilot() -> OpenShell {
    open("6", Some("copilot"), Some("c-1"))
}

/// The expected `restoring` answer.
pub fn restoring_of(list: &[(&str, Option<&str>)]) -> Vec<(String, Option<String>)> {
    list.iter()
        .map(|(name, agent)| (name.to_string(), agent.map(String::from)))
        .collect()
}

pub fn changed(task: &str) -> DaemonMessage {
    DaemonMessage::ShellsChanged {
        task_id: task.into(),
    }
}
