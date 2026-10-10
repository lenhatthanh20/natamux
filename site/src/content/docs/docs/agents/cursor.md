---
title: Cursor CLI
description: Run the Cursor CLI agent in a Natamux task.
sidebar:
  order: 5
---

Natamux starts the Cursor CLI agent with a hook file in the task's worktree.

## Before you start

Install the Cursor CLI and log in. Follow the [Cursor documentation](https://cursor.com/docs). Check that `cursor-agent` runs in your terminal.

## What Natamux runs

| | |
|---|---|
| Program | `cursor-agent` |
| Command line | `cursor-agent [extra args] '<prompt>'` |
| Change the program | `NATAMUX_CURSOR_COMMAND` |
| Add arguments | `NATAMUX_CURSOR_ARGS` |

Start the agent from the dashboard, or:

```sh
natamux agent start T-42 --agent cursor
```

## How hooks are added

Natamux writes `.cursor/hooks.json` in the task's worktree and adds it to `.git/info/exclude`. Each hook runs `natamux hook cursor`. It prints `{}` because Cursor blocks a shell command when a hook's answer is not JSON.

If a file of your own with that name exists, Natamux leaves it alone, and the dashboard follows the terminal only.

## Events and states

| Hook event | Meaning | Task state |
|---|---|---|
| `beforeSubmitPrompt` | A prompt was sent | running |
| `preToolUse`, `postToolUse`, `postToolUseFailure`, `beforeShellExecution`, `afterShellExecution`, `afterFileEdit` | The agent works | no change |
| `stop` | The turn ended | needs input |
| `stop` with `status: error` | The turn failed | failed |

The Cursor CLI has no hook for waiting on your approval. The terminal bell moves a running task to needs input instead.

## Tested

Not run against a real Cursor CLI session. The hooks follow its documentation and are tested with made-up input. The Cursor CLI may not send every event the editor sends.

## Limits

- The wait for approval depends on the terminal bell.
