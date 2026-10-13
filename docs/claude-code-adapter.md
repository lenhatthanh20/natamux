# Claude Code adapter

How Natamux learns what a Claude Code session is doing, without reading the terminal.

Facts below come from the Claude Code docs (`code.claude.com/docs/en/hooks` and `hooks-guide`), read on 2026-10-08. Claude Code changes fast: re-check the docs before changing the adapter.

## What Claude Code gives us

- **Hooks** run at points in a session. They live in `~/.claude/settings.json`, `.claude/settings.json`, `.claude/settings.local.json`, in a plugin, or in the JSON given with `claude --settings`.
- A hook can be of type `command`, `http`, `mcp_tool`, `prompt` or `agent`. An **`http` hook** POSTs the same JSON a command hook would get on stdin, to a URL. Header values may use `$VAR` from the environment, but only for variables listed in `allowedEnvVars`.
- A hook that cannot reach its endpoint, or gets a non-2xx answer, is a non-blocking error. Claude Code goes on.
- Every hook gets `session_id`, `cwd` (the directory the session started in), `transcript_path`, `hook_event_name` and `permission_mode`. `agent_id` is added inside a subagent.
- `Stop` fires **whenever Claude finishes responding, not only when a task is done**. It does not fire when the user interrupts. An API error fires `StopFailure` instead.
- `Stop` also gets `background_tasks` (work still in flight: `shell`, `subagent`, `monitor`, `workflow` and more, each with `type`, `status`, `description`, and `command` for a shell) and `session_crons` (scheduled prompts of the session). The docs (read 2026-10-11) say they tell "session is done" from "session is paused waiting for background work to wake it back up". Both are missing in older versions.
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
| `Stop` with no background work | running | `agent.finished` | needs input |
| `Stop` with no background work | needs input | `agent.resumed`, `agent.finished` | needs input |
| `Stop` with background work | running | `agent.waiting` | running |
| `Stop` with background work | needs input | `agent.resumed`, `agent.waiting` | running |
| `StopFailure` | running | `agent.failed` | failed |
| `StopFailure` | needs input | `agent.resumed`, `agent.failed` | failed |

Everything else is ignored and not logged, so the event log does not fill with calls that change nothing. `idle_prompt` is ignored: `Stop` already moved the task to needs input.

**A choice to know about:** because `Stop` fires after every answer, a task lands in "needs input" at the end of each turn, even when the agent only asked a question in plain text. The dashboard then says "this task is waiting for you", which is true. The stage is done only when the agent runs `natamux done`; the daemon drops the `Stop` that follows that report, runs the checks, and the stage then waits for approval ("in review") or moves on. A person can also mark the stage as done, or type the next prompt, which resumes it.

**Resume after a restart.** Quitting the app ends every agent, but a task keeps its state. The agent of a stage starts again in that stage's terminal in two cases:

- When the daemon starts, after it listens: each stage that is `running` or `needs-input`. The task keeps its state. A task no agent was started for, or whose agent Natamux cannot launch, is left alone.
- When a person clicks **Resume** (or sends `agent.resumed` from the CLI or the API) and no agent runs in the stage's terminal.

Claude Code goes on with its last session of that stage: `claude --resume <session_id>`, with no new prompt. The id is the `sessionId` that the hooks of that stage logged. With no id yet (the agent was in its first turn), the start of the daemon uses `claude --continue`, the newest session of the worktree, for a stage outside a group. A stage of a group, and **Resume**, start the agent with the stage prompt instead, since the stages of a group share the folder. Other agents always start with the stage prompt. An `agent.resumed` from a hook never starts an agent, since it comes from one that runs.

**Background work.** A `Stop` is "with background work" when `background_tasks` has an entry whose `status` is not `completed`, `failed`, `killed` or `stopped`, or `session_crons` has an entry. The agent then only paused: the work wakes it again, for example when `pnpm test` in a background shell ends. So the task stays running, the `agent.waiting` event lists the work (at most 5 entries of 200 characters, in `payload.waitingFor`), and the task's one-line summary says "Waiting for Bash: pnpm test and 1 more". The task keeps `backgroundSince` until the agent shows up again or the task leaves running; until then it is never "quiet". Entries that cannot be read are skipped, so a `Stop` with only bad entries ends the turn as before. The `Stop` after `natamux done` is dropped as always, with or without background work.

## Which task a hook belongs to

1. The `X-Natamux-Task` header, when it holds a task id that exists. The header gets its value from the `NATAMUX_TASK_ID` environment variable of the Claude Code process.
2. Otherwise the task whose workspace contains the hook's `cwd`. This works for a session that was started by hand inside a workspace.
3. Otherwise the hook is ignored.

