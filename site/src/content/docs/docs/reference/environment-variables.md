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
| `NATAMUX_REPO_DIR` | none | A git repository to add once as the first project, with the id `default`. Without it the daemon starts with no project. A project removed later does not come back |
| `NATAMUX_WORKTREES_DIR` | `<data dir>/worktrees` | Where worktrees go, one folder per task |
| `NATAMUX_WEB_DIR` | none | Override the dashboard embedded in the Rust daemon with files from this folder. The API stays under `/api` |
| `NATAMUX_CLAUDE_COMMAND` | `claude` | Program that starts Claude Code |
| `NATAMUX_CODEX_COMMAND` | `codex` | Program that starts Codex |
| `NATAMUX_GEMINI_COMMAND` | `gemini` | Program that starts Gemini CLI |
| `NATAMUX_COPILOT_COMMAND` | `copilot` | Program that starts Copilot CLI |
| `NATAMUX_CURSOR_COMMAND` | `cursor-agent` | Program that starts the Cursor CLI |
| `NATAMUX_CLAUDE_ARGS`, `NATAMUX_CODEX_ARGS`, `NATAMUX_GEMINI_ARGS`, `NATAMUX_COPILOT_ARGS`, `NATAMUX_CURSOR_ARGS` | none | Words added to that agent's command line |

See [Agents](/docs/agents/claude-code/) for how each agent is started. **Generate** in the commit box runs the same program, without the extra words.

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
| `NATAMUX_TOKEN` | The agent token. It may not approve, skip or pass a stage, or change settings. See [The agent token](/docs/reference/http-api/#the-agent-token) |
| `NATAMUX_URL` | `http://127.0.0.1:<daemon port>` |
| `NATAMUX_TASK_ID` | The task id, such as `T-42` |
| `NATAMUX_TERMINAL` | The terminal's name (`1`, `2`, ...). The agent runs in `1` |
| `NATAMUX_PORT` | The first port of the task's own block of 10 ports, from 20000 |
| `PORT` | The same value, for a dev server |
| `LANG` | `C.UTF-8`, only when the daemon has none of `LANG`, `LC_ALL` and `LC_CTYPE`. Without a UTF-8 locale, a shell garbles typed letters such as Vietnamese |

## Desktop app

The desktop app starts the daemon and sets these for it. It passes your other `NATAMUX_*` variables through.

| Variable | Value |
|---|---|
| `NATAMUX_HOST` | `127.0.0.1` |
| `NATAMUX_PORT` | 4317 when free, else a free port |
| `NATAMUX_TOKEN` | A token kept in the app's user data folder |
| `NATAMUX_DATA_DIR` | `<user data>/data` |
| `NATAMUX_REPO_DIR` | The repository an older version asked for, if any. Not set on a new install |
| `NATAMUX_WEB_DIR` | Optional override for the dashboard embedded in the daemon |
| `NATAMUX_PATH_PENDING` | `1` on macOS and Linux: the PATH of your login shell comes after the start. The daemon waits for it before it runs programs, and does not pass the variable on |

The Rust app uses `--smoke` and `--shot` for CI checks. `NATAMUX_UPDATE_AS_VERSION` overrides the running version for update tests.

## Docker

| Variable | Used by | Meaning |
|---|---|---|
| `NATAMUX_TOKEN` | `compose.prod.yaml` | Required. A long random value |
| `NATAMUX_REPO` | `compose.prod.yaml` | Required. The git repository to mount at `/repo` |
| `NATAMUX_BIND` | `compose.prod.yaml` | Address the dashboard port is published on. Default `127.0.0.1` |
| `NATAMUX_PORT` | `compose.prod.yaml` | Published dashboard port. Default `8080` |

The production image sets `NATAMUX_HOST=0.0.0.0`, `NATAMUX_DATA_DIR=/data`, `NATAMUX_REPO_DIR=/repo` and `NATAMUX_WORKTREES_DIR=/data/worktrees` inside the container. See [Self-host with Docker](/docs/guides/self-host-docker/).

The production compose file is `compose.prod.yaml`. Development builds run in Codespaces or CI.
