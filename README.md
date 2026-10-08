# Helmsman

The next generation of an ADE.

Run many coding agents in parallel, manage the work as tasks, and see what needs you on one dashboard.

[![Open in GitHub Codespaces](https://github.com/codespaces/badge.svg)](https://codespaces.new/lenhatthanh20/helmsman)

## Try it

In a codespace:

```sh
scripts/dev.sh demo
```

Open port 5173 and sign in with the token `devtoken`. The demo task has a workspace with commits and changes. See [docs/docker.md](docs/docker.md).

For tasks in every state with agents at work, run `scripts/dev.sh seed`. It starts the daemon with `scripts/fake-agent.sh` in place of Claude Code, which is not in the dev image. The next `scripts/dev.sh up` goes back to `claude`.

## What it does

- **Tasks**: draft → ready → running → needs input / review → done, with an activity log. Edit notes, priority, labels; subtasks; dependencies; templates; import open GitHub issues.
- **Dashboard**: a "Needs me" strip first, a table or a board, live updates, system notifications, keyboard first (`j`/`k`, Enter, `a`, `r`, `/`, `n`).
- **Workspaces**: one git worktree and branch per task, from any local branch, in any added repository. A setup script (`.helmsman-setup`) runs in new ones; each task gets its own ports.
- **Agents**: **Start Claude Code** in the task's terminal, with hooks that move the task between states by themselves. Agents without hooks are followed by their output and bell, and can report with `helmsman ask` / `helmsman done`. **Fan out** a task to several agents and keep the best try.
- **Review**: changed files with a Monaco diff, the branch history as a graph, review comments sent to the agent as its next prompt, commit, push and pull request, CI status, done when the pull request is merged.
- **Terminals**: several shells per task, kept running when the dashboard closes.

## Parts

| Package | What |
|---|---|
| `packages/core` | Task model, state machine, events, shared schemas |
| `packages/daemon` | HTTP and WebSocket API, task files, git, terminals, GitHub ([API](packages/daemon/README.md)) |
| `packages/web` | The dashboard (React) |
| `packages/cli` | `helmsman` command line ([usage](packages/cli/README.md)) |
| `plugins/helmsman` | Claude Code plugin with a skill for working on a task |

## Docs

- Plans: [features](docs/plan-features.md), [tech stack](docs/plan-tech-stack.md)
- Setup: [dev setup](docs/dev-setup.md), [Docker](docs/docker.md)
- Agents: [adapters](docs/agent-adapters.md), [Claude Code](docs/claude-code-adapter.md)
- [MVP checks](docs/mvp-checks.md)

Status: MVP in progress. Runs in a browser tab; the desktop app comes later.
