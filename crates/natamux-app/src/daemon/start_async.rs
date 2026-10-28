//! Starts the app's own daemon on a thread while the window already shows its skeletons
//! (T-0221 1.1), and watches it from a thread so the main thread does not wake for it (9.1).

use std::io;
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use futures::channel::oneshot;
use gpui::{App, WindowHandle};
use natamux_client::{Client, Config};

use super::process::{DaemonSlot, OwnedDaemon, Slot};
use crate::app::source::Source;
use crate::app::update_install::request_restart;
use crate::app::window::Root;
use crate::components::toast_rules::ToastTone;
use crate::components::toasts::{ToastAction, push_toast};

/// A thread asks this often whether the daemon still runs.
const WATCH_EVERY: Duration = Duration::from_secs(1);

/// What the start thread hands to the main thread: the daemon's address and its process, or
/// why it did not start.
type Answer = Result<(Config, Arc<Mutex<Child>>), String>;

/// The app's own daemon while it starts. Err: its thread could not start.
pub type Started = io::Result<oneshot::Receiver<Answer>>;

fn lock<T>(slot: &Mutex<Slot<T>>) -> MutexGuard<'_, Slot<T>> {
    slot.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Starts the daemon on its own thread; `follow` takes the answer on the main thread.
pub fn begin(slot: Arc<Mutex<DaemonSlot>>) -> Started {
    start_on_thread(OwnedDaemon::start, slot, facts)
}

fn facts(daemon: &OwnedDaemon) -> (Config, Arc<Mutex<Child>>) {
    (daemon.config(), daemon.child())
}

/// Runs `start` on a thread and puts its value in `slot`. The answer carries `facts` of the
/// value, or the error's text. No answer when the slot closed first: the value was dropped.
pub(super) fn start_on_thread<T, R>(
    start: impl FnOnce() -> io::Result<T> + Send + 'static,
    slot: Arc<Mutex<Slot<T>>>,
    facts: impl FnOnce(&T) -> R + Send + 'static,
) -> io::Result<oneshot::Receiver<Result<R, String>>>
where
    T: Send + 'static,
    R: Send + 'static,
{
    let (sender, receiver) = oneshot::channel();
    std::thread::Builder::new()
        .name("natamux-daemon-start".into())
        .spawn(move || {
            let answer = match start() {
                Ok(value) => {
                    let known = facts(&value);
                    let late = lock(&slot).put(value);
                    if late.is_some() {
                        // Dropped here, outside the lock: an OwnedDaemon stops its process.
                        return;
                    }
                    Ok(known)
                }
                Err(error) => Err(error.to_string()),
            };
            let _ = sender.send(answer);
        })?;
    Ok(receiver)
}

/// On the main thread: the window follows the daemon once it answers, or says why it did not
/// start. `smoke`: a failed start ends the `--smoke` run with this flag set.
pub fn follow(
    started: Started,
    window: WindowHandle<Root>,
    slot: Arc<Mutex<DaemonSlot>>,
    smoke: Option<Arc<AtomicBool>>,
    cx: &mut App,
) {
    cx.spawn(async move |cx| {
        let answer = match started {
            Ok(receiver) => match receiver.await {
                Ok(answer) => answer,
                // The app quit first; the start thread stopped the daemon.
                Err(oneshot::Canceled) => return,
            },
            Err(error) => Err(error.to_string()),
        };
        let failed = smoke.as_deref();
        cx.update(|cx| take_answer(answer, window, slot, failed, cx));
    })
    .detach();
}

fn take_answer(
    answer: Answer,
    window: WindowHandle<Root>,
    slot: Arc<Mutex<DaemonSlot>>,
    smoke: Option<&AtomicBool>,
    cx: &mut App,
) {
    let source = answer.and_then(|(config, child)| {
        watch_daemon(child, slot, cx);
        Client::new(config)
            .map(|client| Arc::new(Source::Daemon(Arc::new(client))))
            .map_err(|error| error.to_string())
    });
    let shown = match source {
        Ok(source) => window.update(cx, |root, _, cx| root.use_source(source, cx)),
        Err(reason) => {
            if let Some(failed) = smoke {
                eprintln!("smoke: the app could not start its daemon: {reason}");
                failed.store(true, Ordering::Relaxed);
                cx.quit();
                return;
            }
            window.update(cx, |root, _, cx| {
                root.dashboard()
                    .update(cx, |dashboard, cx| dashboard.start_failed(reason, cx))
            })
        }
    };
    if let Err(error) = shown {
        eprintln!("natamux-app: the window is gone: {error}");
    }
}

/// Toasts once when the daemon stops while the app runs.
fn watch_daemon(child: Arc<Mutex<Child>>, slot: Arc<Mutex<DaemonSlot>>, cx: &mut App) {
    let quitting = move || lock(&slot).is_closed();
    let exited = match watch_child(child, WATCH_EVERY, quitting) {
        Ok(exited) => exited,
        Err(error) => {
            eprintln!("natamux-app: cannot watch the daemon: {error}");
            return;
        }
    };
    cx.spawn(async move |cx| {
        // Canceled: the app quits, or the watcher stopped.
        if exited.await.is_ok() {
            cx.update(|cx| {
                push_toast(
                    "The Natamux daemon stopped.",
                    ToastTone::Error,
                    Some("Restart the app to reconnect.".into()),
                    vec![ToastAction::new("Restart", |_, cx| request_restart(cx))],
                    cx,
                );
            });
        }
    })
    .detach();
}

/// Sends once when `child` exits. Ends without a send when `quitting` says the app quits. The
/// child is locked only to ask, never while the thread sleeps: OwnedDaemon's drop takes it.
pub(super) fn watch_child(
    child: Arc<Mutex<Child>>,
    every: Duration,
    quitting: impl Fn() -> bool + Send + 'static,
) -> io::Result<oneshot::Receiver<()>> {
    let (sender, receiver) = oneshot::channel();
    std::thread::Builder::new()
        .name("natamux-daemon-watch".into())
        .spawn(move || {
            loop {
                std::thread::sleep(every);
                if quitting() {
                    return;
                }
                let exited = child.lock().map_or(true, |mut process| {
                    process.try_wait().ok().flatten().is_some()
                });
                if exited {
                    if !quitting() {
                        let _ = sender.send(());
                    }
                    return;
                }
            }
        })?;
    Ok(receiver)
}
