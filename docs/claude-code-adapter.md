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
| `UserPromptSubmit` | failed | `agent.resumed` | running (the session goes on after an API error) |
| `PermissionRequest` | running | `agent.asked` | needs input |
| `Notification` (`permission_prompt`, `elicitation_dialog`, `agent_needs_input`) | running | `agent.asked` | needs input |
| `PostToolUse` | needs input | `agent.resumed` | running (the permission was answered) |
| `PreToolUse`, `PostToolUse` | failed | `agent.resumed` | running (the agent works again) |
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

- When the daemon starts, after it listens: each stage that is `running`, `needs-input` or `review`, when its shell still had its agent (see the list of open shells below). The task keeps its state; a stage in review moves to running only when you type to the agent. A task no agent was started for, whose agent Natamux cannot launch, or whose agent the person quit before the app closed, is left alone; **Resume** starts it.
- When a person clicks **Resume** (or sends `agent.resumed` from the CLI or the API) and no agent runs in the stage's terminal.

**A failed stage goes on.** `StopFailure` fails the stage, but the Claude Code session lives on. Typing to the agent takes the stage up again (table above). **Start agent** on a failed agent stage logs `agent.resumed` for that stage and goes on as **Resume** does: an agent that still runs is asked to go on; with none left, the stage's agent starts again in its terminal, on the stage's last session.

A `testing` stage with a `command` has no agent. When the daemon starts and such a stage is `running` (and not waiting for CI), it goes back to `ready` (`agent.stopped`) and its command runs again in a new shell.

A `ready` stage that waited for its workspace, or for the tasks it depends on, waits again after the daemon starts and starts when they are there. A task made with no agent (New workspace without an agent) keeps `withoutAgent: true` in its file until a person starts an agent, so no agent starts in its shell after a restart either.

The agent goes on with the last session it reported in the stage's shell (the list of open shells below), with no new prompt: `claude --resume <session_id>`, `codex resume ... <id>` and so on. Claude Code without one goes on with the `sessionId` that the hooks of that stage logged. With no id yet (the agent was in its first turn), the start of the daemon uses `claude --continue`, the newest session of the worktree, for a stage outside a group. A stage of a group, and **Resume**, start the agent with the stage prompt instead, since the stages of a group share the folder. Other agents with no session start with the stage prompt. An `agent.resumed` from a hook never starts an agent, since it comes from one that runs.

**The other shells after a restart.** The daemon keeps the open shells of each task in `<dataDir>/tasks/<id>.open-shells.json`: `[{ name, agent, session }]`. It writes the file when a shell starts, when an agent is launched in a shell, when a hook from a Natamux shell (`X-Natamux-Terminal`) names a new session, and when an agent typed by hand shows in a shell's processes (with no session), never on a hook that changes nothing. A shell that closes, and an agent that leaves the foreground of its shell, are written down only when they stay so for 3 s, so the file is the same after a quit, a kill or a crash: agents and shells that end with the app come back, the ones the person quit or closed do not. An agent that could not come back keeps its entry, so the next start tries again. At shutdown the daemon stops writing. The file goes when the task is done, cancelled or deleted, or its workspace is removed.

At the next start, before the daemon listens, every listed shell of an unfinished task with a workspace waits, except the shells of the stages above (their agents start as described; a shell whose stage agent did not start waits too). `GET /tasks/<id>/terminals` lists waiting shells in `terminals` and in `restoring` (`{ "<shell>": "<agent>" | null }`); the sidebar shows a waiting agent as an agent row. The first WebSocket attach without `shellId` to any shell of the task opens the task: every waiting agent of it starts, the attached shell's first, and goes on with its session with no prompt. An agent with no session goes on with the newest session of the folder (`--continue`, `codex resume --last`, `gemini --resume`); a second one of the same kind in the task comes back plain, since both would open that session. A plain shell starts with its own attach. A start that fails (the task waits for other tasks, too many shells) leaves the attached shell plain and lets the others wait again. A restored launch logs `agent.launched` with `terminal` and `restored: true` and does not change the task's agent. A shell started by a stage or a person under a waiting name replaces it as a plain shell. A restored agent, and an agent that the start of the daemon relaunches, stay in `restoring` until a program that is not a shell is in the foreground, at most 15 s; the dashboard shows a spinner meanwhile.

| Agent | Resume command |
|---|---|
| Claude Code | `claude --resume <id>` |
| Codex | `codex resume <hook and trust flags> <id>` |
| Gemini CLI | `gemini --resume <id>` |
| GitHub Copilot | `copilot --resume <id>` |
| Cursor | `cursor-agent --resume <chatId>`; the hooks' `conversation_id` is the chat id |

**Pause an idle agent.** `IdlePause` (`agents/idle_pause.rs`) keeps the last hook signal of each Natamux shell in memory, with its time. Once a minute it pauses shell T of task X when all hold: the project's `agents.pauseIdleAfterMinutes` is N > 0 (default 30); T's last signal is `TurnEnded`, N or more minutes ago, and `backgroundSince` is unset; X is not done or cancelled; an agent runs in T, Natamux knows which, and it has a session; and no client of T is visible. After a daemon restart nothing is known until the next turn end; an agent restored with no prompt counts as a turn end at its launch, so it can pause again.

