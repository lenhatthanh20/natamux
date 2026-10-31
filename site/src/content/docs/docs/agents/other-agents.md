---
title: Other agents
description: Use any command line agent in a Natamux task, with the terminal bell and the ask and done commands.
sidebar:
  order: 6
---

Natamux has hook support for five agents: Claude Code, Codex, Gemini CLI, GitHub Copilot CLI and Cursor. Any other command line agent still works. Add it as a [custom agent](#custom-agents-a-built-in-agent-with-your-own-command), or start it in the task's first terminal.

## Custom agents (a built-in agent with your own command)

A custom agent is an agent you add once in **Settings → Agents** of the desktop app. It then shows in every agent picker: the **Agent** field of a stage in the project settings, the agent menu of a stage card, and **Start a task**.

| Field | Meaning |
|---|---|
| **Id** | The name the pickers, the task and `.natamux.json` use, such as `claude-qwen`. Lower case words joined by `-`, up to 32 characters, not the id of a built-in agent |
| **Based on** | A built-in agent, or **Other CLI** |
| **Command** | The program in place of the built-in one, as your shell runs it. It can be a function or alias of your shell, or carry variables and words, such as `ANTHROPIC_MODEL=x claude` |

### Based on a built-in agent

The custom agent works exactly as its base: the same hooks, states, resume after a restart, permission flags, workflows and skills. Only the program differs. For example, with this function in `~/.zshrc`:

```zsh
cq() {
  ANTHROPIC_BASE_URL=https://llm.example.com \
  ANTHROPIC_MODEL=qwen3-27b \
  claude "$@"
}
```

add `claude-qwen`, based on **Claude Code**, with the command `cq`. A stage with the agent `claude-qwen` then starts `cq` where Claude Code starts `claude`. Other stages of the same task can still use Claude Code.

Natamux runs the start line in the task terminal's own shell, so the functions and aliases of your shell start file work. Commit message drafts do not load your shell start file, so they use the base's own program.

To pick a model, put its words in the stage's **Arguments** field, such as `--model x`, as for a built-in agent.

### Other CLI

An agent based on **Other CLI** starts as `<command> <stage arguments> '<prompt>'`: the prompt is the last word. Natamux has no hooks for it, so it sees only what [it sees without hooks](#what-natamux-sees-without-hooks): output, the bell, `natamux ask` and `natamux done`. A stage ends only when the agent or you run `natamux done`. It does not come back after a restart.

### Where the list is kept

The list is in `custom-agents.json` in the data folder, readable only by you. A project's `.natamux.json` names a custom agent by its id but never holds its command, so a cloned repository cannot choose a program to run. A stage that names a custom agent you do not have fails to start with "Unknown agent".

## What Natamux sees without hooks

| Signal | Effect |
|---|---|
| Output in the terminal | A sign of life. A running task whose terminal is silent for 10 minutes shows as quiet. |
| Terminal bell (`\x07`) | Moves a running task to needs input. |
| `natamux ask` | Moves a running task to needs input, with your question. |
| `natamux done` | Says the stage is done, so the daemon runs its checks. |

The bell counts only for agents without hooks, or whose hooks do not report a wait (Copilot and Cursor). A bell in the first 15 seconds after an agent launch is ignored.

## Skills

Natamux does not know where another agent keeps its skills, so it has no sure call for one. For `{skill:<name>}` in a stage prompt, and for a `skill` context source, it follows two rules:

1. When a `SKILL.md` of that name exists in the worktree, in your main checkout or on your machine (in the folder of any of the five agents), the prompt says `the skill in <path> (read that file and follow it)`. Most agents can read a file and follow it.
2. Otherwise the prompt says `the <name> skill`, and the stage card says "The skill <name> was not found for <agent>." The stage still starts.

See [Skills in a prompt](/docs/concepts/stages/#skills-in-a-prompt).

## Report from a prompt or a script

Inside a Natamux terminal, `NATAMUX_TOKEN`, `NATAMUX_URL`, `NATAMUX_TASK_ID` and `NATAMUX_TERMINAL` are set, so the commands know the task.

```sh
natamux ask "Keep the old CSV columns, or drop them?"
natamux done
```

Tell the agent about both commands in its instructions. For the options, see the [CLI reference](/docs/reference/cli/).

## Forward hook JSON

If an agent can run a command on an event, pipe its JSON to Natamux:

```sh
natamux hook <agent> [event]
```

- It reads the hook's JSON from stdin and posts it to `POST /hooks/<agent>` on the daemon.
- It prints nothing, and always exits 0, so it never breaks the agent. It prints `{}` for `cursor` and `gemini`.
- Without `NATAMUX_TOKEN`, it does nothing.
- `event` names the event when the JSON does not (Copilot needs it).
- Set `NATAMUX_DEBUG` to any value to see problems on stderr.

:::caution
The daemon turns a hook into task events only for agents it has an adapter for: `claude-code`, `codex`, `gemini`, `copilot` and `cursor`. For any other name it answers 404, and the hook has no effect. For other agents, use the bell, `natamux ask` and `natamux done`, or send events with `natamux event`.
:::

## Send events yourself

```sh
natamux event T-42 agent.asked --source agent --payload '{"question":"Which database?"}'
```

See the [CLI reference](/docs/reference/cli/) for the event types.

## Related

- [Mix agents in one task](/docs/guides/mix-agents/): an agent and model per stage
