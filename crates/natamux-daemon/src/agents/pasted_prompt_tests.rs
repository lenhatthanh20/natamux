use std::sync::atomic::AtomicI64;

use super::*;

const START: i64 = 1_000;

struct Env {
    clock: Arc<AtomicI64>,
    front: Arc<Mutex<Option<String>>>,
    pasted: Arc<Mutex<Vec<(String, String, String)>>>,
    prompts: Arc<PastedPrompts>,
}

/// The shell is in front until a test puts the agent there.
fn env() -> Env {
    let clock = Arc::new(AtomicI64::new(START));
    let front = Arc::new(Mutex::new(Some("zsh".to_string())));
    let pasted = Arc::new(Mutex::new(Vec::new()));
    let now = Arc::clone(&clock);
    let in_front = Arc::clone(&front);
    let into = Arc::clone(&pasted);
    let prompts = PastedPrompts::new(
        Arc::new(move || now.load(Ordering::SeqCst)),
        Arc::new(move |task: &str, terminal: &str, text: &str| {
            into.lock()
                .unwrap()
                .push((task.into(), terminal.into(), text.into()));
        }),
        Arc::new(move |_: &str, _: &str| in_front.lock().unwrap().clone()),
    );
    Env {
        clock,
        front,
        pasted,
        prompts,
    }
}

impl Env {
    fn at(&self, ms: i64) {
        self.clock.store(ms, Ordering::SeqCst);
    }

    fn in_front(&self, program: Option<&str>) {
        *self.front.lock().unwrap() = program.map(str::to_string);
    }

    fn pasted(&self) -> Vec<(String, String, String)> {
        self.pasted.lock().unwrap().clone()
    }

    fn output(&self, terminal: &str, text: &str) {
        self.prompts.on_output("T-0001", terminal, text.as_bytes());
    }

    /// `hold` starts the real ticker thread; these tests call `tick` with the fake clock, so the
    /// thread is marked as running to keep it out.
    fn hold(&self, terminal: &str, text: &str) {
        self.prompts.ticking.store(true, Ordering::SeqCst);
        self.prompts.hold("T-0001", terminal, text);
    }

    /// The agent comes up at `ms` and draws its first screen.
    fn agent_up(&self, ms: i64) {
        self.at(ms);
        self.in_front(Some("aider"));
        self.prompts.tick();
        self.output("1", "Aider v0.86\r\n> ");
    }
}

fn one(terminal: &str, text: &str) -> Vec<(String, String, String)> {
    vec![("T-0001".into(), terminal.into(), text.into())]
}

#[test]
fn pastes_nothing_before_the_agent_printed() {
    let env = env();
    env.hold("1", "Fix the bug");
    env.in_front(Some("aider"));
    env.at(START + SETTLE_MS * 4);
    env.prompts.tick();
    assert!(env.pasted().is_empty());
    assert!(env.prompts.is_held("T-0001", "1"));
}

#[test]
fn pastes_once_the_agents_screen_is_quiet() {
    let env = env();
    env.hold("1", "Fix the bug");
    env.agent_up(2_000);
    env.at(2_000 + SETTLE_MS - 1);
    env.prompts.tick();
    assert!(env.pasted().is_empty());
    env.at(2_000 + SETTLE_MS);
    env.prompts.tick();
    assert_eq!(env.pasted(), one("1", "Fix the bug"));
}

#[test]
fn the_shells_echo_alone_is_no_first_screen() {
    let env = env();
    env.hold("1", "Fix the bug");
    // The shell echoes the launch line; the agent is slow to start.
    env.output("1", ". '/data/agent-launch-T-0001.sh'\r\n");
    env.at(START + SETTLE_MS * 2);
    env.prompts.tick();
    assert!(env.pasted().is_empty());
    // Now the agent is in front, but has drawn nothing yet: still no paste.
    env.in_front(Some("aider"));
    env.at(START + SETTLE_MS * 4);
    env.prompts.tick();
    assert!(env.pasted().is_empty());
    assert!(env.prompts.is_held("T-0001", "1"));
}

#[test]
fn more_output_moves_the_quiet_time_on() {
    let env = env();
    env.hold("1", "Fix the bug");
    env.agent_up(2_000);
    env.at(3_000);
    env.output("1", "Loading the repo map");
    env.at(2_000 + SETTLE_MS);
    env.prompts.tick();
    assert!(env.pasted().is_empty());
    env.at(3_000 + SETTLE_MS);
    env.prompts.tick();
    assert_eq!(env.pasted().len(), 1);
}

