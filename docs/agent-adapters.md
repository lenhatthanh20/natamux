# Agent adapters

Natamux is a product for the public, so it must work with the agents people already use, not only one. This file records how the main agents can report progress, and how Natamux is built to cover them.

Researched on 2026-10-08. Agents change quickly: re-check before building an adapter.

Verification labels: **docs** = the vendor's documentation page, **source** = the vendor's open source repository, **third party** = a blog or article, **unverified** = not read.

## What Natamux needs from an agent

Four signals are enough to drive every state in the task state machine:

| Signal | Moves the task to |
|---|---|
| The user sent a prompt, or the agent started working | running |
| The agent waits for the user (a permission, a question) | needs input |
| The agent ended its turn | needs input (the stage is done only when the agent runs `natamux done`; then the checks run) |
| The agent failed | failed |

## How each agent can send them

| Agent | Prompt sent | Waits for the user | Turn ended | Failed | Handler types | Label |
|---|---|---|---|---|---|---|
| **Claude Code** | `UserPromptSubmit` | `PermissionRequest`, `Notification` (`permission_prompt`, `elicitation_dialog`, `agent_needs_input`) | `Stop` | `StopFailure` | command, **http**, mcp_tool, prompt, agent | docs |
| **Codex CLI** | `UserPromptSubmit` | `PermissionRequest` | `Stop` (input has `turn_id`, `last_assistant_message`) | none seen (`Interrupt` exists) | command, mcp_tool, prompt, agent. No `http` seen | source |
| **Gemini CLI** | `BeforeAgent` | `Notification` with type `ToolPermission` | `AfterAgent` | none documented | command only | docs |
| **Cursor** | `beforeSubmitPrompt` | permission answers `ask` on `preToolUse`, `beforeShellExecution` and others | `stop` | `postToolUseFailure` for a tool | command (cloud agents: command only) | docs |
| **GitHub Copilot CLI** | `userPromptSubmitted` | `permissionRequest`, `notification` (the docs list no dedicated "waiting" event) | `agentStop` | `errorOccurred` | command (`bash` or `powershell`) | docs |
| **OpenCode** | not documented | `permission.asked` | `session.idle` | `session.error` | a JavaScript or TypeScript plugin, which can make HTTP calls | docs (the event payload is not documented) |
| **Aider** and other agents without hooks | no | no | no | no | none | unverified |

Details worth knowing:

- Claude Code and Codex use the same event names and the same core input fields (`session_id`, `cwd`, `hook_event_name`, `transcript_path`, `permission_mode`). Codex adds `turn_id` and `model`. **source**: `codex-rs/hooks/schema/generated/*.command.input.schema.json` in `openai/codex`.
- Gemini CLI input has `session_id`, `transcript_path`, `cwd`, `hook_event_name`, `timestamp`. Its `Notification` hook is for observing only. **docs**
- Cursor input has `conversation_id`, `workspace_roots` (a list, not a single `cwd`) and `transcript_path`. A forum thread reports that the Cursor CLI does not send every event the editor sends. **docs** and **third party**: check before relying on it.
- Copilot CLI payloads come in two spellings (camelCase and PascalCase) with different time formats. **docs**
- Codex also has an older `notify` program that gets a JSON argument when a turn ends, and terminal notifications (`osc9`, `bel`). **third party** and **source** (`legacy_notify.rs`). The docs pages of Codex answered 403 to our fetch, so its hook documentation was read from the source, not from its docs site.
- Agent Client Protocol (ACP) is an open standard that started at Zed: JSON-RPC over stdio between an editor and an agent, with `session/prompt`, `session/update`, `session/request_permission`, `session/cancel` and the stop reason `end_turn`. A tool that speaks ACP can drive any ACP agent and get exact states without hooks. **docs** (overview page only); the list of agents that support it is **third party** ("up to forty clients").

## Design: three tiers, one contract

All tiers end in the same task events (`agent.started`, `agent.asked`, `agent.finished`, ...). Everything above the adapter layer (state machine, dashboard, workspaces) stays the same.

