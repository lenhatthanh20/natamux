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

## Report from a prompt or a script

Inside a Natamux terminal, `NATAMUX_TOKEN`, `NATAMUX_URL`, `NATAMUX_TASK_ID` and `NATAMUX_TERMINAL` are set, so the commands know the task.

```sh
natamux ask "Keep the old CSV columns, or drop them?"
natamux done
```

Tell the agent about both commands in its instructions. For the options, see the [CLI reference](/natamux/docs/reference/cli/).

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

See the [CLI reference](/natamux/docs/reference/cli/) for the event types.
