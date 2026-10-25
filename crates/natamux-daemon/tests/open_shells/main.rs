//! `open-shells.test.ts`, ported: the groups 'OpenShells' and 'OpenShells across restarts'. The
//! 'the open-shells file' group is in `tests/shell_files.rs`. Node's async `agentIn` is a plain
//! function that runs on a scheduler job, so `advance(0)` stands for the `await`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod basic;
mod fixture;
mod programs;
mod restarts;
