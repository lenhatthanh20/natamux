//! `stage-runner.test.ts`, ported: one file per top `describe` group, and the fixture of its
//! `beforeEach`. A real git repository with a bare origin, worktrees made with `git worktree add`
//! (in place of `WorkspaceService.create`, which is part 5), fake ptys, a fake `gh`, a fake CI
//! check that waits for the test, and a clock the test moves.
//!
//! The runner works on threads of its own: `settle` waits for it, as Node's `runner.settle()`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// Each group uses a part of the helpers.
#![allow(dead_code)]
// `Fixture::new` is `beforeEach`; a `Default` would hide that it builds a repository.
#![allow(clippy::new_without_default)]

#[path = "../support/mod.rs"]
mod support;

mod fixture;
mod helpers;

pub use fixture::*;

mod action_source;
mod agent_review;
mod agent_stages;
mod attachments;
mod branch;
mod ci_check;
mod ci_testing;
mod context;
mod end_shell;
mod failed_side_work;
mod group;
mod group_skip;
mod issue_pipeline;
mod own_agent;
mod own_pipeline;
mod reopen;
mod report_other_step;
mod resume_command_ran;
mod resume_daemon_start;
mod resume_helpers;
mod resume_restart;
mod running_agent_prompt;
mod send_back;
mod settings_screen;
mod shells_come_back;
mod skills;
mod skip_marked;
mod stage_files_removed;
mod subtasks;
mod terminals_agents;
mod workflow;
