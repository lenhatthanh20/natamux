//! Live terminals kept off screen, so showing one again needs no new socket and no history
//! replay (`features/terminals/terminal-cache.ts`). The workspace and the Agents grid share them:
//! Terminal 1 of a task is the same terminal in both. Each hidden terminal keeps its socket and
//! screen, so they are capped.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use gpui::{App, AppContext, Entity, Global, Window};
use natamux_client::TerminalStatus;

use super::remote_terminal::{RemoteTerminal, Target};
use super::terminal_find::TerminalFind;
use crate::app::source::Source;

/// `KEEP_ALIVE_TERMINALS`.
pub const KEEP_ALIVE_TERMINALS: usize = 8;
/// `HIDDEN_TERMINAL_TTL_MS`: an idle hidden terminal is freed too.
pub const HIDDEN_TTL_MS: u64 = 20 * 60_000;

/// The views that show cached terminals.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Owner {
    Workspace,
    Agents,
}

/// `terminalKey`.
pub fn terminal_key(task_id: &str, terminal: &str) -> String {
    format!("{task_id}/{terminal}")
}

fn task_of(key: &str) -> &str {
    key.split_once('/').map_or(key, |(task, _)| task)
}

/// What the eviction rule needs of one entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntryState {
    pub key: String,
    pub shown: bool,
    /// When it left the screen, in ms.
    pub hidden_at: u64,
    /// Its task is gone, finished or has no workspace: its shell ends once no client is left.
    pub task_ends: bool,
}

/// The hidden entries to free: those of tasks whose shells end, those hidden longer than the
/// TTL, then the oldest beyond the cap. A shown entry always stays.
pub fn evictions(entries: &[EntryState], now: u64) -> Vec<String> {
    let mut hidden: Vec<&EntryState> = entries.iter().filter(|entry| !entry.shown).collect();
    hidden.sort_by_key(|entry| entry.hidden_at);
    let mut out: Vec<String> = hidden
        .iter()
        .filter(|entry| entry.task_ends || now.saturating_sub(entry.hidden_at) >= HIDDEN_TTL_MS)
        .map(|entry| entry.key.clone())
        .collect();
    let kept: Vec<&&EntryState> = hidden
        .iter()
        .filter(|entry| !out.contains(&entry.key))
        .collect();
    let over = kept.len().saturating_sub(KEEP_ALIVE_TERMINALS);
    out.extend(kept.iter().take(over).map(|entry| entry.key.clone()));
    out
}

struct Entry {
    terminal: Entity<RemoteTerminal>,
    find: Option<Entity<TerminalFind>>,
    hidden_at: u64,
}

#[derive(Default)]
pub struct TerminalCache {
    entries: BTreeMap<String, Entry>,
    claims: BTreeMap<Owner, BTreeSet<String>>,
    /// The tasks whose shells live: listed, not finished, with a workspace (`shellsEnd`).
    live: BTreeSet<String>,
}

impl Global for TerminalCache {}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64)
}

impl TerminalCache {
    fn global(cx: &mut App) -> &mut TerminalCache {
        cx.default_global::<TerminalCache>()
    }