The daemon always answers `200 {}`, so a hook that matches nothing never shows as an error in Claude Code. Without the token it answers 401.

## Turning it on

```sh
# in a terminal that can reach the daemon
export NATAMUX_TOKEN=<token the daemon printed>
export NATAMUX_TASK_ID=T-0001        # optional when claude starts inside the task's workspace
claude --settings "$(natamux hooks claude-code)"
```

`natamux hooks claude-code` prints the settings JSON: one `http` hook for each event above, pointing at `$NATAMUX_URL` (default `http://127.0.0.1:4317`). The token and the task id are never written into it.

## Start from the dashboard

**Start Claude Code** in the details of a task makes the workspace when there is none, then types this into the task's terminal:

```sh
claude --settings '<data dir>/claude-code-settings.json' '<title> - <description>'
```

- The daemon writes that settings file at start. It holds the `http` hooks of `natamux hooks claude-code`, pointed at the daemon on loopback, so nothing has to be installed in `~/.claude`.
- The shell of the terminal has `NATAMUX_TOKEN`, `NATAMUX_URL` and `NATAMUX_TASK_ID`, so the hooks name the task.
- `NATAMUX_CLAUDE_COMMAND` replaces `claude`, for example with a full path.
- The task gets `agent: claude-code` and an `agent.launched` line in its activity. The first prompt then moves it to running through `UserPromptSubmit`.
- The prompt is put on one line: the shell runs the command at the first line end.

## Known gap

`Stop` does not fire when the user interrupts Claude Code (Esc). A task that was running then stays "running" until the next hook arrives. Other ADEs have the same problem (Orca guesses an interrupt for this). Seen in the real session below: the last event of the test was an `agent.resumed` with no `agent.finished` after it.

What Natamux does about it: every hook of a running task, even one that changes nothing (for example `PostToolUse`), moves the time of the task's last update, at most once every 30 seconds, and logs an `agent.active` event (the dashboard does not list those). When a running task has had no update for 10 minutes, and its agent does not wait for background work, the dashboard marks it "quiet", lists it among the tasks that need you, and says it may be stuck, interrupted or closed. It is a hint, not a state: the task stays running until you or the agent change it. The 10 minutes are a guess: a long command keeps an agent silent without any hook, so such a task can be marked quiet by mistake.

## Verified with a real session

2026-10-08, Claude Code on a Mac, the daemon in a GitHub codespace, the port forwarded with `gh codespace ports forward 4317:4317`, hooks from `natamux hooks claude-code`.

- `UserPromptSubmit` on a draft task wrote `task.readied` and `agent.started`, 24 ms apart. The task went from draft to running.
- Before stages existed, `Stop` wrote `agent.finished` and the task went to in review (it goes to needs input now). A prompt typed while in review wrote `agent.resumed`. Three such rounds all matched. Every event had the source `agent` and was accepted.
- An interrupted prompt left the task running (the known gap above).
- **A proxy can block the hooks.** With a proxy set on the machine, both hooks failed with "proxy refused the connection". Claude Code sends an `http` hook through the proxy, and the docs do not say that loopback is exempt for hooks. `export NO_PROXY="localhost,127.0.0.1"` and `export no_proxy="$NO_PROXY"` before starting `claude` fixed it. Many company machines have a proxy, so a user may need this. A `command` hook that runs `curl --noproxy "*"` is the other way round it. Natamux now adds `127.0.0.1,localhost,::1` to `NO_PROXY` and `no_proxy` in every task terminal and keeps the hosts already there (`no-proxy.ts`). A shell profile that sets these variables again must keep `127.0.0.1` in them.
- **`--settings` adds to the other hooks, it does not replace them.** Claude Code reported "Ran 2 stop hooks" although these settings define one. The second came from the user's own settings. This is evidence from one session, not a statement in the docs.

## Not verified

- The permission path (`PermissionRequest`, and `Notification` with `permission_prompt`). In the test, Claude Code did not ask for permission to create a file, so those hooks never fired. The tester's guess is that the session ran in an auto permission mode (not checked). Skipped on purpose; it is covered only by tests with made-up input.
- How fast a permission prompt moves the task to "needs input".
- How Claude Code behaves when the daemon answers slowly. The hooks use a 5 second timeout.
- `Stop` with background work. Built from the docs only. Still to check with a real session: the `status` values of `background_tasks`, which hook comes first when a background shell ends, and whether a scheduled wakeup shows in `session_crons`.
- Every other hook event (`SubagentStop`, `TaskCompleted`, `WorktreeCreate` and more). They could give finer states later.
- A session that Claude Desktop or the VS Code extension runs: the docs say `permission_prompt` is timed differently there.
- Finding the task by the working directory (no `NATAMUX_TASK_ID`). The session in the test ran outside the workspace, so only the header path was tried.
