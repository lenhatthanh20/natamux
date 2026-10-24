//! The git and GitHub CLI calls of the Rust daemon (L1), and the text work around them: ports of
//! `packages/daemon/src/git/*`. git and gh run as child processes (`std::process`), never through
//! a library. Everything here is blocking: callers run it on their own threads, never on the
//! HTTP reactor or the terminal output path.

pub mod branches;
pub mod changes;
pub mod commit_drafter;
pub mod commit_push;
pub mod exec;
pub mod file_diff;
pub mod gh;
pub mod graph;
pub mod process;
pub mod rollback;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
pub mod worktrees;
