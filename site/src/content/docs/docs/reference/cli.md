---
title: CLI reference
description: Every natamux command and option, with exit codes and environment variables.
sidebar:
  order: 1
---

The command is `natamux`. The short name `nmx` is the same program.

```text
natamux <command> [options]
nmx <command> [options]
```

Run `natamux help`, `natamux --help` or `natamux -h` to print the usage text.

Task ids look like `T-42`. Inside a Natamux terminal, the token, the daemon address and the task are already set. There the token is the task's own: it reaches only that task, so the gate commands (`stage approve`, `stage skip`, `stage pass`) and commands on another task get 403. Run them in the app, or in a terminal outside a task. Outside one, the Rust CLI reads the running desktop app's saved token and port when available. Set `NATAMUX_TOKEN` and `NATAMUX_URL` for a different daemon (see [Environment](#environment)).

## Commands

| Command | What it does |
|---|---|
| `task create <title>` | Create a draft task |
| `task list` | List tasks, newest update first |
| `task show [id]` | Show one task |
| `task edit <id>` | Change the title, notes, priority, labels or dependencies |
| `task link <id>` | Link the task to its issue or ticket |
| `agent start <id>` | Start an agent in the task's terminal |
| `ask <question>` | For an agent: say the task waits for you |
| `done` | For an agent: say the stage is done, so the daemon checks it |
| `stage <command> <id>` | `approve`, `back`, `skip`, `pass`, `fail` or `start` the task's stage |
| `stage workflow <id> on\|off` | Run a stage as a workflow the next time it starts (needs `--stage`) |
| `stage ci <id> on\|off` | Check a stage on CI before it passes (needs `--stage`) |
| `event <id> <type>` | Send an event to a task |
| `events <id>` | Show the event log of a task |
| `workspace create <id>` | Create a git worktree and a branch for a task |
| `workspace remove <id>` | Remove the worktree of a task (needs `--yes`) |
| `repo trust <repo>` | Show the commands a project runs, or approve them (`--approve`) |
| `hooks claude-code` | Print Claude Code settings that report a task's progress |
| `hook <agent> [event]` | For an agent's hook: send the JSON on stdin to the daemon |
| `help` | Show the help |

## Options

| Option | Used by | Meaning |
|---|---|---|
| `--title <text>` | `task edit` | New title |
| `--description <text>` | `task create`, `task edit` | Notes |
| `--priority <name>` | `task create`, `task edit` | `low`, `normal`, `high` or `urgent` |
| `--label <name>` | `task create`, `task edit` | A label. Repeat for more |
| `--agent <name>` | `task create`, `agent start` | The agent that works on the task, or the agent to start (default for `agent start`: `claude-code`) |
| `--depends-on <id>` | `task create`, `task edit` | A task that must finish first. Repeat for more |
| `--parent <id>` | `task create` | Make the task a subtask of this task. A subtask cannot have subtasks. Without `--repo`, it is in the project of the parent |
| `--context <source>` | `task create` | Where the context comes from: `manual`, `github-issue`, `skill` or `mcp`, as `source` or `source:ref`, such as `github-issue:42` |
| `--repo <id>` | `task create` | The project of the task. Default: the project of the `NATAMUX_TASK_ID` task, or the only project |
| `--attach <path>` | `task create` | A file for the agents to read, such as a document or a screenshot. Repeat for more. At most 20 files, 20 MB each and 50 MB together |
| `--prompt <text>` | `agent start` | First prompt. Default: the task title and notes |
| `--task <id>` | `ask`, `done` | The task. Default: `NATAMUX_TASK_ID` |
| `--stage <id>` | `stage`, `done`, `ask` | The stage, when two run side by side. For `stage workflow` and `stage ci`: the stage to change |
| `--to <id>` | `stage back` | The stage to go back to. Default: the last agent stage |
| `--reason <text>` | `stage back`, `stage fail` | Why, for the agent and the log |
| `--issue <key>` | `task link` | The issue or ticket, such as `#42` or `ABC-123` |
| `--url <address>` | `task link` | Its address |
| `--provider <name>` | `task link` | `github`, `jira` and so on. Default: `github` when the address contains `github.com`, else `tracker` |
| `--state <state>` | `task list` | Only tasks in this state: `draft`, `ready`, `running`, `needs-input`, `review`, `blocked`, `failed`, `done`, `cancelled` |
| `--source <name>` | `event` | `agent`, `human`, `git`, `ci` or `system`. Default: `human`. In a task terminal the daemon records `agent`, whatever you pass |
| `--payload <json>` | `event` | A JSON object sent with the event |
| `--base <branch>` | `workspace create`, `agent start` | Start the new workspace from this local branch. Default: the default branch of the repository, such as `main`. Natamux fetches the base first (see [Base branch](/docs/concepts/workspaces/#base-branch)) |
| `--branch <name>` | `workspace create`, `agent start` | Name the branch of the new workspace. A local branch of that name is checked out, `--base` is ignored. A branch only on `origin` becomes a local branch that tracks it. See [Branch name](/docs/concepts/workspaces/#branch-name) |
| `--yes` | `workspace remove` | Confirm that the worktree folder is deleted |
| `--delete-branch` | `workspace remove` | Also delete the branch, when it is merged |
| `--approve <hash>` | `repo trust` | Approve the commands with this hash, as the list printed it |
| `--json` | most commands | Print JSON instead of text |
| `-h`, `--help` | all | Show the help |

## task

```sh
natamux task create "Fix the login bug" --priority high --label bug
natamux task create "Add export" --depends-on T-41 --context github-issue:42
natamux task create "Update the docs" --repo my-app
natamux task create "Part A" --parent T-42
natamux task create "ABC-12: Fix the login bug"
natamux task list --state running
natamux task show T-42
natamux task edit T-42 --title "Fix the login redirect" --label auth
natamux task link T-42 --issue ABC-123 --url https://example.com/browse/ABC-123
```

- `task create` needs a title. All words after `create` form the title.
- Without `--repo`, `task create` uses the project of the task in `NATAMUX_TASK_ID`. Outside a task terminal it uses the only project. With more than one project, it fails and lists the project ids.
- `task edit` needs at least one of `--title`, `--description`, `--priority`, `--label` or `--depends-on`.
- `task show` without an id shows the task of this terminal (`NATAMUX_TASK_ID`).
- `task link` needs `--issue` and `--url`.

## agent start

```sh
natamux agent start T-42
natamux agent start T-42 --agent codex --prompt "Add tests for the parser"
natamux agent start T-42 --base develop
```

The agent is one of `claude-code`, `codex`, `gemini`, `copilot` or `cursor`. If the task has no workspace, `agent start` creates one first, from `--base` when you give it. See [Agents and hooks](/docs/concepts/agents-and-hooks/).

## ask and done

Run these from an agent, inside a task's terminal.

```sh
natamux ask "Keep the old CSV columns, or drop them?"
natamux done
natamux done --stage implement
```

- `ask` sends an `agent.asked` event with your question. The task moves to needs input.
- `done` reports that the stage is done. The daemon then runs the stage's checks.
- Without `--task`, the task comes from `NATAMUX_TASK_ID`. If neither is set, the command exits with code 2.

## stage

```sh
natamux stage approve T-42
natamux stage back T-42 --to spec --reason "The spec misses the error case"
natamux stage fail T-42 --reason "Tests do not pass"
```

`back` goes to the stage named with `--to`. It cannot be used with `--stage`. See [Stages](/docs/concepts/stages/).

`skip` skips the current stage. Marking a stage that the task has not reached yet, so it is skipped when the task gets there (**Skip** and **Will be skipped** in the dashboard), is not a command. Use the dashboard or `PUT /tasks/:id/skip` in the [HTTP API](/docs/reference/http-api/#main-endpoints). The API action `reopen` has no command either. `back` is the command that sends a task to an earlier stage.

```sh
natamux stage workflow T-42 on --stage implement
natamux stage workflow T-42 off --stage implement
```

`workflow` chooses whether a stage runs as a workflow the next time it starts. The project settings must offer it on the stage, and the agent of the task must be Claude Code. Turning it off always works. See [Run a stage as a workflow](/docs/concepts/stages/#run-a-stage-as-a-workflow).

```sh
natamux stage ci T-42 on --stage implement
```

`ci` chooses whether a stage waits for CI before it passes: Natamux pushes the work to `natamux-ci/<branch>` and waits for its runs. The project settings must offer it on the stage. See [Check a stage on CI](/docs/concepts/stages/#check-a-stage-on-ci).

## event and events

```sh
natamux event T-42 agent.asked --source agent --payload '{"question":"Which database?"}'
natamux events T-42
```

Event types you can send:

`task.readied`, `task.blocked`, `task.unblocked`, `task.retried`, `task.cancelled`, `agent.started`, `agent.asked`, `agent.resumed`, `agent.finished`, `agent.failed`, `agent.waiting`, `review.approved`, `review.changes-requested`, `state.overridden`, `pr.merged`, `stage.started`, `stage.reported`, `stage.check-failed`, `stage.checked`, `stage.approved`, `stage.passed`, `stage.failed`, `stage.reopened`, `stage.skipped`.

The daemon records the other event types itself. An event that does not fit the task's state is logged as not accepted, and the task does not change.

## workspace

```sh
natamux workspace create T-42 --base main
natamux workspace remove T-42 --yes --delete-branch
```

`workspace remove` deletes the worktree folder, so it needs `--yes`. Git refuses when the worktree has uncommitted files.

## repo trust

```sh
natamux repo trust my-app
natamux repo trust my-app --approve 3f1c…
```

A project's commands (stage checks, command stages, path conditions, stage `args` and the setup script) run only after you approve them. See [Approve the project's commands](/docs/guides/project-settings/#approve-the-projects-commands).

- Without `--approve`, it prints each command with where it comes from, the setup scripts, and the hash.
- With `--approve <hash>`, it approves them. The daemon refuses a hash that is no longer current: the commands changed since you read them, so read them again.
- It needs the daemon's own token. The token of a task terminal gets 403.

## hooks and hook

```sh
claude --settings "$(natamux hooks claude-code)"
natamux hook codex
natamux hook copilot agentStop
```

- `hooks claude-code` prints settings JSON. It needs no token and does not call the daemon. Use it only outside a task terminal, or when the worktree's `.claude/settings.local.json` is your own: a task terminal already has the hooks, and both would send each event twice.
- `hook <agent> [event]` is for an agent's own hook. It reads JSON on stdin and posts it to the daemon. It prints nothing (`{}` for `cursor` and `gemini`), always exits 0, and does nothing without `NATAMUX_TOKEN`. Even a usage mistake exits 0, because agents treat exit code 2 as "block this action". See [Other agents](/docs/agents/other-agents/).

## JSON output

Add `--json` to print JSON. Tasks come as task objects, `task list` as an array, `events` as an array of stored events, `event` and `ask` as `{ task, accepted }`, `workspace remove` as `{ task, branchDeleted }`.

```sh
natamux task list --json
```

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Done |
| 1 | The request failed. The error is on stderr |
| 2 | The command line is wrong, a task id is missing, or `NATAMUX_TOKEN` is not set |

## Environment

| Variable | Meaning |
|---|---|
| `NATAMUX_TOKEN` | The daemon's token. The CLI reads the app's saved token when it is available. Required for a separate daemon, except for `hooks claude-code`. In a task terminal it is the task's own token, and the saved token is never read there |
| `NATAMUX_URL` | The daemon's address. The CLI reads the app's saved `daemon-port` when it is available, then falls back to `http://127.0.0.1:4317` |
| `NATAMUX_TASK_ID` | The task of this terminal. Used by `ask`, `done`, `task show`, `task create` (for its project) and hooks. Natamux terminals set it |
| `NATAMUX_TERMINAL` | The terminal of the task. Tells the daemon which stage `done` is about. Natamux terminals set it |
| `NATAMUX_STAGE_TICKET` | Set by the script of a command stage, so its `stage pass` gets through once. `stage pass` and `stage fail` send it in the `X-Natamux-Stage-Ticket` header |
| `NATAMUX_DEBUG` | `natamux hook` only. Any value prints problems to stderr |

See [Environment variables](/docs/reference/environment-variables/) for the daemon's variables.