1. **Tier 1: any terminal agent.** Natamux runs the agent in a terminal it owns (the agent runner). It sees process exit, output going quiet, and terminal bell or OSC 9 notifications. Works with every command line agent, including those with no hooks. States are marked "estimated" in the UI.
2. **Tier 2: native hooks.** One adapter per agent turns its hook events into the four signals. Exact states. Today: Claude Code. Next in line, because their hook data is documented or in open source: Codex, Gemini CLI, Cursor, Copilot CLI. OpenCode needs a small plugin instead of hooks.
3. **Tier 3: ACP client.** Later: Natamux talks ACP to agents that support it, for structured sessions.

**An open door for the rest.** Any tool can report progress through `natamux event` or the REST API. The format is documented and stable, so the community and the agents' vendors can add support without changing Natamux.

### The pieces

- **`natamux hook <agent>`**: a command that reads a hook's JSON from stdin and forwards it to the daemon. Most agents (Codex, Gemini CLI, Cursor, Copilot CLI) can only run a command, not call a URL. A command also keeps working on a machine with a proxy: in the first real test of the Claude Code `http` hook, the proxy of the machine refused the request to the daemon until `NO_PROXY` was set. It reads the token and the task id from the environment, and it must never make the agent fail: if the daemon is down it exits 0 quietly. Claude Code can keep using its `http` hook, which needs no process.
- **`POST /hooks/:agent`** on the daemon, where the agent name picks the adapter.
- **An adapter interface** (done): for each agent, a function that turns the raw hook input into one of six signals: `prompt`, `activity`, `asking`, `turn-ended`, `turn-paused`, `failed` (plus the working directory and some details). The code that decides which task events follow from a signal (`agent-signals.ts`), and which task a hook belongs to (`agent-hook-service.ts`), is shared by all agents. An adapter is one file in `packages/daemon/src`, listed in `agent-adapters.ts`; `claude-code-hooks.ts` is the example.
- **A config generator per agent**: `natamux hooks <agent>` prints the settings for that agent's own file format (Claude Code settings JSON, Codex `hooks`, Gemini `settings.json`, Cursor `hooks.json`, Copilot `.github/hooks/*.json`).

### How a hook finds its task

Same for every agent: the `NATAMUX_TASK_ID` environment variable first, then the workspace that holds the agent's working directory (`cwd`, or one of Cursor's `workspace_roots`).

A hook from a Natamux terminal (it has `NATAMUX_TERMINAL`) for a task with no workspace is ignored. Terminals run only in a workspace, so the hook comes from an agent that ended when its workspace was removed. Removing a workspace also sends `agent.stopped`, which moves a running or waiting stage back to ready.

## How other ADEs do it

Read from their open source code on 2026-10-08 (GitHub). Names of files are in the repositories named below.

| ADE | How it learns the state of an agent | Agents it covers |
|---|---|---|
| **Orca** (`stablyai/orca`) | A local hook server fed by hooks it installs itself at startup. States: `working`, `blocked`, `waiting`, `done`. A second source: the terminal title (OSC), with per-agent rules and spinner glyphs. It also guesses an interrupt, because an agent can miss its "cancelled" hook. | Many (Claude Code, Codex, Gemini, Cursor, OpenCode, Pi, Droid, Grok and more) |
| **Superset** (`superset-sh/superset`) | One shared `notify.sh` script, registered as a hook in each agent's **global** config (`~/.claude/settings.json`, `~/.codex/hooks.json`, `~/.cursor/hooks.json`, ...). A PATH wrapper per agent sets `SUPERSET_AGENT_ID`. The script posts to a local URL from `SUPERSET_HOST_AGENT_HOOK_URL`. | Claude Code, Codex, Cursor, Gemini, Copilot, Droid, Mastra, Grok, Kimi, Vibe, OpenCode |
| **Emdash** (`generalaction/emdash`) | Events `start`, `stop`, `notification`, `error` with a source of `hook` or `input` (the user typed in the terminal). Statuses `idle`, `working`, `awaiting-input`, `error`, `completed`. Notification types are the same as Claude Code's. | Many |
| **Claude Squad** (`smtg-ai/claude-squad`) | Reads the tmux pane. A change in a hash of the pane text means "working". A fixed text per agent means "waiting" (for Claude Code `No, and tell Claude what to do differently`, for Aider `(Y)es/(N)o/(D)on't ask again`). It presses Enter on the trust prompt by itself. | Claude Code, Aider, Codex, Gemini, others |
| **Vibe Kanban** (`BloopAI/vibe-kanban`) | Does not use the terminal screen. One "executor" per agent runs it in a machine-readable mode: Claude Code with `--output-format=stream-json`, `--input-format=stream-json` and `--permission-prompt-tool=stdio`; Codex through its app-server JSON-RPC; an ACP executor for others. It has its own approval service for tool use and questions. | Claude Code, Codex, Copilot, Cursor, Droid, Gemini, OpenCode, Qwen, Amp, ACP |

