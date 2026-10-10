# @natamux/cli

The `natamux` command, also installed as the short name `nmx`. It talks to the daemon over HTTP. Agents can call it from any shell.

Run in a codespace after `pnpm build`: `pnpm --filter @natamux/cli start <command>`.

## Settings (environment variables)

| Variable | Default | Meaning |
|---|---|---|
| `NATAMUX_TOKEN` | none, required | Token the daemon printed when it started |
| `NATAMUX_URL` | `http://127.0.0.1:4317` | Address of the daemon |

## Commands

```sh
natamux task create "Fix the bug" --priority high --label bug --depends-on T-0007
natamux task list --state review
natamux task show T-0001
natamux event T-0001 task.blocked --payload '{"reason":"Needs a runner"}'
natamux event T-0001 agent.asked --source agent --payload '{"question":"Which branch?"}'
natamux events T-0001
natamux workspace create T-0001
natamux workspace remove T-0001 --yes --delete-branch
natamux task edit T-0001 --title "Fix the parser" --priority urgent
natamux agent start T-0001 --prompt "Start with the failing test"
natamux ask "Should the export keep old columns?"
natamux done
natamux stage approve T-0001 --stage test-spec
natamux stage back T-0001 --to plan --reason "Add the error cases"
natamux task link T-0001 --issue ABC-123 --url https://jira.example.com/browse/ABC-123
natamux task create "Export tasks" --context github-issue:42
```

```sh
export NATAMUX_TOKEN=<token> NATAMUX_TASK_ID=T-0001
claude --settings "$(natamux hooks claude-code)"
```

`natamux hook <agent>` is for an agent that can only run a command as its hook: it reads the hook's JSON on stdin and sends it to `POST /hooks/<agent>` of the daemon. It does nothing unless `NATAMUX_TOKEN` is set (so it is harmless in a session that is not Natamux's), always exits 0 and prints nothing, so it can never get in the agent's way. Set `NATAMUX_DEBUG=1` to see why a hook did not arrive. It does not use the proxy variables.

`hooks claude-code` prints Claude Code settings that report the session's progress to the daemon, so the task moves between running, needs input and in review by itself. It needs no token to print and never writes the token into the output. See `docs/claude-code-adapter.md`.

`ask` and `done` report on the task of the terminal (`NATAMUX_TASK_ID`, which Natamux sets in its terminals), or on `--task T-0001`. `ask` moves a running stage to needs input. `done` says the stage of this terminal is done (`NATAMUX_TERMINAL` tells which stage inside a group); the daemon then checks it, and the stage waits for your approval or moves on. Every stage prompt asks the agent to run it.

`stage` moves a task through its stages: `approve`, `back` (to an earlier stage, with `--to` and `--reason`), `skip`, `pass`, `fail` and `start`. Name the stage with `--stage` when two run side by side.

`task link` links a task to its issue or ticket elsewhere; `task create --context` says where its context comes from (`manual`, `github-issue:42`, `skill:ABC-1`, `mcp:ABC-1`).

`agent start` types the agent's command into the task's terminal, as **Start agent** in the dashboard does. A task with no workspace gets one first (`--base` picks the branch it starts from). `task show` without an id shows the task of the terminal (`NATAMUX_TASK_ID`).

`workspace create` makes a git worktree and a branch for the task. `workspace remove` deletes the worktree folder, so it needs `--yes`. Git refuses when the worktree has files that are not committed. With `--delete-branch` the branch goes too, but only when it is fully merged.

Add `--json` to any command to print JSON. `natamux help` lists every option and event type.

Exit codes: `0` done, `1` the request failed (daemon not reachable, wrong token, task not found), `2` the command line is wrong.

An event that does not fit the task's state prints `Not accepted` and exits `0`: the daemon recorded it and left the task as it was.
