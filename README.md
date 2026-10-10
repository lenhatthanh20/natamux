# Natamux

The next generation of an ADE.

Run many coding agents in parallel, manage the work as tasks, and see what needs you on one dashboard.

[![Open in GitHub Codespaces](https://github.com/codespaces/badge.svg)](https://codespaces.new/lenhatthanh20/natamux)

## Try it

In a codespace:

```sh
scripts/dev.sh demo
```

Open port 5173 and sign in with the token `devtoken`. The demo task has a workspace with commits and changes. See [docs/docker.md](docs/docker.md).

For tasks in every state with agents at work, run `scripts/dev.sh seed`. It starts the daemon with `scripts/fake-agent.sh` in place of Claude Code, which is not in the dev image. The next `scripts/dev.sh up` goes back to `claude`.

## What it does

- **Tasks**: draft → ready → running → needs input / review → done, with an activity log. Edit notes, priority, labels; subtasks; dependencies; templates; import open GitHub issues.
- **Stages**: each task goes through Context → Spec → Plan → [Test spec ‖ Implement] → Review → Testing → Release, set per repository in `.natamux.json`. Agents start each stage, report it with `natamux done`, the daemon checks the result, and you approve where the gate is manual. Context comes from your notes, a GitHub issue, a skill or an MCP server; approving the spec can open a GitHub issue, and the pull request closes it. See [docs/design-task-stages.md](docs/design-task-stages.md).
- **Dashboard**: a "Needs me" strip first, a table, a board or a progress view of every task against its stages, live updates, system notifications, keyboard first (`j`/`k`, Enter, `a`, `r`, `/`, `n`). Each task has a Progress tab with the evidence of each stage, and a Docs tab with its stage files.
- **Workspaces**: one git worktree and branch per task, from any local branch, in any added repository. A setup script (`.natamux-setup`) runs in new ones; each task gets its own ports.
- **Agents**: **Start Claude Code** in the task's terminal, with hooks that move the task between states by themselves. Agents without hooks are followed by their output and bell, and can report with `natamux ask` / `natamux done`. **Fan out** a task to several agents and keep the best try.
- **Review**: changed files with a Monaco diff, the branch history as a graph, review comments sent to the agent as its next prompt, commit, push and pull request, CI status, done when the pull request is merged.
- **Terminals**: several shells per task, kept running when the dashboard closes.

## Parts

| Package | What |
|---|---|
| `packages/core` | Task model, state machine, events, shared schemas |
| `packages/daemon` | HTTP and WebSocket API, task files, git, terminals, GitHub ([API](packages/daemon/README.md)) |
| `packages/web` | The dashboard (React) |
| `packages/cli` | `natamux` command line ([usage](packages/cli/README.md)) |
| `plugins/natamux` | Claude Code plugin with a skill for working on a task |

## Docs

- Plans: [features](docs/plan-features.md), [tech stack](docs/plan-tech-stack.md)
- Setup: [dev setup](docs/dev-setup.md), [Docker](docs/docker.md)
- Agents: [adapters](docs/agent-adapters.md), [Claude Code](docs/claude-code-adapter.md)
- [MVP checks](docs/mvp-checks.md)

Status: MVP checks pass (`docs/mvp-checks.md`). Runs in a browser tab, or in the desktop app (`packages/desktop`, not packaged yet).
