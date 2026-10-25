//! The live dashboard data: each part is read again whenever `/events` says it changed, as the
//! dashboard's live feed (packages/web/src/features/app/useLiveTasks.ts and lib/live.ts).

use std::collections::BTreeSet;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use async_tungstenite::WebSocketStream;
use async_tungstenite::tokio::{ConnectStream, connect_async};
use async_tungstenite::tungstenite::{self, Message};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use futures::{FutureExt, StreamExt};
use natamux_proto::tasks::task::Task;
use natamux_proto::terminals::query::encode;
use tokio::sync::watch;
use tokio::time::Instant;

use crate::config::Config;
use crate::error::ClientError;
use crate::events::stale::{DashboardEvent, Halt, Stale, send};
use crate::terminals::backoff::reconnect_delay;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TaskListEvent {
    Tasks(Vec<Task>),
    Status(LinkStatus),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkStatus {
    Connecting,
    Live,
    /// Lost or not reachable; tries again by itself.
    Reconnecting(ClientError),
    /// The daemon refused the token. Trying again would not help.
    Refused,
}

/// How often the feed reads by itself. The defaults are the dashboard's.
#[derive(Clone, Copy, Debug)]
pub struct FeedTimes {
    /// At most one diff stats read per this time for `workspace.dirty` (`DIFFSTATS_EVERY_MS`).
    pub diff_stats_every: Duration,
    /// Jobs, activity, diff stats and usage, for what the socket missed (`FALLBACK_POLL_MS`).
    pub fallback_every: Duration,
    /// The terminal rows of the tasks the app shows.
    pub terminals_every: Duration,
}

impl Default for FeedTimes {
    fn default() -> Self {
        FeedTimes {
            diff_stats_every: Duration::from_secs(15),
            fallback_every: Duration::from_secs(60),
            terminals_every: Duration::from_secs(5 * 60),
        }
    }
}

/// The feed's events, and where the app says which tasks show terminal rows.
pub struct DashboardFeed {
    pub events: UnboundedReceiver<DashboardEvent>,
    pub terminals: TerminalRows,
}

/// The tasks whose terminals the app shows. The feed reads a task's terminals when it is added
/// here, on its `shells.changed`, and every `terminals_every`.
pub struct TerminalRows(watch::Sender<BTreeSet<String>>);

impl TerminalRows {
    pub fn show(&self, task_ids: impl IntoIterator<Item = String>) {
        let ids: BTreeSet<String> = task_ids.into_iter().collect();
        self.0.send_if_modified(|shown| {
            let changed = *shown != ids;
            *shown = ids;
            changed
        });
    }
}

pub fn watch_dashboard(
    runtime: &tokio::runtime::Handle,
    config: Arc<Config>,
    times: FeedTimes,
) -> DashboardFeed {
    let (events, receiver) = unbounded();
    let (rows, shown) = watch::channel(BTreeSet::new());
    runtime.spawn(run(config, times, events, shown));
    DashboardFeed {
        events: receiver,
        terminals: TerminalRows(rows),
    }
}

/// Only the task list of the dashboard feed, for callers that show nothing else.
pub fn watch(
    runtime: &tokio::runtime::Handle,
    config: Arc<Config>,
) -> UnboundedReceiver<TaskListEvent> {
    let (events, receiver) = unbounded();
    let mut feed = watch_dashboard(runtime, config, FeedTimes::default());
    runtime.spawn(async move {
        while let Some(event) = feed.events.next().await {
            let event = match event {
                DashboardEvent::Tasks(tasks) => TaskListEvent::Tasks(tasks),
                DashboardEvent::Status(status) => TaskListEvent::Status(status),
                _ if events.is_closed() => return,
                _ => continue,
            };
            if events.unbounded_send(event).is_err() {
                return;
            }
        }
    });
    receiver
}

async fn run(
    config: Arc<Config>,
    times: FeedTimes,
    events: UnboundedSender<DashboardEvent>,
    mut shown: watch::Receiver<BTreeSet<String>>,
) {
    if send(&events, DashboardEvent::Status(LinkStatus::Connecting)).is_err() {
        return;
    }
    let mut tries: u32 = 0;
    loop {
        let halt = match follow(&config, times, &events, &mut shown, &mut tries).await {
            Ok(never) => match never {},
            Err(halt) => halt,
        };
        let reason = match halt {
            Halt::AppGone => return,
            Halt::Refused => {
                let _ = send(&events, DashboardEvent::Status(LinkStatus::Refused));
                return;
            }
            Halt::Lost(reason) => reason,
        };
        let status = DashboardEvent::Status(LinkStatus::Reconnecting(reason));
        if send(&events, status).is_err() {
            return;
        }
        tokio::time::sleep(reconnect_delay(tries, fastrand::f64())).await;
        tries = tries.saturating_add(1);
    }
}

/// One `/events` socket, until it is lost. Each burst of changes is one read per part: while
/// reads run, new messages wait in the socket, and the ones already there join the next reads.
async fn follow(
    config: &Config,
    times: FeedTimes,
    events: &UnboundedSender<DashboardEvent>,
    shown: &mut watch::Receiver<BTreeSet<String>>,
    tries: &mut u32,
) -> Result<Infallible, Halt> {
    let mut socket = connect(config).await?;
    // Subscribed first, then read: a change in between is not lost.
    let mut rows = shown.borrow_and_update().clone();
    let mut stale = Stale::everything(&rows);
    let mut live = false;
    let mut next_poll = Instant::now() + times.fallback_every;
    let mut next_rows = Instant::now() + times.terminals_every;
    let mut rows_open = true;
    let gone = || {
        Halt::Lost(ClientError::Unreachable {
            url: config.base_url.clone(),
            reason: "the event socket closed".into(),
        })
    };
    loop {
        stale
            .read(config, times.diff_stats_every, &mut live, events)
            .await?;
        if live {
            *tries = 0;
        }
        let wake = [
            Some(next_poll),
            Some(next_rows),
            stale.due(times.diff_stats_every),
        ]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(next_poll);
        tokio::select! {
            frame = socket.next() => {
                take(frame, &mut stale, &rows, events).ok_or_else(gone)??;
                // Messages that are already here join these reads.
                while let Some(frame) = socket.next().now_or_never() {
                    take(frame, &mut stale, &rows, events).ok_or_else(gone)??;
                }
            }
            () = tokio::time::sleep_until(wake) => {
                let now = Instant::now();
                if now >= next_poll {
                    stale.poll();
                    next_poll = now + times.fallback_every;
                }
                if now >= next_rows {
                    stale.terminals.extend(rows.iter().cloned());
                    next_rows = now + times.terminals_every;
                }
            }
            changed = shown.changed(), if rows_open => match changed {
                Ok(()) => {
                    let now_shown = shown.borrow_and_update().clone();
                    stale.terminals.extend(now_shown.difference(&rows).cloned());
                    rows = now_shown;
                }
                // The app let go of its TerminalRows: the rows stay as they are.
                Err(_) => rows_open = false,
            },
        }
        if events.is_closed() {
            return Err(Halt::AppGone);
        }
    }
}

async fn connect(config: &Config) -> Result<WebSocketStream<ConnectStream>, Halt> {
    let url = config
        .ws_url(&format!("/events?token={}", encode(&config.token)))
        .map_err(Halt::Lost)?;
    match connect_async(url.as_str()).await {
        Ok((socket, _)) => Ok(socket),
        Err(tungstenite::Error::Http(response)) if response.status().as_u16() == 401 => {
            Err(Halt::Refused)
        }
        Err(error) => Err(Halt::Lost(ClientError::Unreachable {
            url: config.base_url.clone(),
            reason: error.to_string(),
        })),
    }
}

/// Notes one frame. None when the socket is gone.
fn take(
    frame: Option<Result<Message, tungstenite::Error>>,
    stale: &mut Stale,
    shown: &BTreeSet<String>,
    events: &UnboundedSender<DashboardEvent>,
) -> Option<Result<(), Halt>> {
    match frame {
        Some(Ok(Message::Text(text))) => Some(match stale.note(text.as_str(), shown) {
            Some(step) => send(events, DashboardEvent::WorkspaceStep(step)),
            None => Ok(()),
        }),
        Some(Ok(Message::Close(_))) | Some(Err(_)) | None => None,
        Some(Ok(_)) => Some(Ok(())),
    }
}