#[test]
fn drops_the_prompt_when_the_agent_exited_and_the_shell_is_in_front_again() {
    let env = env();
    env.hold("1", "rm -rf ~");
    env.agent_up(2_000);
    env.at(2_500);
    env.output("1", "Bye\r\n% ");
    env.in_front(Some("zsh"));
    env.at(2_500 + SETTLE_MS);
    env.prompts.tick();
    assert!(env.pasted().is_empty());
    assert!(!env.prompts.is_held("T-0001", "1"));
    env.at(START + LAUNCH_WATCH_MS * 2);
    env.prompts.tick();
    assert!(env.pasted().is_empty());
}

#[test]
fn drops_the_prompt_of_an_agent_the_shell_cannot_find() {
    let env = env();
    env.hold("1", "Fix the bug");
    env.output("1", "zsh: command not found: aider\r\n");
    env.output("1", "\x1b]2;natamux-agent-failed\x07% ");
    env.at(START + SETTLE_MS);
    env.prompts.tick();
    assert!(!env.prompts.is_held("T-0001", "1"));
    assert!(env.pasted().is_empty());
}

#[test]
fn drops_the_prompt_when_the_agent_never_came_up() {
    let env = env();
    env.hold("1", "Fix the bug");
    env.at(START + LAUNCH_WATCH_MS);
    env.prompts.tick();
    assert!(env.pasted().is_empty());
    assert!(!env.prompts.is_held("T-0001", "1"));
}

#[test]
fn pastes_after_the_launch_watch_to_an_agent_in_front_that_printed_nothing() {
    let env = env();
    env.hold("2", "Review it");
    env.in_front(Some("node"));
    env.at(START + LAUNCH_WATCH_MS);
    env.prompts.tick();
    assert_eq!(env.pasted(), one("2", "Review it"));
}

#[test]
fn where_the_front_is_unknown_the_ended_title_drops_the_prompt() {
    let env = env();
    env.in_front(None);
    env.hold("1", "Fix the bug");
    // Windows types the title's words in its launch line: they alone end nothing.
    env.output(
        "1",
        "$Host.UI.RawUI.WindowTitle = 'natamux-agent-exited'\r\n",
    );
    env.output("1", "\x1b]0;natamux-agent-exi");
    env.output("1", "ted\x07");
    env.at(START + SETTLE_MS);
    env.prompts.tick();
    assert!(env.pasted().is_empty());
    assert!(!env.prompts.is_held("T-0001", "1"));
}

#[test]
fn where_the_front_is_unknown_it_pastes_once_the_output_is_quiet() {
    let env = env();
    env.in_front(None);
    env.hold("1", "Fix the bug");
    env.output("1", "Welcome to Amp");
    env.at(START + SETTLE_MS);
    env.prompts.tick();
    assert_eq!(env.pasted(), one("1", "Fix the bug"));
}

#[test]
fn never_pastes_a_prompt_twice() {
    let env = env();
    env.hold("1", "Fix the bug");
    env.agent_up(START);
    env.at(START + SETTLE_MS);
    env.prompts.tick();
    env.output("1", "more");
    env.at(START + LAUNCH_WATCH_MS * 2);
    env.prompts.tick();
    assert_eq!(env.pasted().len(), 1);
    assert!(!env.prompts.is_held("T-0001", "1"));
}

#[test]
fn output_of_another_terminal_does_not_count() {
    let env = env();
    env.hold("1", "Fix the bug");
    env.in_front(Some("aider"));
    env.prompts.tick();
    env.output("2", "other");
    env.prompts.on_output("T-0002", "1", b"other");
    env.at(START + SETTLE_MS);
    env.prompts.tick();
    assert!(env.pasted().is_empty());
}

#[test]
fn a_newer_start_replaces_a_prompt_not_pasted_yet() {
    let env = env();
    env.hold("1", "old");
    env.hold("1", "new");
    env.in_front(Some("aider"));
    env.at(START + LAUNCH_WATCH_MS);
    env.prompts.tick();
    assert_eq!(env.pasted(), one("1", "new"));
}
