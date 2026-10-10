---
title: Codex
description: Run the OpenAI Codex CLI in a Natamux task.
sidebar:
  order: 2
---

Natamux starts Codex with hooks for that session only.

## Before you start

Install Codex and log in. Follow the [Codex project page](https://github.com/openai/codex). Check that `codex` runs in your terminal.

## What Natamux runs

| | |
|---|---|
| Program | `codex` |
| Command line | `codex --dangerously-bypass-hook-trust -c 'hooks.<Event>=[…]' [extra args] '<prompt>'` |
| Change the program | `NATAMUX_CODEX_COMMAND` |
| Add arguments | `NATAMUX_CODEX_ARGS` |

Set the variables in the environment of the daemon (or the desktop app) before it starts. Start the agent from the dashboard, or:

```sh
natamux agent start T-42 --agent codex
```

## How hooks are added

Natamux passes one `-c hooks.<Event>=…` override for each event. Each hook runs `natamux hook codex`. Your own Codex settings do not change.

:::caution
Codex runs a hook only after you trust it. The hooks change with the daemon's address, so Natamux passes `--dangerously-bypass-hook-trust`. This flag also skips the review of the session's other hooks, including those of a project you already trust. To avoid the flag, start Codex by hand in the task's terminal. Its hooks then come from its own settings, and Natamux sees only the terminal.
:::

## Events and states

| Hook event | Meaning | Task state |
|---|---|---|
| `UserPromptSubmit` | A prompt was sent | running |
| `PreToolUse`, `PostToolUse` | The agent works | no change |
| `PermissionRequest` | Codex asks for permission | needs input |
| `Stop`, `Interrupt` | The turn ended, or you pressed Esc | needs input |

Codex has no failure hook, so a failed turn does not move the task to failed.

## Tested

Not run against a real Codex session. The hooks follow Codex's source code and are tested with made-up input.

## Limits

- No failed state.
- If `codex` is not found, the task fails in the first 15 seconds with a message that names `NATAMUX_CODEX_COMMAND`.
