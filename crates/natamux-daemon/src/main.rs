//! `natamux-daemon`: the Rust daemon of phase 4. It runs only in tests and CI until phase 5.

use std::sync::Arc;

use natamux_daemon::config::load_config;
use natamux_daemon::shared::scheduler::Scheduler;
use natamux_daemon::start::start;
use natamux_daemon::terminals::login_path::{WAIT, wait_for_login_path};
use natamux_pty::{Env, spawn_shell};

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let mut env: Env = std::env::vars().collect();
    wait_for_login_path(&mut env, None, WAIT);
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
    stopped().await;
    // A second signal must not exit before the shells have ended: it is not listened to.
    let _ = tokio::task::spawn_blocking(move || daemon.shut_down()).await;
    std::process::ExitCode::SUCCESS
}

async fn stopped() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut terminate) => {
                tokio::select! {
                    _ = terminate.recv() => {}
                    _ = tokio::signal::ctrl_c() => {}
                }
            }
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
