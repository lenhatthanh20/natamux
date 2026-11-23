# Agent adapters

Natamux is a product for the public, so it must work with the agents people already use, not only one. This file records how the main agents can report progress, and how Natamux is built to cover them.

Researched on 2026-10-08. The agents OpenCode, Qwen Code, Factory Droid, Amp, Kimi, Goose, Aider, Crush, Cline, Kiro and Muse Code were researched on 2026-11-17 (see "Added on 2026-11-17"). Agents change quickly: re-check before building an adapter.

Verification labels: **docs** = the vendor's documentation page, **source** = the vendor's open source repository, **third party** = a blog or article, **unverified** = not read.

## What Natamux needs from an agent

Four signals are enough to drive every state in the task state machine:

| Signal | Moves the task to |
|---|---|
| The user sent a prompt, or the agent started working | running |
| The agent waits for the user (a permission, a question) | needs input |
| The agent ended its turn with a report | idle, and the app rings "Finished its turn"; the Dock counts it until you open the task (the stage is done only when the agent runs `natamux done`; then the checks run) |
| The agent ended its turn on a question (its last line, or the line before the list of options it ends with, ends in `?`) | needs input |
| The agent ended its turn while the task waits for an answer (`natamux ask`, a question) | stays needs input |
| The agent ended its turn while the task waits on a hook (a permission, an options dialog) | idle: the wait ended with the turn, as after a denied permission |
| The agent failed | failed |

Permission and terminal-bell waits return to running after a tool completion hook, or to idle when the turn ends after a denied permission. A before-tool hook can fire before the permission card, so it does not clear a wait. An explicit question (`natamux ask` or a turn that ends on a question) stays waiting until a new prompt arrives, even if other tools run. The daemon reads the wait source from the stage's event log, so it also works after a restart. Unknown waits stay conservative. Each parallel stage keeps its own wait.

The regression cases in `crates/natamux-daemon/tests/hook_service/resume_waits.rs` cover these adapter events with synthetic payloads:

| Agent | Submitted answer | Permission or dialog wait | Tool completion |
|---|---|---|---|
| Claude Code | `UserPromptSubmit` | `PermissionRequest` | `PostToolUse` |
| Codex | `UserPromptSubmit` | `PermissionRequest` | `PostToolUse` |
| Gemini CLI | `BeforeAgent` | `Notification` (`ToolPermission`) | `AfterTool` |
| Copilot CLI | `userPromptSubmitted` | `notification` (`permission_prompt`), or terminal bell | `postToolUse` |
| Cursor CLI | `beforeSubmitPrompt` | Terminal bell; the CLI has no reliable permission-wait hook | `afterShellExecution` |

These cases cover submitted answers, tool completion, denied permissions, duplicate hooks, restart recovery and explicit questions. They do not replace checks in live agent sessions. A custom agent uses its base agent's hooks. An agent without hooks still needs a prompt event or an explicit state change; output alone does not prove that a question was answered.

## How each agent can send them

