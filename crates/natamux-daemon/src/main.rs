//! The Natamux daemon used by the app, CLI, and browser dashboard.

use std::io::Read;
use std::sync::Arc;

use natamux_daemon::config::load_config;
use natamux_daemon::shared::scheduler::Scheduler;
use natamux_daemon::start::start;
use natamux_daemon::terminals::login_path::{WAIT, read_path_message, wait_for_login_path};
use natamux_pty::{Env, spawn_shell};

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let mut env: Env = std::env::vars().collect();
    let app_pipe = env.remove("NATAMUX_PARENT_PIPE").as_deref() == Some("1");
    let (path_pipe, parent_end) =
        if env.get("NATAMUX_PATH_PENDING").map(String::as_str) == Some("1") {
            let (send, receive) = std::sync::mpsc::channel();
            let (end_send, end_receive) = tokio::sync::oneshot::channel();
            std::thread::spawn(move || {
                read_path_message(std::io::stdin().lock(), send, end_send);
            });
            (Some(receive), Some(end_receive))
        } else if app_pipe {
            let (end_send, end_receive) = tokio::sync::oneshot::channel();
            std::thread::spawn(move || {
                read_parent_end(std::io::stdin().lock(), end_send);
            });
            (None, Some(end_receive))
        } else {
            (None, None)
        };
    wait_for_login_path(&mut env, path_pipe.as_ref(), WAIT);
    let cwd = std::env::current_dir().unwrap_or_default();
    let config = match load_config(&env, &cwd) {
        Ok(config) => config,
        Err(message) => {
            eprintln!("{message}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let daemon = match start(&config, &env, Arc::new(spawn_shell), Scheduler::real()).await {
        Ok(daemon) => daemon,
        Err(error) => {
            eprintln!("The daemon could not start: {error}");
            return std::process::ExitCode::FAILURE;
        }
    };
    println!(
        "Natamux daemon listening on http://{}:{}",
        config.host,
        daemon.address.port()
    );
    println!("Data directory: {}", config.data_dir.display());
    if config.token_was_generated {
        println!("Token for this run: {}", config.token);
    }
    stopped(parent_end).await;
    // A second signal must not exit before the shells have ended: it is not listened to.
    let _ = tokio::task::spawn_blocking(move || daemon.shut_down()).await;
    std::process::ExitCode::SUCCESS
}

fn read_parent_end(mut input: impl Read, ended: tokio::sync::oneshot::Sender<()>) {
    let _ = std::io::copy(&mut input, &mut std::io::sink());
    let _ = ended.send(());
}

async fn stopped(parent_end: Option<tokio::sync::oneshot::Receiver<()>>) {
    let parent_ended = async move {
        if let Some(receiver) = parent_end {
            let _ = receiver.await;
        } else {
            std::future::pending::<()>().await;
        }
    };
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut terminate) => {
                tokio::select! {
                    _ = terminate.recv() => {}
                    _ = tokio::signal::ctrl_c() => {}
                    _ = parent_ended => {}
                }
            }
            Err(_) => {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {}
                    _ = parent_ended => {}
                }
            }
        }
    }
    #[cfg(not(unix))]
    {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = parent_ended => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::read_parent_end;

    #[test]
    fn parent_pipe_eof_sends_the_shutdown_signal() {
        let (ended, mut receiver) = tokio::sync::oneshot::channel();
        read_parent_end(std::io::Cursor::new(b""), ended);
        assert!(receiver.try_recv().is_ok());
    }
}