The pause, in this order: `OpenShells::pause` puts T in `waiting` and writes `paused: true` for it in the open-shells file (`[{ name, agent, session, paused }]`); `TerminalService::pause_one` ends T as `close_one` does, but each client gets `{ "type": "paused", "minutes": N }` instead of `exit`; `StageRun` forgets what it knew of T; and the daemon logs `agent.paused` (source `system`, payload `{ terminal, agent, session, idleMinutes }`). A short `pausing` set keeps a `sync` in between from taking T for a replaced shell.

A paused agent starts again on its session: with no prompt on a visible attach, or an attach to T itself (a hidden attach to another shell of X leaves it paused); with the prompt on a stage launch in T or on a review comment (`POST /tasks/<id>/agent/message`). After a daemon restart a paused shell waits, and the start of the daemon does not relaunch it. The mark goes once T is open again, or when T is forgotten. `NATAMUX_IDLE_MINUTE_MS` sets the length of one minute, for the E2E images only.

**Resume while the agent still runs.** The agent waits at its prompt, so a click on **Resume** types `Go on with the stage you are working on.` to it, but only when the last thing it did in that stage was to end its turn (`Stop`). Only the agent's own events count: a start of the agent after a restart, a CI result or an edit of the task in between do not. An agent that asked (`PermissionRequest`) gets nothing, since the Enter would answer its menu. A double click types the prompt once: the second `agent.resumed` is not accepted.

**Background work.** A `Stop` is "with background work" when `background_tasks` has an entry whose `status` is not `completed`, `failed`, `killed` or `stopped`, or `session_crons` has an entry. The agent then only paused: the work wakes it again, for example when `pnpm test` in a background shell ends. So the task stays running, the `agent.waiting` event lists the work (at most 5 entries of 200 characters, in `payload.waitingFor`), and the task's one-line summary says "Waiting for Bash: pnpm test and 1 more". The task keeps `backgroundSince` until the agent shows up again or the task leaves running; until then it is never "quiet". Entries that cannot be read are skipped, so a `Stop` with only bad entries ends the turn as before. The `Stop` after `natamux done` is dropped as always, with or without background work.

## Which task a hook belongs to

1. The `X-Natamux-Task` header, when it holds a task id that exists. The header gets its value from the `NATAMUX_TASK_ID` environment variable of the Claude Code process.
2. Otherwise the task whose workspace contains the hook's `cwd`. This works for a session that was started by hand inside a workspace.
3. Otherwise the hook is ignored.

The daemon always answers `200 {}`, so a hook that matches nothing never shows as an error in Claude Code. Without the token it answers 401.

## Turning it on

In a Natamux task terminal, plain `claude` is enough: the worktree's `.claude/settings.local.json` has the `command` hooks. Adding `--settings "$(natamux hooks claude-code)"` there runs both sets, and each event reaches the daemon twice. Use the flag only where Natamux did not prepare the folder, or where that file is the user's own:

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
claude '<title> - <description>'
```

- The hooks are in the worktree's `.claude/settings.local.json` (see `agent-adapters.md`, agents started by hand). Each runs `natamux hook claude-code`.
- When that file has hooks of the user's own, Natamux leaves it alone and adds `--settings '<data dir>/claude-code-settings.json'`. The daemon writes that file at start. It holds the `http` hooks of `natamux hooks claude-code`, pointed at the daemon on loopback, so nothing has to be installed in `~/.claude`. Natamux never passes both: Claude Code would run each hook twice.
- Why commands in the worktree and not `http` hooks: GitHub Copilot CLI (1.0.12 and later) reads `.claude/settings.json` and `.claude/settings.local.json` too. It refuses an `http` hook that is not `https://` and prints a warning at start. `natamux hook claude-code` does nothing without `CLAUDECODE=1`, which Claude Code sets and Copilot does not, so Copilot's events never reach the daemon as Claude Code's.
- The shell of the terminal has `NATAMUX_TOKEN`, `NATAMUX_URL` and `NATAMUX_TASK_ID`, so the hooks name the task.
- `NATAMUX_CLAUDE_COMMAND` replaces `claude`, for example with a full path.
- The task gets `agent: claude-code` and an `agent.launched` line in its activity. The first prompt then moves it to running through `UserPromptSubmit`.
- The prompt is put on one line: the shell runs the command at the first line end.

## Known gap

`Stop` does not fire when the user interrupts Claude Code (Esc). A task that was running then stays "running" until the next hook arrives. Other ADEs have the same problem (Orca guesses an interrupt for this). Seen in the real session below: the last event of the test was an `agent.resumed` with no `agent.finished` after it.

What Natamux does about it: every hook of a running task, even one that changes nothing (for example `PostToolUse`), moves the time of the task's last update, at most once every 30 seconds. It adds no line to the task's log, so the dashboard does not read the log again for it. When a running task has had no update for 10 minutes, and its agent does not wait for background work, the dashboard marks it "quiet", lists it among the tasks that need you, and says it may be stuck, interrupted or closed. It is a hint, not a state: the task stays running until you or the agent change it. The 10 minutes are a guess: a long command keeps an agent silent without any hook, so such a task can be marked quiet by mistake.

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