| Agent | Prompt sent | Waits for the user | Turn ended | Failed | Handler types | Label |
|---|---|---|---|---|---|---|
| **Claude Code** | `UserPromptSubmit` | `PermissionRequest`, `Notification` (`permission_prompt`, `elicitation_dialog`, `agent_needs_input`) | `Stop` | `StopFailure` | command, **http**, mcp_tool, prompt, agent | docs |
| **Codex CLI** | `UserPromptSubmit` | `PermissionRequest` | `Stop` (input has `turn_id`, `last_assistant_message`) | none seen (`Interrupt` exists) | command, mcp_tool, prompt, agent. No `http` seen | source |
| **Gemini CLI** | `BeforeAgent` | `Notification` with type `ToolPermission` | `AfterAgent` | none documented | command only | docs |
| **Cursor** | `beforeSubmitPrompt` | permission answers `ask` on `preToolUse`, `beforeShellExecution` and others | `stop` | `postToolUseFailure` for a tool | command (cloud agents: command only) | docs |
| **GitHub Copilot CLI** | `userPromptSubmitted` | `notification` with `notification_type` `permission_prompt` or `elicitation_dialog`. Not `permissionRequest`: it fires before the permission rules, so also for each tool that `--allow-all-tools` approves | `agentStop` | `errorOccurred` | command (`bash` or `powershell`) | docs |
| **OpenCode** | `chat.message` plugin hook | `permission.asked` (`permission.v2.asked` in v2) | `session.idle` | `session.error` | a JavaScript or TypeScript plugin in `.opencode/plugins/`, which can make HTTP calls | docs (https://opencode.ai/docs/plugins/) and source (`packages/plugin/src/index.ts`, tag v1.19.6) |
| **Qwen Code** | `UserPromptSubmit` | `PermissionRequest`, `Notification` (`permission_prompt`) | `Stop` | `StopFailure` | command | docs (`docs/users/features/hooks.md` in `QwenLM/qwen-code`) |
| **Factory Droid** | `UserPromptSubmit` | `Notification` (`permission_prompt`, `elicitation_dialog`). No `PermissionRequest` | `Stop` | none | command | docs (https://docs.factory.com/cli/configuration/hooks-guide) |
| **Muse Code** | `UserPromptSubmit` | `PermissionRequest`, `Notification` | `Stop` | `StopFailure` | command. Natamux does not use them yet: hooks run with a cleared environment (see "Muse hooks") | docs (https://dev.meta.ai/docs/muse-code/extending); the file format is **orca** (`src/main/muse/hook-settings.ts`) |
| **Amp**, **Kimi**, **Goose**, **Aider**, **Crush**, **Cline**, **Kiro** | no | no | no | no | none found | docs (their CLI pages); see "Agents followed by their terminal" |

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
- **An adapter interface** (done): for each agent, a function that turns the raw hook input into one of seven signals: `prompt`, `activity`, `asking`, `turn-ended`, `turn-paused`, `turn-asked`, `failed` (plus the working directory and some details). The Rust adapters are in `crates/natamux-agents/src/hooks/`; `crates/natamux-daemon/src/agents/hook_service.rs` decides which task events follow and which task a hook belongs to. `claude_code.rs` is an example.
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
| Codex | `codex --dangerously-bypass-hook-trust --sandbox workspace-write --ask-for-approval never -c sandbox_workspace_write.network_access=true -c 'projects={…}' -c 'hooks.<Event>=[…]' --no-alt-screen '<prompt>'` | `-c` overrides | `UserPromptSubmit` | `PermissionRequest` | `Stop`, `Interrupt` | none | its source, not run |
| Gemini CLI | `GEMINI_CLI_TRUST_WORKSPACE=true GEMINI_CLI_SYSTEM_SETTINGS_PATH=<file> gemini --approval-mode yolo -i '<prompt>'` | its system settings file, in the data folder | `BeforeAgent` | `Notification` (`ToolPermission`) | `AfterAgent` | none | its docs, not run |
| GitHub Copilot CLI | `copilot --allow-all-tools -i '<prompt>'` | `.github/hooks/natamux.json` in the worktree, kept out of git | `userPromptSubmitted` | `notification` (`permission_prompt`, `elicitation_dialog`) | `agentStop` | `errorOccurred` that is not recoverable | a real 1.0.94 session |
| Cursor CLI | `cursor-agent --force '<prompt>'` | `.cursor/hooks.json` in the worktree, kept out of git | `beforeSubmitPrompt` | none (the bell instead) | `stop` | `stop` with `status: error` | its docs, not run |
| OpenCode | `opencode --auto [extra args] --prompt '<prompt>'` | the plugin `.opencode/plugins/natamux.js` in the worktree, kept out of git | `chat.message` | `permission.asked`, `permission.v2.asked` | `session.idle` | `session.error` | its docs and source, not run |
| Qwen Code | `qwen --approval-mode yolo [extra args] -i '<prompt>'` | `.qwen/settings.json` in the worktree, kept out of git | `UserPromptSubmit` | `PermissionRequest`, `Notification` (`permission_prompt`) | `Stop` | `StopFailure` | its docs, not run |
| Factory Droid | `droid --auto high [extra args] '<prompt>'` | `.factory/hooks.json` in the worktree, kept out of git | `UserPromptSubmit` | `Notification` (`permission_prompt`, `elicitation_dialog`); the bell still counts | `Stop` | none | its docs, not run |

Notes:

- **Permission flags.** The flag after the program is the `auto` permission mode, the default of `agents.permissionMode` in `.natamux.json`. With `ask` it is left out. Only Claude Code's auto mode checks each action.
- **Codex hook trust.** Codex runs a hook only after the user trusted it, and asks at start for every hook it has not seen. Natamux's hooks change with the daemon's address, so it passes `--dangerously-bypass-hook-trust`. The flag skips that review for the session's other hooks too: hooks of a project the user already trusts run without the review. To work without the flag, start Codex by hand in the task's terminal; its hooks then come from its own settings.
- **Folder trust.** Each task has a new worktree, and most agents ask once per folder whether to trust it. Natamux trusts it before the start: Codex by a `-c 'projects={"<worktree>"={trust_level="trusted"}}'` override (a dotted `-c` key would split the path at its dots), Gemini by `GEMINI_CLI_TRUST_WORKSPACE=true`, Copilot by adding the worktree to `trustedFolders` in `~/.copilot/config.json`, Cursor by writing `~/.cursor/projects/<slug>/.workspace-trusted`, the marker `cursor-agent` itself writes (slug: the path with each run of non-alphanumeric characters turned into `-`, read from `cursor-agent` 2026.10.01). Claude Code does not ask in a Natamux worktree. Cursor's `--trust` flag works only with `-p`.
- **Copilot names no event.** Its hook gets the same JSON shape for several events, so each hook command names its event: `natamux hook copilot agentStop`. In the real session the hooks ran in a new, untrusted folder, and got Natamux's environment variables.
- **JSON answers.** Cursor blocks a shell command when a hook's answer is not JSON, and Gemini reads the answer as JSON, so `natamux hook cursor` and `natamux hook gemini` print `{}`, even outside a Natamux terminal.
- **Agents started by hand.** The daemon names the agent in a shell from its processes (`agentOfCommand`; Cursor also as `agent`). Each new shell gets the workspace hook files, all kept out of git: `.claude/settings.local.json`, `.gemini/settings.json`, `.codex/config.toml`, `.cursor/hooks.json`, `.github/hooks/natamux.json`. So each agent started by hand reports, with its session, and comes back on it after a restart. Copilot CLI also reads the `.claude` file; its hooks run `natamux hook claude-code`, which drops a call without `CLAUDECODE=1`, so an agent never reports through another agent's file. In a shell that no stage uses, hooks set a state for that shell only (`AgentHookService.shellState`); they never change the task. Limits: Codex reads `.codex/config.toml` only in a folder the user trusts (checked with Codex 0.162), and asks once to trust the hooks; Gemini CLI may ask before it runs a new project hook. Without the hooks, an agent started by hand comes back on the newest session of the folder.
- **One copy of each hook.** An agent that Natamux starts gets its hooks from the launch command too. Claude Code's file has `command` hooks and its `--settings` file has `http` hooks: different handlers, so Claude Code would run both. A launch leaves out `--settings` when `.claude/settings.local.json` is in place, and adds it only when that file is the user's own. Codex's `-c hooks.<event>=` replaces the same key of the file. Gemini's docs do not say that it runs a hook named twice once, so a launch leaves out `GEMINI_CLI_SYSTEM_SETTINGS_PATH` when `.gemini/settings.json` is in place.
- **A file of the user's own** at one of these places is left alone: Natamux writes a JSON file again only when every hook in it is Natamux's (a command that starts with `natamux hook`, or Claude Code's http hook with the `X-Natamux-Task` header), and keeps its other keys, such as the permissions Claude Code saved there. It writes `.codex/config.toml` again only when its first line is Natamux's. Otherwise the agent runs without Natamux's hooks and is followed by its terminal.
- **Gemini CLI** replaced its free tier with the Antigravity CLI on 2026-06-18 (its docs say so); paid accounts keep `gemini`.
- **An agent that does not start.** If the shell answers "command not found" (or the same in zsh, sh, fish, PowerShell, cmd) for the agent's program in the first 30 seconds, the task fails with the shell's line and the variable that sets the command (`NATAMUX_<AGENT>_COMMAND`). The stage card and the banner offer **Start agent** with a choice of agent.
- **An agent that asks to sign in.** With no login an agent shows its sign-in screen and waits, and no hook runs. If a line of that screen shows up in the first 30 seconds (the echo of the typed launch line does not count), the task gets `agent.asked` with `reason: "sign in"` and a `message` that quotes the line and names the terminal, so it moves to needs input. The agent's next hook after the login moves it back. Cursor moves count as spaces: Claude Code and Codex put the words of a line apart with them. The lines are in `crates/natamux-agents/src/sign_in.rs`. Each was seen on 2026-11-09 in the real CLI, installed fresh on Ubuntu with no login, in a 120x40 terminal. Check them again when a version changes:

  | Agent | Version | Sign-in line | What the CLI did |
  |---|---|---|---|
  | Claude Code | 2.1.319 | `Choose the text style`, `Please run /login` | The first start shows the theme picker, then the login. Once set up, it shows no login screen: it answers the prompt with `Not logged in · Please run /login` |
  | Codex | 0.167.0 | `Sign in with ChatGPT` | The sign-in menu |
  | Gemini CLI | 0.67.0 | `How would you like to authenticate` | The auth dialog, in a trusted folder. In a folder it does not trust, it ends at once (code 55): the next item covers that |
  | GitHub Copilot CLI | 1.0.107 | `Please use /login to sign in` | The message under the prompt box |
  | Cursor CLI | 2026.11.05 | `Authentication required` | The interactive start showed a blank screen for 60 s, which Natamux cannot tell from a slow start. With `-p` it prints `Error: Authentication required. Please run 'agent login' first` and ends with code 1 |

- **The task's model.** The Model field of **Start a task** (and of the task's Edit form) is empty by default: the agent uses its own default. A name is passed as `--model '<name>'` after the `NATAMUX_<AGENT>_ARGS` words and before the stage words. It goes only to the task's agent (a stage's other agent gets none), only to a built-in agent or a custom one on a built-in base, and not when the stage words name a model (`--model`, `--model=`, `-m`): the stage wins. The app and the daemon accept 1 to 100 characters of `A-Z a-z 0-9 . _ - : / @ [ ]`, with no leading `-`; anything else is refused before the task is made (400, issue at `model`). There is no list of known names: a well-formed but wrong name reaches the agent. An agent that then ends at once fails the task, and the reason names the model; one that shows an error at its first turn (Claude Code does) waits in its terminal. Fix the model in Edit, then start the agent again.
- **An agent that ends at once.** The launch script runs `<command> || printf '\033]2;natamux-agent-failed\007'` (`||` and printf work in sh, bash, zsh and fish; fish has no `$?`); on Windows the typed line sets that title when `$LASTEXITCODE` is not 0. If the title shows up in the first 30 seconds and the agent sent no event since its launch, the task fails with the agent's last output line and where to look (`NATAMUX_<AGENT>_ARGS`, or Settings > Agents for a custom agent). An agent that came back after a restart is left alone: a session it cannot find is no failure. An exit with code 0, such as `/exit`, changes nothing.
- **OpenCode, Qwen Code and Droid** were added on 2026-11-17. Each gets a file in the worktree, written by Natamux and kept out of git, as Copilot and Cursor do. A file of the user's own at that place is left alone, as above (the OpenCode plugin is Natamux's only when its first line is `// Natamux hooks: Natamux writes this file again.`).
- **Qwen Code asks to approve project hooks.** Project hooks do not run until approved. Qwen asks once per workspace and binds the approval to the exact file, so an edit asks again. Only `--yolo` or `--approval-mode yolo` on the command line skips the question; the `auto` mode passes it. With `ask`, the first start waits at that question. **docs** (`docs/users/features/hooks.md`, "Approving project hooks").
- **Qwen's prompt flag.** `qwen -i '<prompt>'` runs the prompt and stays open. A prompt with no `-i`, or with `-p`, runs once and ends. **source** (`packages/cli/src/config/top-level-options.ts`).
- **OpenCode's plugin folder.** v1 reads `.opencode/plugin/` and `.opencode/plugins/`; the docs of v2 name only `.opencode/plugins/`, so Natamux writes there. Orca treats v2 as another program (`opencode2`); set `NATAMUX_OPENCODE_COMMAND` for it. **source** (`packages/opencode/src/config/plugin.ts`, v1.19.6) and **orca** (`src/shared/agent-launch/catalog/tui-agent-config.ts`).
- **Droid's flag.** `--auto high` is documented for `droid exec`. Orca passes it to the interactive mode too, and so does Natamux. **docs** and **orca** (`permissions/tui-agent-permissions.ts`).
- **Droid and the bell.** Droid's hooks do send a `Notification` for a permission, but the daemon still counts the bell for it as a fallback (`AGENTS_WITH_HOOKS` in `generic_signals.rs`).
- **Skills.** OpenCode, Qwen Code and Droid read their own folders, listed in "Skills of the new agents" below.

## Added on 2026-11-17

Checked on 2026-11-17 by reading each vendor's docs and source. None of the 11 new agents was run for this; "Checked" says "its docs, not run". The facts of each agent are labelled: **docs** = vendor docs, **source** = vendor repository, **orca** = `stablyai/orca` (`src/shared/agent-launch/...`), which launches most of these agents as a bare terminal program and pastes the prompt once it is up.

### Agents followed by their terminal

Amp, Kimi, Goose, Aider, Crush, Cline, Kiro and Muse Code have no hooks in Natamux. They are followed as in "Agents without hooks" below: output, the terminal bell, quiet time, `natamux ask` and `natamux done`. They send no session, so a task cannot come back on its session after a restart unless the line has a resume flag (last column).

| Agent | Command line | Auto flag | Model | Resume | Checked |
|---|---|---|---|---|---|
| Amp | `amp --dangerously-allow-all` | `--dangerously-allow-all` (**orca**, `permissions/tui-agent-permissions.ts`; not in the docs) | none (no flag found) | none | its docs, not run |
| Kimi | `kimi --yolo` | `--yolo` (**docs**, `docs/en/reference/kimi-command.md` in `MoonshotAI/kimi-cli`) | `--model` | `--session <id>`, `--continue` | its docs, not run |
| Goose | `GOOSE_MODE=auto goose session` (PowerShell: `$env:GOOSE_MODE='auto';`) | the `GOOSE_MODE=auto` variable, set only in the `auto` mode; a user's config may set `approve` (**docs**, **orca**) | none (`GOOSE_MODEL` in the environment) | `--resume --session-id <id>`, `--resume` | its docs, not run |
| Aider | `aider --yes-always` | `--yes-always` (**docs**) | `--model` | `--restore-chat-history` for the newest only | its docs, not run |
| Crush | `crush --yolo` | `--yolo` (**source**, `internal/cmd/root.go`) | none (only `crush run` takes `--model`) | `--session <id>`, `--continue` | its source, not run |
| Cline | `cline --auto-approve true` | `--auto-approve true` (**docs**) | `--model` | `--id <session>` | its docs, not run |
| Kiro | `kiro-cli chat --trust-all-tools` | `--trust-all-tools` (**docs**) | `--model` | `--resume-id <id>`, `--resume` | its docs, not run |
| Muse Code | `muse --trust-workspace --yolo` | `--yolo` (**docs**, https://dev.meta.ai/docs/muse-code/permissions) | `--model` | `muse resume <id>` | its docs, not run |

- **Model.** The Model field of **Start a task** goes to the agent as `--model '<name>'`, except for Amp, Goose and Crush (`NO_MODEL_AGENTS` in `crates/natamux-proto/src/agents/model.rs`). The app hides the field for them.
- **The prompt is pasted.** None of these takes a first prompt on its line and stays open. Amp, Kimi, Aider, Crush and Muse have no flag for it. A prompt on Cline's line runs it headless. The flags of Goose (`run -s -t`) and Kiro (`chat "<text>"`) are not documented to keep the screen open. Natamux starts the agent with no prompt and pastes the prompt into its terminal when its first screen has settled: the agent is in front of the shell (the terminal's foreground process is not a shell), it printed, then 1.5 s with no output. The shell's echo of the launch line does not count. An agent in front that printed nothing gets it after 30 s. The prompt is dropped, never typed into the shell, when the agent ended (the shell is in front again, or the launch line's title of a failed or ended agent came) or never came up in 30 s: the shell would run it as a command, and it can hold the text of an imported issue. Windows does not tell the foreground process, so there only the titles tell. Each prompt is pasted once (`crates/natamux-daemon/src/agents/pasted_prompt.rs`). Orca pastes for the same agents.
- **The bell.** The bell counts for every agent in this section (`AGENTS_WITH_HOOKS` in `generic_signals.rs` lists the agents that ignore it).
- **A short answer** (a commit message draft) must run no tools. It runs the agent's one-shot mode where that mode cannot change files: `qwen -p`, `droid exec` (read-only without `--auto`, **docs**), `GOOSE_MODE=chat goose run -t` (the `chat` mode runs no tools, **docs**), `kiro-cli chat --no-interactive`. The others have no mode that only answers, so their drafts fall back to Claude Code (`has_one_shot`): Aider's `--message` and a prompt on Cline's line run a whole turn; `kimi --print` turns on `--afk`, which approves every tool call (**docs**, https://moonshotai.github.io/kimi-cli/en/reference/kimi-command.html); `crush run` approves each permission of its session (**source**, `internal/cmd/run.go`); `amp -x` runs a whole turn with tools (**docs**, https://ampcode.com/docs/cli/execute-mode); `opencode run` uses the `build` agent, which allows every tool by default (**docs**, https://opencode.ai/docs/permissions/); Muse's docs do not say that `muse exec` runs no tools.
- **Usage.** No token numbers for any of the 11 new agents. The dashboard shows none.
- **Sign-in.** No known sign-in screen line for them, so a task that waits on a login is not marked "sign in".
- **Kimi.** Two products share the name: the Python Kimi Code CLI (docs above) and a newer Kimi Code 2.x with the program `kimi-code` and the flag `--auto` (**orca**). Natamux accepts both programs and passes `--yolo`. The difference is not resolved.
- **Muse hooks.** Meta's docs name a project file `.muse/hooks.json` (**docs**, https://dev.meta.ai/docs/muse-code/extending), and it runs only in a trusted folder. Muse runs hooks with a cleared environment: only a short list of variables passes, and others must be named in the settings key `managed_hooks_env_vars` (**orca**, `src/main/muse/hook-config-json.ts`). So `natamux hook` would not see `NATAMUX_TOKEN`, and Natamux does not write the file yet. Orca writes the Claude-shaped format `{"hooks":{"<Event>":[{"hooks":[{"type":"command","command":"..."}]}]}}`; the docs do not show the format.
- **Not verified.** Cline's `-i` with a prompt (keeps the TUI open?) was not found, Kiro's `--tui` is in Orca but not in its docs, and the Amp flag could have changed in the newest release.

