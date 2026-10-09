# Claude Code adapter

How Helmsman learns what a Claude Code session is doing, without reading the terminal.

Facts below come from the Claude Code docs (`code.claude.com/docs/en/hooks` and `hooks-guide`), read on 2026-10-08. Claude Code changes fast: re-check the docs before changing the adapter.

## What Claude Code gives us

- **Hooks** run at points in a session. They live in `~/.claude/settings.json`, `.claude/settings.json`, `.claude/settings.local.json`, in a plugin, or in the JSON given with `claude --settings`.
- A hook can be of type `command`, `http`, `mcp_tool`, `prompt` or `agent`. An **`http` hook** POSTs the same JSON a command hook would get on stdin, to a URL. Header values may use `$VAR` from the environment, but only for variables listed in `allowedEnvVars`.
- A hook that cannot reach its endpoint, or gets a non-2xx answer, is a non-blocking error. Claude Code goes on.
- Every hook gets `session_id`, `cwd` (the directory the session started in), `transcript_path`, `hook_event_name` and `permission_mode`. `agent_id` is added inside a subagent.
- `Stop` fires **whenever Claude finishes responding, not only when a task is done**. It does not fire when the user interrupts. An API error fires `StopFailure` instead.
- `Notification` has a `notification_type`. Among them: `permission_prompt` (an approval waited about six seconds), `idle_prompt` (Claude finished about 60 seconds ago and nobody typed), `elicitation_dialog`, `agent_needs_input`.
- `PermissionRequest` fires when Claude Code is about to ask for permission, so it is faster than the `permission_prompt` notification.

## How the events map to a task

| Hook | Task is | Event sent | Result |
|---|---|---|---|
| `UserPromptSubmit` | draft | `task.readied`, `agent.started` | running |
| `UserPromptSubmit` | ready | `agent.started` | running |
| `UserPromptSubmit` | needs input or in review | `agent.resumed` | running |
| `PermissionRequest` | running | `agent.asked` | needs input |
| `Notification` (`permission_prompt`, `elicitation_dialog`, `agent_needs_input`) | running | `agent.asked` | needs input |
| `PostToolUse` | needs input | `agent.resumed` | running (the permission was answered) |
| `PreToolUse` | any | none (a sign of life) | unchanged; the tool and its file or command become the task's one-line summary, such as "Edit src/a.ts" |
| `Stop` | running | `agent.finished` | needs input |
| `Stop` | needs input | `agent.resumed`, `agent.finished` | needs input |
| `StopFailure` | running | `agent.failed` | failed |
| `StopFailure` | needs input | `agent.resumed`, `agent.failed` | failed |

Everything else is ignored and not logged, so the event log does not fill with calls that change nothing. `idle_prompt` is ignored: `Stop` already moved the task to needs input.

**A choice to know about:** because `Stop` fires after every answer, a task lands in "needs input" at the end of each turn, even when the agent only asked a question in plain text. The dashboard then says "this task is waiting for you", which is true. The stage is done only when the agent runs `helmsman done`; the daemon drops the `Stop` that follows that report, runs the checks, and the stage then waits for approval ("in review") or moves on. A person can also mark the stage as done, or type the next prompt, which resumes it.

## Which task a hook belongs to

1. The `X-Helmsman-Task` header, when it holds a task id that exists. The header gets its value from the `HELMSMAN_TASK_ID` environment variable of the Claude Code process.
2. Otherwise the task whose workspace contains the hook's `cwd`. This works for a session that was started by hand inside a workspace.
3. Otherwise the hook is ignored.

The daemon always answers `200 {}`, so a hook that matches nothing never shows as an error in Claude Code. Without the token it answers 401.

## Turning it on

```sh
# in a terminal that can reach the daemon
export HELMSMAN_TOKEN=<token the daemon printed>
export HELMSMAN_TASK_ID=T-0001        # optional when claude starts inside the task's workspace
claude --settings "$(helmsman hooks claude-code)"
```