So there are three real approaches: hooks (Orca, Superset, Emdash), reading the screen (Claude Squad, and Orca's title rules as a second source), and driving the agent through its structured protocol (Vibe Kanban).

### What to take from them

- **Hooks are the main source.** Three of five do it, and it matches the plan.
- **Guard every hook with an environment variable.** Superset registers hooks in global agent settings and wrote down the result (`HOOKS_INVESTIGATION.md`): for Droid, Codex and others the hook ran in every session on the machine, outside Superset too, with a visible hook line in the agent's UI. Their fix is a command that does nothing unless a Superset variable is set. Natamux: `natamux hook <agent>` must exit 0 at once when `NATAMUX_TOKEN` is not set, and Natamux should prefer per-session settings (`claude --settings`, or settings that Natamux passes when it starts the agent) over editing global files. If a user chooses a global install, offer an uninstall.
- **An interrupted run leaves no event.** Orca guesses an interrupt because agents miss their cancel hook. Claude Code's docs say `Stop` does not fire when the user interrupts. So a task can stay "running" after Esc. Codex has an `Interrupt` hook. Natamux has a "no sign of life" rule for running tasks (any hook refreshes the time of the last update; 10 minutes without one marks the task "quiet" on the dashboard), and should use `Interrupt` where it exists.
- **The terminal is a useful second source.** Orca reads terminal titles, Claude Squad reads the screen text. Both are fragile: a text change in the agent breaks them. Use them only as the fallback tier, marked as estimated, and prefer the generic signals (output going quiet, bell, OSC 9) over the exact words of one agent.
- **Driving the agent through a protocol is exact but costly.** Vibe Kanban needs a separate executor for each agent and owns the whole interface, with no native terminal. Natamux should not write one per agent. ACP is the one shared protocol; use it later.
- **Orca's size is a warning.** Its hook code alone is dozens of files (pane identity, freshness, races, remote hosts). Keep Natamux to the four signals and add detail only when a real case needs it.

## Built (2026-10-10)

Each agent starts from the dashboard (**Start a task**, or **Start agent** on a stage or a failed task) with hooks for that session only. Nothing in the user's own settings changes.

| Agent | Command | How its hooks are passed | Prompt sent | Waits for the user | Turn ended | Failed | Checked |
|---|---|---|---|---|---|---|---|
| Claude Code | `claude --settings <file> --permission-mode auto '<prompt>'` | the settings file | `UserPromptSubmit` | `PermissionRequest`, `Notification` | `Stop` | `StopFailure` | real sessions |
| Codex | `codex --dangerously-bypass-hook-trust --full-auto -c 'hooks.<Event>=[…]' '<prompt>'` | `-c` overrides | `UserPromptSubmit` | `PermissionRequest` | `Stop`, `Interrupt` | none | its source, not run |
| Gemini CLI | `GEMINI_CLI_SYSTEM_SETTINGS_PATH=<file> gemini --approval-mode yolo -i '<prompt>'` | its system settings file, in the data folder | `BeforeAgent` | `Notification` (`ToolPermission`) | `AfterAgent` | none | its docs, not run |
| GitHub Copilot CLI | `copilot --allow-all-tools -i '<prompt>'` | `.github/hooks/natamux.json` in the worktree, kept out of git | `userPromptSubmitted` | `permissionRequest` | `agentStop` | `errorOccurred` that is not recoverable | a real 1.0.94 session |
| Cursor CLI | `cursor-agent --force '<prompt>'` | `.cursor/hooks.json` in the worktree, kept out of git | `beforeSubmitPrompt` | none (the bell instead) | `stop` | `stop` with `status: error` | its docs, not run |

Notes:

- **Permission flags.** The flag after the program is the `auto` permission mode, the default of `agents.permissionMode` in `.natamux.json`. With `ask` it is left out. Only Claude Code's auto mode checks each action.
- **Codex hook trust.** Codex runs a hook only after the user trusted it, and asks at start for every hook it has not seen. Natamux's hooks change with the daemon's address, so it passes `--dangerously-bypass-hook-trust`. The flag skips that review for the session's other hooks too: hooks of a project the user already trusts run without the review. To work without the flag, start Codex by hand in the task's terminal; its hooks then come from its own settings.
- **Copilot names no event.** Its hook gets the same JSON shape for several events, so each hook command names its event: `natamux hook copilot agentStop`. In the real session the hooks ran in a new, untrusted folder, and got Natamux's environment variables.
- **JSON answers.** Cursor blocks a shell command when a hook's answer is not JSON, and Gemini reads the answer as JSON, so `natamux hook cursor` and `natamux hook gemini` print `{}`, even outside a Natamux terminal.
- **A file of the user's own** at `.github/hooks/natamux.json` or `.cursor/hooks.json` is left alone; the agent then runs without Natamux's hooks and is followed by its terminal.
- **Gemini CLI** replaced its free tier with the Antigravity CLI on 2026-06-18 (its docs say so); paid accounts keep `gemini`.
- **An agent that does not start.** If the shell answers "command not found" (or the same in zsh, sh, fish, PowerShell, cmd) for the agent's program in the first 15 seconds, the task fails with the shell's line and the variable that sets the command (`NATAMUX_<AGENT>_COMMAND`). The stage card and the banner offer **Start agent** with a choice of agent.

## Order of work

1. (Done) Move the mapping out of the Claude Code adapter into the shared adapter interface. Add `natamux hook <agent>` (guarded: it does nothing without `NATAMUX_TOKEN`, always exits 0 and prints nothing) and `POST /hooks/:agent`. Claude Code keeps working.
2. Add Codex (its input is the same shape, so it is the cheapest second adapter). Use its `Interrupt` hook.
3. Add Gemini CLI.
4. (Done) A "no sign of life" rule for running tasks, shown as "quiet" on the dashboard.
5. Add Cursor and Copilot CLI after reading their payloads in full.
6. The agent runner with Tier 1 detection (output quiet, bell, OSC 9).
7. OpenCode plugin, then ACP.

Each adapter needs a real session to check it. Until then it is tested only against fake input.

## Sources

- Claude Code hooks: https://code.claude.com/docs/en/hooks and https://code.claude.com/docs/en/hooks-guide
- Codex source: https://github.com/openai/codex (`codex-rs/hooks`, `codex-rs/config/src/hook_config.rs`)
- Gemini CLI hooks: https://geminicli.com/docs/hooks/reference/
- Cursor hooks: https://cursor.com/docs/agent/hooks
- GitHub Copilot CLI hooks: https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-hooks-reference
- OpenCode plugins: https://opencode.ai/docs/plugins/
- Agent Client Protocol: https://agentclientprotocol.com/overview/introduction

## Agents without hooks

Any agent started in a task's first terminal is followed by its output (`generic-signals.ts`):

- Output is a sign of life. A running task whose terminal stays silent for 10 minutes shows as quiet.
- The terminal bell (`\x07`) moves a running task to needs input, unless the task's agent has hooks (Claude Code).
- `natamux ask` and `natamux done` let such an agent report the rest (see the plugin skill in `plugins/natamux`).