### Skills of the new agents

| Agent | Project folders | User folders | Label |
|---|---|---|---|
| OpenCode | `.opencode/skills`, `.claude/skills`, `.agents/skills` | `~/.config/opencode/skills`, `~/.claude/skills`, `~/.agents/skills` | docs |
| Qwen Code | `.qwen/skills` | `~/.qwen/skills`, `~/.agents/skills` | docs |
| Droid | `.factory/skills` | `~/.factory/skills` | docs |
| Amp | `.agents/skills`, `.claude/skills` | `~/.config/agents/skills`, `~/.agents/skills`, `~/.config/amp/skills`, `~/.claude/skills` | docs |
| Kimi | `.kimi/skills`, `.claude/skills`, `.codex/skills`, `.agents/skills` | `~/.kimi/skills`, `~/.claude/skills`, `~/.config/agents/skills`, `~/.agents/skills` | docs |
| Crush | `.agents/skills`, `.crush/skills`, `.claude/skills`, `.cursor/skills` | `~/.config/crush/skills`, `~/.agents/skills`, `~/.claude/skills` | source (README) |
| Cline | `.cline/skills` | `~/.cline/skills` | docs |
| Kiro | `.kiro/skills` | `~/.kiro/skills` | docs |
| Muse Code | `.agents/skills`, `.codex/skills`, `.claude/skills` | `~/.config/muse/skills`, `~/.agents/skills`, `~/.claude/skills` | docs |

