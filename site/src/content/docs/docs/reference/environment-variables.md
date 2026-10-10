---
title: Environment variables
description: Every NATAMUX_* variable that the daemon, the CLI, the desktop app and Docker read or set.
sidebar:
  order: 3
---

## Daemon

Set these in the environment where the daemon starts.

| Variable | Default | Meaning |
|---|---|---|
| `NATAMUX_PORT` | `4317` | Port. Must be 0 to 65535 |
| `NATAMUX_HOST` | `127.0.0.1` | Address to bind. Only a container sets it, with its port published to loopback |
| `NATAMUX_TOKEN` | A random token for each run, printed at start | The token every request needs |
| `NATAMUX_DATA_DIR` | `<current folder>/.natamux` | Task files and the SQLite index |
| `NATAMUX_REPO_DIR` | The current folder | The git repository tasks get worktrees from |
| `NATAMUX_WORKTREES_DIR` | `<data dir>/worktrees` | Where worktrees go, one folder per task |
| `NATAMUX_WEB_DIR` | none | The built dashboard. With it, the daemon serves the dashboard and the API under `/api` on its own port |
| `NATAMUX_WEB_INDEX` | `<web dir>/index.html` when `NATAMUX_WEB_DIR` is set | The dashboard's `index.html`, for deep links |
| `NATAMUX_CLAUDE_COMMAND` | `claude` | Program that starts Claude Code |
| `NATAMUX_CODEX_COMMAND` | `codex` | Program that starts Codex |
| `NATAMUX_GEMINI_COMMAND` | `gemini` | Program that starts Gemini CLI |
| `NATAMUX_COPILOT_COMMAND` | `copilot` | Program that starts Copilot CLI |
| `NATAMUX_CURSOR_COMMAND` | `cursor-agent` | Program that starts the Cursor CLI |
| `NATAMUX_CLAUDE_ARGS`, `NATAMUX_CODEX_ARGS`, `NATAMUX_GEMINI_ARGS`, `NATAMUX_COPILOT_ARGS`, `NATAMUX_CURSOR_ARGS` | none | Words added to that agent's command line |

See [Agents](/docs/agents/claude-code/) for how each agent is started.

## CLI

| Variable | Default | Meaning |
|---|---|---|
| `NATAMUX_TOKEN` | none | The daemon's token. Required, except for `hooks claude-code` |
| `NATAMUX_URL` | `http://127.0.0.1:4317` | The daemon's address |
| `NATAMUX_TASK_ID` | none | The task for `ask`, `done`, `task show` and `hook` |
| `NATAMUX_TERMINAL` | none | The terminal of the task |
| `NATAMUX_DEBUG` | none | `natamux hook` prints problems to stderr when set |

## Task terminals

Every terminal of a task gets the daemon's environment plus:

| Variable | Value |
|---|---|
| `NATAMUX_TOKEN` | The daemon's token |
| `NATAMUX_URL` | `http://127.0.0.1:<daemon port>` |
| `NATAMUX_TASK_ID` | The task id, such as `T-42` |
| `NATAMUX_TERMINAL` | The terminal's name (`1`, `2`, ...). The agent runs in `1` |
| `NATAMUX_PORT` | The first port of the task's own block of 10 ports, from 20000 |
| `PORT` | The same value, for a dev server |

## Desktop app

The desktop app starts the daemon and sets these for it. It passes your other `NATAMUX_*` variables through.

| Variable | Value |
|---|---|
| `NATAMUX_HOST` | `127.0.0.1` |
| `NATAMUX_PORT` | 4317 when free, else a free port |
| `NATAMUX_TOKEN` | A token kept in the app's user data folder |
| `NATAMUX_DATA_DIR` | `<user data>/data` |
| `NATAMUX_REPO_DIR` | The repository you picked |
| `NATAMUX_WEB_DIR` | The dashboard in the app's resources, unless you set it |

The desktop app also reads `NATAMUX_DESKTOP_SMOKE` and `NATAMUX_DESKTOP_SMOKE_SHOT`. They are for testing the app.

## Docker

| Variable | Used by | Meaning |
|---|---|---|
| `NATAMUX_TOKEN` | `compose.prod.yaml` | Required. A long random value |
| `NATAMUX_REPO` | `compose.prod.yaml` | Required. The git repository to mount at `/repo` |
| `NATAMUX_BIND` | `compose.prod.yaml` | Address the dashboard port is published on. Default `127.0.0.1` |
| `NATAMUX_PORT` | `compose.prod.yaml` | Published dashboard port. Default `8080` |

The production image sets `NATAMUX_HOST=0.0.0.0`, `NATAMUX_DATA_DIR=/data`, `NATAMUX_REPO_DIR=/repo` and `NATAMUX_WORKTREES_DIR=/data/worktrees` inside the container. See [Self-host with Docker](/docs/guides/self-host-docker/).

The development setup (`compose.yaml`) is for working on Natamux itself. It reads `NATAMUX_TOKEN` (default `devtoken`), `NATAMUX_CLAUDE_COMMAND`, `NATAMUX_UID`, `NATAMUX_GID`, `NATAMUX_GIT_NAME` and `NATAMUX_GIT_EMAIL`.
