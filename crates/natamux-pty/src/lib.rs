//! Pseudo terminals for the daemon's shells (L1): spawn a login shell, read and write its bytes,
//! resize it, and end it with everything that runs in it. Unix uses `openpty` directly, so the
//! shell gets exactly the environment given, as with node-pty; Windows uses ConPTY through
//! `alacritty_terminal`. Ported from `packages/daemon/src/terminals/pty.ts` and its helpers.

pub mod da1;
pub mod input_gap;
pub mod no_proxy;
pub mod process_tree;
mod pty;
pub mod shell_command;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

pub use pty::{Env, Pty, PtyEvents, Signal, SpawnOptions, spawn_shell};