Goose and Aider: no skill folder found (Aider has no skills). The current code is `crates/natamux-agents/src/skills.rs`.

### Sources

- OpenCode: https://opencode.ai/docs/plugins/ , https://opencode.ai/docs/cli/ , https://opencode.ai/docs/skills/ , https://github.com/anomalyco/opencode
- Qwen Code: https://github.com/QwenLM/qwen-code (`docs/users/features/hooks.md`, `approval-mode.md`, `headless.md`, `skills.md`)
- Factory Droid: https://docs.factory.com/reference/cli-reference , https://docs.factory.com/cli/configuration/hooks-guide , https://docs.factory.com/cli/droid-exec/overview
- Amp: https://ampcode.com/docs/cli , https://ampcode.com/docs/cli/execute-mode
- Kimi: https://github.com/MoonshotAI/kimi-cli
- Goose: https://goose-docs.ai/docs/guides/goose-cli-commands , https://github.com/aaif-goose/goose
- Aider: https://aider.chat/docs/config/options.html
- Crush: https://github.com/charmbracelet/crush
- Cline: https://docs.cline.bot/cline-cli/cli-reference
- Kiro: https://kiro.dev/docs/reference/cli-commands/ , https://kiro.dev/docs/skills/
- Muse Code: https://dev.meta.ai/docs/muse-code
- Orca: https://github.com/stablyai/orca (`src/shared/agent-launch/`)

