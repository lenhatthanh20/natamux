---
title: GitHub Copilot CLI
description: Run GitHub Copilot CLI in a Natamux task.
sidebar:
  order: 4
---

Natamux starts Copilot CLI with a hook file in the task's worktree.

## Before you start

Install Copilot CLI and sign in. Follow the [GitHub documentation](https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-hooks-reference) for the CLI. Check that `copilot` runs in your terminal.

## What Natamux runs

| | |
|---|---|
| Program | `copilot` |
| Command line | `copilot --allow-all-tools [extra args] -i '<prompt>'` |
| Permission flag | `--allow-all-tools`, left out with `"agents": { "permissionMode": "ask" }` in [`.natamux.json`](/docs/reference/configuration/#agents) |
| Change the program | `NATAMUX_COPILOT_COMMAND` |
| Add arguments | `NATAMUX_COPILOT_ARGS` |

For example, to pick the model:

```sh
export NATAMUX_COPILOT_ARGS="--model gpt-5"
```

Start the agent from the dashboard, or:

```sh
natamux agent start T-42 --agent copilot
```

## How hooks are added

Natamux writes `.github/hooks/natamux.json` in the task's worktree and adds it to the repository's `.git/info/exclude`, so git ignores it. Copilot does not say which event a hook is for, so each hook command names it, for example `natamux hook copilot agentStop`.

If a file of your own with that name exists, Natamux leaves it alone. The agent then runs without Natamux's hooks, and the dashboard follows its terminal only.

## Events and states

| Hook event | Meaning | Task state |
|---|---|---|
| `userPromptSubmitted` | A prompt was sent | running |
| `preToolUse`, `postToolUse`, `postToolUseFailure` | The agent works | no change |
| `permissionRequest` | Copilot asks for permission | needs input |
| `agentStop` | The turn ended | needs input |
| `errorOccurred` with `recoverable: false` | An error ended the work | failed |

A recoverable error is ignored.

## Tested

A real Copilot CLI 1.0.94 session.

## Limits

- Copilot may ring the terminal bell when it starts. Natamux ignores a bell in the first 15 seconds after launch.
- Copilot's hooks may be off until its folder is trusted. Until then, the terminal bell moves a running task to needs input.
