---
title: Gemini CLI
description: Run Gemini CLI in a Natamux task.
sidebar:
  order: 3
---

Natamux starts Gemini CLI with hooks for that session only.

## Before you start

Install Gemini CLI and sign in. Follow the [Gemini CLI documentation](https://geminicli.com/docs/). Check that `gemini` runs in your terminal.

:::note
Per `docs/agent-adapters.md`, the Gemini CLI free tier was replaced by the Antigravity CLI on 2026-06-18. Paid accounts keep `gemini`.
:::

## What Natamux runs

| | |
|---|---|
| Program | `gemini` |
| Command line | `GEMINI_CLI_SYSTEM_SETTINGS_PATH=<file> gemini --approval-mode yolo [extra args] -i '<prompt>'` |
| Permission flag | `--approval-mode yolo`, left out with `"agents": { "permissionMode": "ask" }` in [`.natamux.json`](/docs/reference/configuration/#agents) |
| Change the program | `NATAMUX_GEMINI_COMMAND` |
| Add arguments | `NATAMUX_GEMINI_ARGS` |

On Windows, Natamux sets the variable with `$env:` in PowerShell. Start the agent from the dashboard, or:

```sh
natamux agent start T-42 --agent gemini
```

## How hooks are added

The daemon writes `gemini-settings.json` into its data folder. Gemini CLI reads it as its system settings, so your own settings stay as they are. Each hook runs `natamux hook gemini`, which prints `{}` because Gemini reads the answer as JSON.

## Events and states

| Hook event | Meaning | Task state |
|---|---|---|
| `BeforeAgent` | A prompt was sent | running |
| `BeforeTool`, `AfterTool` | The agent works | no change |
| `Notification` with type `ToolPermission` | Gemini waits for permission | needs input |
| `AfterAgent` | The turn ended | needs input |

## Tested

Not run against a real Gemini CLI session. The hooks follow its documentation and are tested with made-up input.

## Limits

- Gemini CLI has no failure hook. A turn that ends in an error leaves the task running. After 10 minutes with no sign of life, the dashboard marks it quiet.
- Other notification types are ignored.
