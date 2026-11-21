---
title: Environment variables
description: Every NATAMUX_* variable that the daemon, the CLI and the desktop app read or set.
sidebar:
  order: 3
---

## Daemon

Set these in the environment where the daemon starts.

| Variable | Default | Meaning |
|---|---|---|
| `NATAMUX_PORT` | `4317` | Port. Must be 0 to 65535 |
| `NATAMUX_HOST` | `127.0.0.1` | Address to bind. Only a container sets it, with its port published to loopback |
| `NATAMUX_ALLOWED_HOSTS` | none | Comma list of host names a request may use in its `Host` header, besides `127.0.0.1`, `localhost`, `::1` and `NATAMUX_HOST`. Names only, no ports. Other hosts get 403. A WebSocket `Origin` must use an allowed name too |
| `NATAMUX_TOKEN` | A random token for each run, printed at start | The token every request needs |
| `NATAMUX_OFFLINE` | none | `1` turns on [offline mode](/docs/help/security-review/#offline-mode): no update check, no `gh`, and no `git` command that reaches a remote. The desktop app reads it too, and passes it to the daemon it starts. The settings file of the machine turns it on for every user |
| `NATAMUX_DATA_DIR` | `<current folder>/.natamux` | Task files and the SQLite index |
| `NATAMUX_REPO_DIR` | none | A git repository to add once as the first project, with the id `default`. Without it the daemon starts with no project. A project removed later does not come back |
| `NATAMUX_WORKTREES_DIR` | `<data dir>/worktrees` | Where worktrees go, one folder per task |
| `NATAMUX_INPUT_TRACE` | none | `1` writes every key sent to a terminal to the daemon log, for finding typing problems such as lost Vietnamese letters. It logs passwords too: turn it off after use |
| `NATAMUX_CLAUDE_COMMAND` | `claude` | Program that starts Claude Code |
| `NATAMUX_CODEX_COMMAND` | `codex` | Program that starts Codex |
| `NATAMUX_GEMINI_COMMAND` | `gemini` | Program that starts Gemini CLI |
| `NATAMUX_COPILOT_COMMAND` | `copilot` | Program that starts Copilot CLI |
| `NATAMUX_CURSOR_COMMAND` | `cursor-agent` | Program that starts the Cursor CLI |
| `NATAMUX_OPENCODE_COMMAND` | `opencode` | Program that starts OpenCode |
| `NATAMUX_QWEN_COMMAND` | `qwen` | Program that starts Qwen Code |
| `NATAMUX_DROID_COMMAND` | `droid` | Program that starts Factory Droid |
| `NATAMUX_AMP_COMMAND` | `amp` | Program that starts Amp |
| `NATAMUX_KIMI_COMMAND` | `kimi` | Program that starts Kimi |
| `NATAMUX_GOOSE_COMMAND` | `goose` | Program that starts Goose |
| `NATAMUX_AIDER_COMMAND` | `aider` | Program that starts Aider |
| `NATAMUX_CRUSH_COMMAND` | `crush` | Program that starts Crush |
| `NATAMUX_CLINE_COMMAND` | `cline` | Program that starts Cline |
| `NATAMUX_KIRO_COMMAND` | `kiro-cli` | Program that starts Kiro |
| `NATAMUX_MUSE_COMMAND` | `muse` | Program that starts Muse Code |
| `NATAMUX_CLAUDE_ARGS`, `NATAMUX_CODEX_ARGS`, `NATAMUX_GEMINI_ARGS`, `NATAMUX_COPILOT_ARGS`, `NATAMUX_CURSOR_ARGS`, `NATAMUX_OPENCODE_ARGS`, `NATAMUX_QWEN_ARGS`, `NATAMUX_DROID_ARGS`, `NATAMUX_AMP_ARGS`, `NATAMUX_KIMI_ARGS`, `NATAMUX_GOOSE_ARGS`, `NATAMUX_AIDER_ARGS`, `NATAMUX_CRUSH_ARGS`, `NATAMUX_CLINE_ARGS`, `NATAMUX_KIRO_ARGS`, `NATAMUX_MUSE_ARGS` | none | Words added to that agent's command line |

See [Agents](/docs/agents/claude-code/) for how each agent is started. **Generate** in the commit box runs the same program, without the extra words.

## CLI

| Variable | Default | Meaning |
|---|---|---|
| `NATAMUX_TOKEN` | The desktop app's saved token, if present | The daemon's token. Set it for another daemon; `hooks claude-code` does not need one |
| `NATAMUX_URL` | The desktop app's saved daemon port, then `http://127.0.0.1:4317` | The daemon's address |
| `NATAMUX_TASK_ID` | none | The task for `ask`, `done`, `task show` and `hook` |
| `NATAMUX_TERMINAL` | none | The terminal of the task |
| `NATAMUX_DEBUG` | none | `natamux hook` prints problems to stderr when set |

## Task terminals

Every terminal of a task gets the daemon's environment plus:

| Variable | Value |
|---|---|
| `NATAMUX_TOKEN` | The task's own agent token. It reaches only this task, and may not approve, skip or pass a stage, or change settings. See [The agent token](/docs/reference/http-api/#the-agent-token) |
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
| `NATAMUX_PATH_PENDING` | `1` on macOS and Linux: the PATH of your login shell comes after the start. The daemon serves at once and waits for it only before it runs a program, and does not pass the variable on |

The Rust app uses `--smoke` and `--shot` for CI checks. `NATAMUX_UPDATE_AS_VERSION` overrides the running version for update tests.

## Test stack in Docker

`compose.prod.yaml` runs the daemon alone for CI and codespace tests. It is not a supported self-host: the images hold no coding agent and have no UI. It reads `NATAMUX_TOKEN`, `NATAMUX_REPO`, `NATAMUX_BIND` (default `127.0.0.1`), `NATAMUX_PORT` (default `8080`) and `NATAMUX_ALLOWED_HOSTS` (default `daemon`). See `docs/docker.md` in the repository.
