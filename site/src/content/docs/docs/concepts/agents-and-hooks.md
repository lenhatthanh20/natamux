---
title: Agents and hooks
description: How Natamux starts an agent in a task terminal and learns what the agent is doing.
sidebar:
  order: 4
---

Natamux does not replace your coding agent. It starts the agent in a terminal that it owns, and it listens to **hooks**, small callbacks that the agent runs when something happens. Hooks tell Natamux when the agent works, waits for you, finishes a turn, or fails.

## How Natamux starts an agent

When a stage needs an agent, Natamux:

1. Writes the full command, with the first prompt and the hook settings, into a small script in its data folder. The file is named `agent-launch-T-0001.sh`.
2. Types a short line into the task terminal that runs the script. The settings path and the prompt do not fill your screen.
3. Records an `agent.launched` event and sets the `agent` field of the task.

On Windows, Natamux types the command itself, because PowerShell can refuse scripts.

The agent starts in its interactive mode, in the worktree of the task. It stays open, so later stages send their prompts to the same agent. Stages that run side by side start another agent in another terminal.

The default program for each agent:

| Agent | Id | Program | Command variable | Extra arguments variable |
|---|---|---|---|---|
| [Claude Code](/docs/agents/claude-code/) | `claude-code` | `claude` | `NATAMUX_CLAUDE_COMMAND` | `NATAMUX_CLAUDE_ARGS` |
| [Codex](/docs/agents/codex/) | `codex` | `codex` | `NATAMUX_CODEX_COMMAND` | `NATAMUX_CODEX_ARGS` |
| [Gemini CLI](/docs/agents/gemini/) | `gemini` | `gemini` | `NATAMUX_GEMINI_COMMAND` | `NATAMUX_GEMINI_ARGS` |
| [Copilot CLI](/docs/agents/copilot/) | `copilot` | `copilot` | `NATAMUX_COPILOT_COMMAND` | `NATAMUX_COPILOT_ARGS` |
| [Cursor CLI](/docs/agents/cursor/) | `cursor` | `cursor-agent` | `NATAMUX_CURSOR_COMMAND` | `NATAMUX_CURSOR_ARGS` |

Set these variables when you start the daemon. See [Environment variables](/docs/reference/environment-variables/). For any other agent, see [Other agents](/docs/agents/other-agents/).

By default each agent starts with a flag that makes it ask for permission less often, for example `--permission-mode auto` for Claude Code. See [`agents`](/docs/reference/configuration/#agents) for the flag of each agent and how to turn it off.

The agent that starts is, in this order: the one you choose when you start it, the `agent` of the task, or `claude-code`.

## How Natamux knows what the agent does

Every agent has its own hook names. Natamux turns them into five common signals:

| Signal | Meaning | Effect on the task |
|---|---|---|
| `prompt` | A prompt was sent to the agent. | A `draft` or `ready` task becomes `running`. A task in `needs-input` or `review` becomes `running` again. |
| `activity` | The agent ran a tool. | A task in `needs-input` becomes `running`, because a permission was answered. Otherwise the dashboard shows that the agent is alive. |
| `asking` | The agent waits for you: a permission or a question. | A `running` task becomes `needs-input`. |
| `turn-ended` | The agent finished its turn. | A `running` task becomes `needs-input`, unless the agent ran `natamux done` first. |
| `failed` | The agent stopped with an error. | A `running` or `needs-input` task becomes `failed`. |

A turn that ends without `natamux done` means the agent stopped early, for example to ask something in plain text. The stage is done only when the agent runs `natamux done`. Then the checks run. See [Stages](/docs/concepts/stages/).

Hooks also send a one-line summary of what the agent does now, such as `Edit src/a.ts`. The dashboard shows it.

### How a hook finds its task

1. The `NATAMUX_TASK_ID` variable of the terminal, when it is set. Terminals that Natamux opens set it.
2. Otherwise, the task whose worktree holds the working directory of the agent.

`NATAMUX_TERMINAL` tells which stage of a group the agent works on.

## Hook setup for each agent

Natamux adds the hooks for one run of the agent. It does not change your own agent settings.

| Agent | How the hooks are added | Where | In git? |
|---|---|---|---|
| Claude Code | `--settings <file>` on the command line. The hooks post to the daemon over HTTP. | `claude-code-settings.json` in the data folder | No |
| Codex | `-c hooks.<Event>=...` for each hook event, and `--dangerously-bypass-hook-trust` | The command line | No |
| Gemini CLI | The variable `GEMINI_CLI_SYSTEM_SETTINGS_PATH` points to a settings file | `gemini-settings.json` in the data folder | No |
| Copilot CLI | A hook file in the worktree | `.github/hooks/natamux.json` | Kept out of git |
| Cursor CLI | A hook file in the worktree | `.cursor/hooks.json` | Kept out of git |

Notes:

- Claude Code reads the token and the task id from its environment when it sends a hook.
- The other agents run `natamux hook <agent>` as a command. It reads the hook JSON from stdin and sends it to the daemon. It prints nothing, and it never makes the agent fail. If the daemon is down, it exits quietly. Outside a Natamux terminal it does nothing.
- Codex asks you to trust every hook it has not seen, and Natamux hooks change with the daemon. `--dangerously-bypass-hook-trust` skips that question for runs that Natamux starts.
- For Copilot and Cursor, Natamux writes the hook file in the worktree and adds its path to the ignore list of your repository (`.git/info/exclude`). The file never reaches a commit.
- If the worktree already has a hook file at that path that is not from Natamux, Natamux leaves it alone. The agent then runs without Natamux hooks. The terminal still works.

`natamux hooks claude-code` prints the Claude Code settings, for use in a setup that Natamux did not start.

## Launch failures

Natamux reads the terminal for 15 seconds after it types the launch line. If the shell prints that it cannot find the agent program (`command not found`, `not found`, `unknown command` or `is not recognized`), the task moves to `failed` with a message such as:

> The agent did not start: "zsh: command not found: claude". Install claude, or set NATAMUX_CLAUDE_COMMAND to its command, then start the agent again.

Fix the problem, then start the agent again.

## Agents without hooks

You can use any command line agent in a task terminal. Without hooks, Natamux has less to go on. Two fallbacks help:

- **The terminal bell.** Many tools ring the bell when they wait for you. A `running` task whose agent has no hooks that report waiting becomes `needs-input` when the bell rings. This applies to Copilot CLI, Cursor CLI and unknown agents. Natamux ignores a bell in the first 15 seconds after the launch, because some agents ring it at start. Agents with hooks (Claude Code, Codex, Gemini CLI) do not use the bell.
- **Commands the agent can run.** Tell the agent to run these in its shell:

| Command | Effect |
|---|---|
| `natamux ask "question"` | The task becomes `needs-input`. |
| `natamux done` | The stage is done, and the checks run. |

```bash
natamux ask "Which database should I use?"
natamux done
```

Both read the task from `NATAMUX_TASK_ID`. Use `--task` and `--stage` if you run them outside a task terminal or inside a group.

Natamux also treats terminal output as a sign of life, so a running task is not marked quiet while the agent prints.

## Related

- [Tasks](/docs/concepts/tasks/): the states that hooks move
- [Stages](/docs/concepts/stages/): `natamux done` and the checks
- [Agents](/docs/agents/claude-code/): one page for each agent
