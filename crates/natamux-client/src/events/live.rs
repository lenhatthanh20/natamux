//! The live task list: `GET /tasks` again whenever `/events` says a task changed.

use std::sync::Arc;

use async_tungstenite::tokio::connect_async;
use async_tungstenite::tungstenite::{self, Message};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use futures::{FutureExt, StreamExt};
use natamux_proto::events::kind::EventKind;
use natamux_proto::tasks::task::Task;
use natamux_proto::terminals::query::encode;

use crate::config::Config;
use crate::error::ClientError;
use crate::tasks::api;
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

pub fn watch(
    runtime: &tokio::runtime::Handle,
    config: Arc<Config>,
) -> UnboundedReceiver<TaskListEvent> {
    let (events, receiver) = unbounded();
    runtime.spawn(run(config, events));
    receiver
}

async fn run(config: Arc<Config>, events: UnboundedSender<TaskListEvent>) {
    let emit = |event: TaskListEvent| events.unbounded_send(event).is_ok();
    if !emit(TaskListEvent::Status(LinkStatus::Connecting)) {
        return;
    }
    let mut tries: u32 = 0;
    loop {
        let reason = match follow(&config, &events, &mut tries).await {
            Ok(Stop::AppGone) => return,
            Ok(Stop::Refused) => {
                emit(TaskListEvent::Status(LinkStatus::Refused));
                return;
            }
            Err(reason) => reason,
        };
        if !emit(TaskListEvent::Status(LinkStatus::Reconnecting(reason))) {
            return;
        }
        tokio::time::sleep(reconnect_delay(tries, fastrand::f64())).await;
        tries = tries.saturating_add(1);
    }
}

enum Stop {
    AppGone,
    Refused,
}

/// One `/events` socket, until it is lost. Each burst of task changes is one `GET /tasks`: while
/// a fetch runs, new messages wait in the socket, so at most one more fetch follows.
async fn follow(
    config: &Config,
    events: &UnboundedSender<TaskListEvent>,
    tries: &mut u32,
) -> Result<Stop, ClientError> {
    let url = config.ws_url(&format!("/events?token={}", encode(&config.token)))?;
    let (mut socket, _) = match connect_async(url.as_str()).await {
        Ok(socket) => socket,
        Err(tungstenite::Error::Http(response)) if response.status().as_u16() == 401 => {
            return Ok(Stop::Refused);
        }
        Err(error) => {
            return Err(ClientError::Unreachable {
                url: config.base_url.clone(),
                reason: error.to_string(),
            });
        }
    };
    let send = |event: TaskListEvent| events.unbounded_send(event).is_ok();
    // Subscribed first, then read: a change in between is not lost.
    let mut changed = true;
    let mut told_live = false;
    loop {
        if changed {
            match api::list_tasks(config).await {
                Ok(tasks) => {
                    *tries = 0;
                    if !told_live {
                        told_live = true;
                        if !send(TaskListEvent::Status(LinkStatus::Live)) {
                            return Ok(Stop::AppGone);
                        }
                    }
                    if !send(TaskListEvent::Tasks(tasks)) {
                        return Ok(Stop::AppGone);
                    }
                }
                Err(ClientError::Unauthorized) => return Ok(Stop::Refused),
                Err(error) => return Err(error),
            }
            changed = false;
        }
        let gone = || ClientError::Unreachable {
            url: config.base_url.clone(),
            reason: "the event socket closed".into(),
        };
        let frame = socket.next().await;
        changed |= changes(&frame).ok_or_else(gone)?;
        // Messages that are already here join this fetch.
        while let Some(frame) = socket.next().now_or_never() {
            changed |= changes(&frame).ok_or_else(gone)?;
        }
        if events.is_closed() {
            return Ok(Stop::AppGone);
        }
    }
}

/// Whether a frame asks for the list again. None when the socket is gone.
fn changes(frame: &Option<Result<Message, tungstenite::Error>>) -> Option<bool> {
    match frame {
        Some(Ok(Message::Text(text))) => {
            Some(EventKind::parse(text.as_str()).is_some_and(EventKind::changes_task_list))
        }
        Some(Ok(Message::Close(_))) | Some(Err(_)) | None => None,
        Some(Ok(_)) => Some(false),
    }
}