## Skills

Checked on 2026-10-11 by reading each vendor's docs. Agents change quickly: check again before relying on it. The current code is in `crates/natamux-agents/src/skills.rs` and `skill_list.rs`.

| Agent | Project folders | User folders | Other | How a prompt calls a skill | Label |
|---|---|---|---|---|---|
| Claude Code | `.claude/skills` | `~/.claude/skills` | Plugins: `<installPath>/skills/<name>/SKILL.md`, called `/<plugin>:<skill>`. `enabledPlugins` (`{ "name@marketplace": true }`) in the repo's `.claude/settings.json` and `.claude/settings.local.json` is shared; in `~/.claude/settings.json` it is this machine's. The install path is in `~/.claude/plugins/installed_plugins.json` | `/<name>` | docs |
| Codex | `.agents/skills` | `~/.agents/skills` | Plugin install path not documented: not read | `$<name>` | docs |
| Gemini CLI | `.gemini/skills`, `.agents/skills` | `~/.gemini/skills`, `~/.agents/skills` | Extensions: `~/.gemini/extensions/<ext>/skills/<name>/SKILL.md` | None: the model activates a skill by its description | docs |
| Copilot CLI | `.github/skills`, `.claude/skills`, `.agents/skills` | `~/.copilot/skills`, `~/.agents/skills` | Plugin folders: not read | `/<name>` ("Use the /name skill") | docs |
| Cursor | `.agents/skills`, `.cursor/skills`, `.claude/skills`, `.codex/skills` | The same four under `~` | | None: the docs do not say the CLI calls skills by name, and a forum thread says it misses some | docs and third party |

