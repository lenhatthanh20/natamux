//! The echo of a typed shell while 29 agents print, each shown in an acking pane (T-0210).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use natamux_daemon::terminals::service::AttachOptions;
use natamux_daemon::terminals::types::{ClientRef, Frame, TerminalClient};
use natamux_proto::terminals::messages::{ACK_EVERY_BYTES, ServerMessage};

use super::{AGENTS, COLS, Echoes, Sink, TASK, echoes, emit, setup, size};

/// An agent's TUI redraws this often, and scrolls a long answer this often.
const AGENT_FRAME_MS: u64 = 100;
const AGENT_SCROLL_EVERY: u64 = 20;

/// What an acking dashboard owes the daemon.
#[derive(Default)]
struct AckState {
    live: bool,
    got: u64,
    acked: u64,
    due: Option<u64>,
}

/// A shown pane that parses at once. Like the app, it acks every ACK_EVERY_BYTES and at the end
/// of a catch-up. The service holds the session's lock while it sends, so the ack waits in `due`.
#[derive(Default)]
struct Acking(Mutex<AckState>);

impl TerminalClient for Acking {
    fn send(&self, frame: Frame) {
        let mut state = self.0.lock().unwrap();
        match frame {
            Frame::Message(ServerMessage::Replayed { offset }) => {
                state.live = true;
                state.got = offset;
                state.acked = offset;
                state.due = Some(offset);
            }
            Frame::Bytes(bytes) if state.live => {
                state.got += bytes.len() as u64;
                if state.got - state.acked >= ACK_EVERY_BYTES {
                    state.acked = state.got;
                    state.due = Some(state.got);
                }
            }
            _ => {}
        }
    }
}

/// One redraw of an agent's TUI: a synchronized frame of 20 coloured rows over the last ones.
fn agent_frame(tick: u64) -> Vec<u8> {
    let mut text = String::from("\x1b[?2026h\x1b[20A");
    for row in 0..20 {
        let colour = 31 + row % 6;
        text += &format!("\r\x1b[2K\x1b[{colour}m{tick:08} row {row:02} \x1b[0m");
        text += &"a".repeat(usize::from(COLS) - 30);
        text += "\r\n";
    }
    text += "\x1b[?2026l";
    text.into_bytes()
}

/// A long answer of an agent: 200 lines that scroll.
fn agent_scroll() -> Vec<u8> {
    (0..200)
        .flat_map(|line| format!("{line:05} {}\r\n", "s".repeat(100)).into_bytes())
        .collect()
}

/// The echo delay of a typed shell while 29 agents print, each shown in an acking pane: p50 and
/// p95 in ms.
pub fn echo_while_agents_print() -> (f64, f64) {
    let (service, sinks) = setup();
    let mut panes = Vec::new();
    for index in 1..=AGENTS {
        let acking = Arc::new(Acking::default());
        let client: ClientRef = acking.clone();
        let terminal = format!("agent-{index}");
        let options = AttachOptions::default();
        let detach = service
            .attach(TASK, Arc::clone(&client), size(), &terminal, options)
            .unwrap();
        panes.push((acking, client, terminal, detach));
    }
    let (heard, arrivals) = channel();
    let typed = Arc::new(Echoes {
        live: AtomicBool::new(false),
        heard: Mutex::new(heard),
    });
    let client: ClientRef = typed.clone();
    let _detach = service
        .attach(TASK, client, size(), "typed", AttachOptions::default())
        .unwrap();
    let all: Vec<Sink> = sinks.lock().unwrap().clone();
    let stop = Arc::new(AtomicBool::new(false));
    let scroll = agent_scroll();
    let agents: Vec<_> = panes
        .iter()
        .enumerate()
        .map(|(index, (acking, client, terminal, _))| {
            let (sink, stop) = (Arc::clone(&all[index]), Arc::clone(&stop));
            let (scroll, service) = (scroll.clone(), service.clone());
            let (acking, client) = (Arc::clone(acking), Arc::clone(client));
            let terminal = terminal.clone();
            // Agents do not redraw in step: each starts at its own point of the 100 ms.
            let offset = index as u64 * AGENT_FRAME_MS / AGENTS as u64;
            let start = Instant::now() + Duration::from_millis(offset);
            std::thread::spawn(move || {
                let mut tick = 0;
                while !stop.load(Ordering::SeqCst) {
                    let due = start + Duration::from_millis(tick * AGENT_FRAME_MS);
                    std::thread::sleep(due.saturating_duration_since(Instant::now()));
                    emit(&sink, &agent_frame(tick));
                    if tick % AGENT_SCROLL_EVERY == AGENT_SCROLL_EVERY - 1 {
                        emit(&sink, &scroll);
                    }
                    let owed = acking.0.lock().unwrap().due.take();
                    if let Some(offset) = owed {
                        service.ack(TASK, &terminal, &client, offset);
                    }
                    tick += 1;
                }
            })
        })
        .collect();
    let measured = echoes(&all[AGENTS], &arrivals);
    stop.store(true, Ordering::SeqCst);
    for agent in agents {
        agent.join().unwrap();
    }
    drop(panes);
    service.close_all();
    measured
}
