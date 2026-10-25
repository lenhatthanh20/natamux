//! Agents of the Rust daemon (L1): how each agent is started (`agent-launch.ts`), the hook files
//! its start writes, workspace trust, and the skills an agent finds (`agent-skills.ts`). Part 4
//! adds the adapters that read the hooks, and usage.

pub mod git_files;
pub mod hook_files;
pub mod hooks;
pub mod launch;
mod paths;
pub mod skill_list;
pub mod skill_resolve;
pub mod skills;
#[cfg(test)]
mod test_support;
pub mod trust;
pub mod usage;
