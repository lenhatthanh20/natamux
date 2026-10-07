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
| `Stop` | running | `agent.finished` | in review |
| `Stop` | needs input | `agent.resumed`, `agent.finished` | in review |
| `StopFailure` | running | `agent.failed` | failed |
| `StopFailure` | needs input | `agent.resumed`, `agent.failed` | failed |

Everything else is ignored and not logged, so the event log does not fill with calls that change nothing. `idle_prompt` is ignored: `Stop` already moved the task to review.

**A choice to know about:** because `Stop` fires after every answer, a task lands in "in review" at the end of each turn, even when the agent only asked a question in plain text. The dashboard then says "this task is waiting for you", which is true. A person decides whether it is really done (Approve) or needs more (type the next prompt, which resumes it).

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

## Not verified

- Whether `--settings` adds these hooks to the ones from other settings files, or replaces them. Check before relying on it.
- How Claude Code behaves when the daemon answers slowly. The hooks use a 5 second timeout.
- Every other hook event (`SubagentStop`, `TaskCompleted`, `WorktreeCreate` and more). They could give finer states later.
- A session that Claude Desktop or the VS Code extension runs: the docs say `permission_prompt` is timed differently there.
- The adapter was written from the docs and tested against a fake daemon. It has not been tried against a real Claude Code session yet.