`helmsman hooks claude-code` prints the settings JSON: one `http` hook for each event above, pointing at `$HELMSMAN_URL` (default `http://127.0.0.1:4317`). The token and the task id are never written into it.

## Start from the dashboard

**Start Claude Code** in the details of a task makes the workspace when there is none, then types this into the task's terminal:

```sh
claude --settings '<data dir>/claude-code-settings.json' '<title> - <description>'
```

- The daemon writes that settings file at start. It holds the `http` hooks of `helmsman hooks claude-code`, pointed at the daemon on loopback, so nothing has to be installed in `~/.claude`.
- The shell of the terminal has `HELMSMAN_TOKEN`, `HELMSMAN_URL` and `HELMSMAN_TASK_ID`, so the hooks name the task.
- `HELMSMAN_CLAUDE_COMMAND` replaces `claude`, for example with a full path.
- The task gets `agent: claude-code` and an `agent.launched` line in its activity. The first prompt then moves it to running through `UserPromptSubmit`.
- The prompt is put on one line: the shell runs the command at the first line end.

## Known gap

`Stop` does not fire when the user interrupts Claude Code (Esc). A task that was running then stays "running" until the next hook arrives. Other ADEs have the same problem (Orca guesses an interrupt for this). Seen in the real session below: the last event of the test was an `agent.resumed` with no `agent.finished` after it.

What Helmsman does about it: every hook of a running task, even one that changes nothing (for example `PostToolUse`), moves the time of the task's last update, at most once every 30 seconds, and logs an `agent.active` event (the dashboard does not list those). When a running task has had no update for 10 minutes, the dashboard marks it "quiet", lists it among the tasks that need you, and says it may be stuck, interrupted or closed. It is a hint, not a state: the task stays running until you or the agent change it. The 10 minutes are a guess: a long command keeps an agent silent without any hook, so such a task can be marked quiet by mistake.

## Verified with a real session

2026-10-08, Claude Code on a Mac, the daemon in a GitHub codespace, the port forwarded with `gh codespace ports forward 4317:4317`, hooks from `helmsman hooks claude-code`.

- `UserPromptSubmit` on a draft task wrote `task.readied` and `agent.started`, 24 ms apart. The task went from draft to running.
- Before stages existed, `Stop` wrote `agent.finished` and the task went to in review (it goes to needs input now). A prompt typed while in review wrote `agent.resumed`. Three such rounds all matched. Every event had the source `agent` and was accepted.
- An interrupted prompt left the task running (the known gap above).
- **A proxy can block the hooks.** With a proxy set on the machine, both hooks failed with "proxy refused the connection". Claude Code sends an `http` hook through the proxy, and the docs do not say that loopback is exempt for hooks. `export NO_PROXY="localhost,127.0.0.1"` and `export no_proxy="$NO_PROXY"` before starting `claude` fixed it. Many company machines have a proxy, so a user may need this. A `command` hook that runs `curl --noproxy "*"` is the other way round it.
- **`--settings` adds to the other hooks, it does not replace them.** Claude Code reported "Ran 2 stop hooks" although these settings define one. The second came from the user's own settings. This is evidence from one session, not a statement in the docs.

## Not verified

- The permission path (`PermissionRequest`, and `Notification` with `permission_prompt`). In the test, Claude Code did not ask for permission to create a file, so those hooks never fired. The tester's guess is that the session ran in an auto permission mode (not checked). Skipped on purpose; it is covered only by tests with made-up input.
- How fast a permission prompt moves the task to "needs input".
- How Claude Code behaves when the daemon answers slowly. The hooks use a 5 second timeout.
- Every other hook event (`SubagentStop`, `TaskCompleted`, `WorktreeCreate` and more). They could give finer states later.
- A session that Claude Desktop or the VS Code extension runs: the docs say `permission_prompt` is timed differently there.
- Finding the task by the working directory (no `HELMSMAN_TASK_ID`). The session in the test ran outside the workspace, so only the header path was tried.
