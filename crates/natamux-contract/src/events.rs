//! A raw `/events` socket that keeps every message, in order.

use std::time::{Duration, Instant};

use async_tungstenite::WebSocketStream;
use async_tungstenite::tokio::{ConnectStream, connect_async};
use async_tungstenite::tungstenite::Message;
use futures::StreamExt;
use serde_json::Value;

use crate::daemon::Daemon;

pub struct Events {
    socket: Option<WebSocketStream<ConnectStream>>,
    pub messages: Vec<Value>,
    /// The HTTP status of a refused upgrade, such as 401.
    pub refused: Option<u16>,
}

impl Events {
    pub async fn open(daemon: &Daemon, token: Option<&str>) -> Events {
        match connect_async(daemon.events_url(token).as_str()).await {
            Ok((socket, _)) => Events {
                socket: Some(socket),
                messages: Vec::new(),
                refused: None,
            },
            Err(async_tungstenite::tungstenite::Error::Http(answer)) => Events {
                socket: None,
                messages: Vec::new(),
                refused: Some(answer.status().as_u16()),
            },
            Err(_) => Events {
                socket: None,
                messages: Vec::new(),
                refused: Some(0),
            },
        }
    }

    /// Reads what comes for `time`.
    pub async fn read_for(&mut self, time: Duration) {
        let Some(socket) = &mut self.socket else {
            return;
        };
        let deadline = Instant::now() + time;
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            match tokio::time::timeout(left, socket.next()).await {
                Ok(Some(Ok(Message::Text(text)))) => {
                    self.messages
                        .push(serde_json::from_str(text.as_str()).unwrap_or(Value::Null));
                }
                Ok(Some(Ok(_))) => {}
                _ => break,
            }
        }
    }

    /// Reads until `done` holds, at most 20 seconds.
    pub async fn until(&mut self, done: impl Fn(&[Value]) -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !done(&self.messages) {
            if Instant::now() > deadline || self.socket.is_none() {
                return false;
            }
            self.read_for(Duration::from_millis(100)).await;
        }
        true
    }

    /// Reads message by message until `want` holds for a new one, at most 20 seconds. Gives the
    /// moment of arrival, which `until` (slices of 100 ms) does not.
    pub async fn next(&mut self, want: impl Fn(&Value) -> bool) -> bool {
        let Some(socket) = &mut self.socket else {
            return false;
        };
        let deadline = Instant::now() + Duration::from_secs(20);
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            match tokio::time::timeout(left, socket.next()).await {
                Ok(Some(Ok(Message::Text(text)))) => {
                    let message = serde_json::from_str(text.as_str()).unwrap_or(Value::Null);
                    let found = want(&message);
                    self.messages.push(message);
                    if found {
                        return true;
                    }
                }
                Ok(Some(Ok(_))) => {}
                _ => return false,
            }
        }
        false
    }

    /// The messages about these tasks, or of a kind that names no task (`jobs.changed`, `resync`).
    pub fn about(&self, tasks: &[&str]) -> Vec<Value> {
        self.messages
            .iter()
            .filter(|message| {
                let id = message["task"]["id"]
                    .as_str()
                    .or_else(|| message["taskId"].as_str());
                id.is_none_or(|id| tasks.contains(&id))
            })
            .cloned()
            .collect()
    }
}