Each skill is a folder with a `SKILL.md`. Its front matter has `name` and `description`. When the same name is in several folders of one agent, the closest wins: project, then plugin, then user.

How `{skill:<name>}` in a stage prompt, and the `skill` context source, is filled for the task's agent:

1. The agent has the skill and a sure call: that call.
2. Otherwise, a `SKILL.md` of that name exists for any agent (in the worktree, in the main checkout, or on this machine): `the skill in <path> (read that file and follow it)`. This works for any agent, also one Natamux does not know. The main checkout counts because its untracked files, such as `.claude/settings.local.json` that turns a plugin on, are not in a new worktree. A skill found only there always gets this form: the agent in the worktree may not have it.
3. Otherwise: `the <name> skill`, and the stage card says "The skill <name> was not found for <agent>."

The files of an agent that Natamux reads, such as `installed_plugins.json`, are Claude Code's own. A file that is missing or has an unknown shape gives no skills from that source, never an error. A plugin that is on but not installed gives a notice.

Sources:

- Claude Code: https://code.claude.com/docs/en/skills , https://code.claude.com/docs/en/plugins-reference
- Codex: https://learn.chatgpt.com/docs/build-skills
- Gemini CLI: https://geminicli.com/docs/cli/skills/
- Copilot CLI: https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/add-skills
- Cursor: https://cursor.com/docs/context/skills