    /// The terminal of `key`, made and connected on first use.
    pub fn terminal(
        source: &Arc<Source>,
        task_id: &str,
        name: &str,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<RemoteTerminal> {
        let key = terminal_key(task_id, name);
        if let Some(entry) = Self::global(cx).entries.get(&key) {
            return entry.terminal.clone();
        }
        let target = Target {
            task_id: task_id.into(),
            terminal: name.into(),
        };
        let source = source.clone();
        let terminal = cx.new(|cx| RemoteTerminal::new(source, target, window, cx));
        Self::global(cx).entries.insert(
            key,
            Entry {
                terminal: terminal.clone(),
                find: None,
                hidden_at: now_ms(),
            },
        );
        terminal
    }

    /// A cached terminal, if it is on screen or kept.
    pub fn cached(key: &str, cx: &App) -> Option<Entity<RemoteTerminal>> {
        let entry = cx.try_global::<TerminalCache>()?.entries.get(key)?;
        Some(entry.terminal.clone())
    }

    /// The connection state of a cached terminal, if it is cached.
    pub fn status(key: &str, cx: &App) -> Option<TerminalStatus> {
        let entry = cx.try_global::<TerminalCache>()?.entries.get(key)?;
        Some(entry.terminal.read(cx).status().clone())
    }

    pub fn find(key: &str, cx: &App) -> Option<Entity<TerminalFind>> {
        cx.try_global::<TerminalCache>()?
            .entries
            .get(key)?
            .find
            .clone()
    }

    pub fn set_find(key: &str, find: Option<Entity<TerminalFind>>, cx: &mut App) {
        if let Some(entry) = Self::global(cx).entries.get_mut(key) {
            entry.find = find;
        }
    }

    /// The terminals one view shows now. The others leave the screen, give up the shell's
    /// size, and are freed by the rule above.
    pub fn claim(owner: Owner, keys: BTreeSet<String>, cx: &mut App) {
        let cache = Self::global(cx);
        if cache.claims.get(&owner) == Some(&keys) {
            return;
        }
        cache.claims.insert(owner, keys);
        Self::settle(cx);
    }

    /// The task list changed: the terminals of tasks that are gone, finished or have no
    /// workspace are freed once hidden.
    pub fn set_live(live: BTreeSet<String>, cx: &mut App) {
        let cache = Self::global(cx);
        if cache.live == live {
            return;
        }
        cache.live = live;
        Self::settle(cx);
    }

    /// A terminal the user closed: its socket goes now.
    pub fn forget(key: &str, cx: &mut App) {
        Self::global(cx).entries.remove(key);
    }

    fn settle(cx: &mut App) {
        let now = now_ms();
        let cache = Self::global(cx);
        let shown: BTreeSet<String> = cache.claims.values().flatten().cloned().collect();
        // A shown terminal leaves the screen at the last time it was shown.
        for (key, entry) in &mut cache.entries {
            if shown.contains(key) {
                entry.hidden_at = now;
            }
        }
        let states: Vec<EntryState> = cache
            .entries
            .iter()
            .map(|(key, entry)| EntryState {
                key: key.clone(),
                shown: shown.contains(key),
                hidden_at: entry.hidden_at,
                task_ends: !cache.live.contains(task_of(key)),
            })
            .collect();
        for key in evictions(&states, now) {
            cache.entries.remove(&key);
        }
        let terminals: Vec<(Entity<RemoteTerminal>, bool)> = cache
            .entries
            .iter()
            .map(|(key, entry)| (entry.terminal.clone(), shown.contains(key)))
            .collect();
        for (terminal, visible) in terminals {
            terminal.update(cx, |terminal, _| terminal.set_visible(visible));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(key: &str, shown: bool, hidden_at: u64, task_ends: bool) -> EntryState {
        EntryState {
            key: key.into(),
            shown,
            hidden_at,
            task_ends,
        }
    }

    #[test]
    fn keeps_the_newest_8_hidden_terminals() {
        let mut entries: Vec<EntryState> = (0..10)
            .map(|at| entry(&format!("T-{at}/1"), false, at, false))
            .collect();
        entries.push(entry("T-shown/1", true, 0, false));
        assert_eq!(evictions(&entries, 100), ["T-0/1", "T-1/1"]);
    }

    #[test]
    fn frees_a_terminal_hidden_longer_than_20_minutes() {
        let entries = [
            entry("T-1/1", false, 0, false),
            entry("T-2/1", false, HIDDEN_TTL_MS, false),
        ];
        assert_eq!(evictions(&entries, HIDDEN_TTL_MS), ["T-1/1"]);
    }

    #[test]
    fn frees_the_hidden_terminals_of_a_task_whose_shells_end() {
        let entries = [
            entry("T-1/1", false, 50, true),
            entry("T-1/2", true, 50, true),
            entry("T-2/1", false, 50, false),
        ];
        assert_eq!(evictions(&entries, 60), ["T-1/1"]);
    }

    #[test]
    fn keys_name_the_task_and_the_terminal() {
        assert_eq!(terminal_key("T-1", "12"), "T-1/12");
        assert_eq!(task_of("T-1/12"), "T-1");
    }
}
