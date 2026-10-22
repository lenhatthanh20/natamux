//! The `natamux` command (also `nmx`) in Rust, phase 2 of docs/plan-rust-migration.md: every
//! command of packages/cli, with the same output, exit codes and hook forwarder.

mod args;
pub mod command;
pub mod daemon;
mod execute;
pub mod format;
mod hook;
mod hooks;
pub mod run;

use std::io::{IsTerminal, Read, Write};
use std::process::ExitCode;

use daemon::HttpTransport;
use run::{Env, Io, run_cli};

/// The terminal's streams. A closed pipe (`natamux task list | head -1`) is not an error.
struct ProcessIo;

impl Io for ProcessIo {
    fn stdout(&mut self, text: &str) {
        let mut out = std::io::stdout().lock();
        let _ = out.write_all(text.as_bytes()).and_then(|()| out.flush());
    }

    fn stderr(&mut self, text: &str) {
        let _ = std::io::stderr().lock().write_all(text.as_bytes());
    }
}

/// All of stdin, or nothing when the command is run by hand in a terminal.
fn read_stdin() -> Vec<u8> {
    let mut stdin = std::io::stdin();
    if stdin.is_terminal() {
        return Vec::new();
    }
    let mut bytes = Vec::new();
    let _ = stdin.read_to_end(&mut bytes);
    bytes
}

pub fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            // The hook runs inside an agent: it never fails it.
            if argv.first().map(String::as_str) == Some("hook") {
                return ExitCode::SUCCESS;
            }
            eprintln!("Error: cannot start: {error}");
            return ExitCode::FAILURE;
        }
    };
    let env = Env::from_process();
    let code = runtime.block_on(run_cli(
        &argv,
        &env,
        &mut ProcessIo,
        &read_stdin,
        &HttpTransport::new,
    ));
    ExitCode::from(code)
}