## Usage

Tokens and cost per task, read from the files the agents write. Checked on 2026-10-21 against real session files (Claude Code, Codex, Copilot CLI, Cursor) and the vendor docs (Gemini CLI). Current code: `crates/natamux-daemon/src/agents/usage_service.rs` and `crates/natamux-agents/src/usage/`.

| Agent | Where the usage is | Fields | Model | Support |
|---|---|---|---|---|
| Claude Code | `transcript_path` of the hook, plus `<session>/subagents/*.jsonl` next to it | `message.usage`: `input_tokens`, `cache_read_input_tokens`, `cache_creation_input_tokens`, `output_tokens`. Several lines share one `message.id`: each id counts once | `message.model` | Full (real file) |
| Codex | `transcript_path` of the hook (rollout file) | `token_count` lines, `info.total_token_usage`: `input_tokens` (includes the cached part), `cached_input_tokens`, `cache_write_input_tokens`, `output_tokens` (includes reasoning) | `turn_context.payload.model` | Full (real file) |
| Gemini CLI | `transcript_path` of the hook (chat JSON) | `messages[].tokens`: `input` (includes `cached`), `cached`, `output`, `thoughts` | `messages[].model` | Full by the docs; not checked on a real session |
| Copilot CLI | `~/.copilot/session-state/<sessionId>/events.jsonl` | `session.usage_record`: `inputTokens` (includes cache), `cacheReadTokens`, `cacheWriteTokens`, `outputTokens`, `reasoningTokens`. Its `cost` is premium requests, not USD | `data.usage.model` | Tokens full; USD is an estimate (real file) |
| Cursor CLI | none: its transcript has no token counts | | | Not supported. The dashboard says "Cursor does not report usage." |
| Agents without hooks | none (no session, no file) | | | Not supported |

- **One shape.** Per model: `input` (fresh, no cache), `cacheRead`, `cacheWrite`, `output` (includes reasoning or thinking). Each reader converts its own fields.
- **When.** After a hook with signal `turn-ended`, `turn-paused`, `turn-asked`, `failed` or `asking`. The read is async and never delays the hook answer.
- **Path check.** The path must be absolute, inside the user's home folder (after links are resolved), end in `.jsonl` or `.json`, be a regular file, and be at most 200 MB. Otherwise no usage is read and the hook works as before. A bad line is skipped.
- **Per stage.** The daemon keeps the last totals of each file. A read adds only what is new to the stage of the hook's terminal, so a resumed session, or one that goes on into the next stage, is not counted twice. A stage that is sent back and runs again adds to its own row. The stage of a hook:
  1. the stage reported from its terminal (`natamux done` or a send back), until the turn end that follows: the report moves the task on before the turn ends;
  2. else the stage of the terminal now;
  3. else the last stage of the terminal, such as an agent that background work woke after its stage passed;
  4. else none: `""`, shown as "No stage", for an agent that never ran a stage there, such as one started by hand. The daemon keeps 3 in memory, so after a restart such a turn has no stage.
