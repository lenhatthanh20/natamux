//! Stage logic of the Rust daemon (L1) that needs no daemon service: the checks of a stage, the
//! conditions of a branch, prompts, stage files, the git and gh calls of stages, the issue context
//! and the CI wait. The runner that uses them is in `natamux-daemon` (`src/stages/`).
//! Everything here is blocking: the runner runs on its own threads.

pub mod actions;
pub mod checks;
pub mod ci;
pub mod files;
pub mod gh;
pub mod git;
pub mod issue;
pub mod paths;
mod process;
pub mod prompts;
#[cfg(test)]
mod test_support;
