//! Stage logic of the Rust daemon (L1) that needs no daemon service: the checks of a stage, the
//! conditions of a branch, prompts, stage files, the git calls of stages (processes and `gh` are
//! in `natamux-git`), the issue context and the CI wait. The runner that uses them is in
//! `natamux-daemon` (`src/stages/`).
//! Everything here is blocking: the runner runs on its own threads.

pub mod actions;
pub mod checks;
pub mod ci;
pub mod ci_check;
pub mod files;
pub mod git;
pub mod issue;
pub mod paths;
pub mod prompts;