- **When.** A file is read 2 s after the hook of a turn end, as the agent writes the last model call of the turn just after that hook. When the process of a shell ends (closed by a stage, the task finished, closed by hand, the agent quit), the last file named there is read once more, for the stage of rule 3, so a turn cut off adds its usage too. Usage files of tasks before T-0226 keep their numbers: their "No stage" rows hold turns that reported a stage.
- **Stored** in `<data dir>/tasks/<id>.usage.json`. Deleting the task deletes it; removing the workspace keeps it.
- **USD** is an estimate at API list prices, from a built-in table (`model-prices.ts`, read 2026-10-21; Claude, OpenAI and Gemini models). A model id is matched in lower case, `.` as `-`, without a date suffix. A model with no price shows tokens, no cost, and is listed in `unpricedModels`. The cost is computed when sent, never stored. A subscription (Claude Max, ChatGPT, Copilot plan) does not pay this.

## Order of work

1. (Done) Move the mapping out of the Claude Code adapter into the shared adapter interface. Add `natamux hook <agent>` (guarded: it does nothing without `NATAMUX_TOKEN`, always exits 0 and prints nothing) and `POST /hooks/:agent`. Claude Code keeps working.
2. Add Codex (its input is the same shape, so it is the cheapest second adapter). Use its `Interrupt` hook.
3. Add Gemini CLI.
4. (Done) A "no sign of life" rule for running tasks, shown as "quiet" on the dashboard.
5. Add Cursor and Copilot CLI after reading their payloads in full.
6. The agent runner with Tier 1 detection (output quiet, bell, OSC 9).
7. (Done 2026-11-17) OpenCode plugin, Qwen Code and Droid hooks; the agent runner follows Amp, Kimi, Goose, Aider, Crush, Cline, Kiro and Muse Code by their terminal.
8. A Muse Code hook file (needs `managed_hooks_env_vars` so `natamux hook` sees `NATAMUX_TOKEN`), then ACP.

Each adapter needs a real session to check it. Until then it is tested only against fake input.

## Sources

- Claude Code hooks: https://code.claude.com/docs/en/hooks and https://code.claude.com/docs/en/hooks-guide
- Codex source: https://github.com/openai/codex (`codex-rs/hooks`, `codex-rs/config/src/hook_config.rs`)
- Gemini CLI hooks: https://geminicli.com/docs/hooks/reference/
- Cursor hooks: https://cursor.com/docs/agent/hooks
- GitHub Copilot CLI hooks: https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-hooks-reference
- OpenCode plugins: https://opencode.ai/docs/plugins/
- The 11 agents added on 2026-11-17: see "Added on 2026-11-17" above.
- Agent Client Protocol: https://agentclientprotocol.com/overview/introduction

## Agents without hooks

Any agent started in a task's first terminal is followed by its output (`generic-signals.ts`):

- Output is a sign of life. A running task whose terminal stays silent for 10 minutes shows as quiet.
- The terminal bell (`\x07`) moves a running task to needs input. Claude Code, Codex, Gemini and OpenCode use their wait hooks instead; Cursor, Copilot, Qwen Code and Droid still use the bell as a fallback. Qwen Code runs project hooks only after the user approved them, so until then only the bell tells that it waits.
- `natamux ask` and `natamux done` let such an agent report the rest (see the plugin skill in `plugins/natamux`).

## Ask mode in Strict mode

In Strict mode Natamux starts an agent only in a mode where it asks before each action (`crates/natamux-agents/src/launch_ask.rs`). The words come before the user's own `NATAMUX_<AGENT>_ARGS`, and auto mode is off. An agent with no documented ask mode does not start, and has no one-shot run (commit message drafts) either. A test fails when an adapter has no row.

| Agent | How it asks |
|---|---|
| Claude Code | `--permission-mode default` |
| Codex | `--ask-for-approval on-request --sandbox workspace-write` |
| Gemini CLI | `--approval-mode default` |
| Qwen Code | `--approval-mode default` |
| Goose | `GOOSE_MODE=approve` before the command |
| Copilot, Cursor, OpenCode, Droid, Amp, Kimi, Aider, Crush, Cline, Kiro, Muse Code, any other CLI | blocked |

Codex's `workspace-write` sandbox has no network, so a `natamux` command of a stage asks for approval before it reaches the daemon. That is the point of the mode.
