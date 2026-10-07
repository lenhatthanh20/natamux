# Agent adapters

Helmsman is a product for the public, so it must work with the agents people already use, not only one. This file records how the main agents can report progress, and how Helmsman is built to cover them.

Researched on 2026-10-08. Agents change quickly: re-check before building an adapter.

Verification labels: **docs** = the vendor's documentation page, **source** = the vendor's open source repository, **third party** = a blog or article, **unverified** = not read.

## What Helmsman needs from an agent

Four signals are enough to drive every state in the task state machine:

| Signal | Moves the task to |
|---|---|
| The user sent a prompt, or the agent started working | running |
| The agent waits for the user (a permission, a question) | needs input |
| The agent ended its turn | in review |
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

1. **Tier 1: any terminal agent.** Helmsman runs the agent in a terminal it owns (the agent runner). It sees process exit, output going quiet, and terminal bell or OSC 9 notifications. Works with every command line agent, including those with no hooks. States are marked "estimated" in the UI.
2. **Tier 2: native hooks.** One adapter per agent turns its hook events into the four signals. Exact states. Today: Claude Code. Next in line, because their hook data is documented or in open source: Codex, Gemini CLI, Cursor, Copilot CLI. OpenCode needs a small plugin instead of hooks.
3. **Tier 3: ACP client.** Later: Helmsman talks ACP to agents that support it, for structured sessions.

**An open door for the rest.** Any tool can report progress through `helmsman event` or the REST API. The format is documented and stable, so the community and the agents' vendors can add support without changing Helmsman.

### The pieces

- **`helmsman hook <agent>`**: a command that reads a hook's JSON from stdin and forwards it to the daemon. Most agents (Codex, Gemini CLI, Cursor, Copilot CLI) can only run a command, not call a URL. It reads the token and the task id from the environment, and it must never make the agent fail: if the daemon is down it exits 0 quietly. Claude Code can keep using its `http` hook, which needs no process.
- **`POST /hooks/:agent`** on the daemon, where the agent name picks the adapter.
- **An adapter interface**: for each agent, a function that turns the raw hook input into one of the four signals (plus `cwd` or `workspace_roots` and the session id). The code that decides which task events follow from a signal, and which task a hook belongs to, is shared by all agents. Today that code sits inside the Claude Code adapter and speaks Claude Code's event names; it needs to move out.
- **A config generator per agent**: `helmsman hooks <agent>` prints the settings for that agent's own file format (Claude Code settings JSON, Codex `hooks`, Gemini `settings.json`, Cursor `hooks.json`, Copilot `.github/hooks/*.json`).

### How a hook finds its task

Same for every agent: the `HELMSMAN_TASK_ID` environment variable first, then the workspace that holds the agent's working directory (`cwd`, or one of Cursor's `workspace_roots`).

## Order of work

1. Move the mapping out of the Claude Code adapter into the shared adapter interface. Add `helmsman hook <agent>` and `POST /hooks/:agent`. Claude Code keeps working.
2. Add Codex (its input is the same shape, so it is the cheapest second adapter).
3. Add Gemini CLI.
4. Add Cursor and Copilot CLI after reading their payloads in full.
5. The agent runner with Tier 1 detection.
6. OpenCode plugin, then ACP.

Each adapter needs a real session to check it. Until then it is tested only against fake input.

## Sources

- Claude Code hooks: https://code.claude.com/docs/en/hooks and https://code.claude.com/docs/en/hooks-guide
- Codex source: https://github.com/openai/codex (`codex-rs/hooks`, `codex-rs/config/src/hook_config.rs`)
- Gemini CLI hooks: https://geminicli.com/docs/hooks/reference/
- Cursor hooks: https://cursor.com/docs/agent/hooks
- GitHub Copilot CLI hooks: https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-hooks-reference
- OpenCode plugins: https://opencode.ai/docs/plugins/
- Agent Client Protocol: https://agentclientprotocol.com/overview/introduction
