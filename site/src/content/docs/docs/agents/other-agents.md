---
title: Other agents
description: Use any command line agent in a Natamux task, with the terminal bell and the ask and done commands.
sidebar:
  order: 6
---

Natamux has hook support for five agents: Claude Code, Codex, Gemini CLI, GitHub Copilot CLI and Cursor. Any other command line agent still works. Start it in the task's first terminal.

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
