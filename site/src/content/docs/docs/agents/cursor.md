---
title: Cursor CLI
description: Run the Cursor CLI agent in a Natamux task.
sidebar:
  order: 5
---

Natamux starts the Cursor CLI agent with a hook file in the task's worktree.

## Before you start

Install the Cursor CLI and log in. Follow the [Cursor documentation](https://cursor.com/docs). Check that `cursor-agent` runs in your terminal. Natamux also knows the agent as `agent`, the name the Cursor installer adds, when you start it yourself in a terminal.

## What Natamux runs

| | |
|---|---|
| Program | `cursor-agent` |
| Command line | `cursor-agent --force [extra args] '<prompt>'` |
| Full screen | None: Cursor CLI runs inline (checked on 2026.10.14), so its output goes into the terminal history |
| Permission flag | `--force`, left out with `"agents": { "permissionMode": "ask" }` in [`.natamux.json`](/docs/reference/configuration/#agents) |
| Change the program | `NATAMUX_CURSOR_COMMAND` |
| Add arguments | `NATAMUX_CURSOR_ARGS` |

Start the agent from the dashboard, or:

```sh
natamux agent start T-42 --agent cursor
```

## How hooks are added

Natamux writes `.cursor/hooks.json` in the task's worktree each time a terminal opens, and adds it to `.git/info/exclude`. So Cursor started by hand in any terminal reports too. Each hook runs `natamux hook cursor`. It prints `{}` because Cursor blocks a shell command when a hook's answer is not JSON.

If a file of your own with that name exists, Natamux leaves it alone, and the dashboard follows the terminal only.

Natamux also writes the Cursor CLI's trust file for the worktree, so `cursor-agent` does not ask whether you trust it. See [Folder trust](/docs/concepts/agents-and-hooks/#folder-trust).

## Events and states

| Hook event | Meaning | Task state |
|---|---|---|
| `beforeSubmitPrompt` | A prompt was sent | running |
| `preToolUse`, `postToolUse`, `postToolUseFailure`, `beforeShellExecution`, `afterShellExecution`, `afterFileEdit` | The agent works | no change |
| `stop` | The turn ended | needs input |
| `stop` with `status: error` | The turn failed | failed |

The Cursor CLI has no hook for waiting on your approval. The terminal bell moves a running task to needs input instead.

Cursor started by hand in an extra terminal gets these states for its terminal only. The task keeps its own. The bell does not count there.

## Skills

| Where | Folder | Shown as |
|---|---|---|
| Project | `.agents/skills`, `.cursor/skills`, `.claude/skills` or `.codex/skills`, then `<name>/SKILL.md` | **Project** |
| You | the same four folders under `~` | **Only on this machine** |

- The Cursor docs do not say that the CLI calls skills by name, and a forum thread says the CLI misses some. So `{skill:<name>}` never becomes a call. It becomes the path of a `SKILL.md` of that name, or `the <name> skill` when none exists.

Source: [Cursor skills](https://cursor.com/docs/context/skills), checked 2026-10-11.

For the rules in general, see [Skills in a prompt](/docs/concepts/stages/#skills-in-a-prompt). Agents change quickly: check the vendor docs if a skill is not found.

## Tested

Not run against a real Cursor CLI session. The hooks follow its documentation and are tested with made-up input. The Cursor CLI may not send every event the editor sends.

## Limits

- The wait for approval depends on the terminal bell.
