---
title: Factory Droid
description: Run the Factory Droid CLI in a Natamux task.
sidebar:
  order: 8
---

Natamux starts Droid with a hook file in the task's worktree.

## Before you start

Install Droid and sign in. Follow the [Factory documentation](https://docs.factory.com). Check that `droid` runs in your terminal.

## What Natamux runs

| | |
|---|---|
| Program | `droid` |
| Command line | `droid --auto high [extra args] '<prompt>'` |
| Permission flag | `--auto high`, left out with `"agents": { "permissionMode": "ask" }` in [`.natamux.json`](/docs/reference/configuration/#agents). The Factory docs describe it for `droid exec`; the interactive mode takes it too |
| Model | `--model '<name>'` from the Model field of **Start a task** |
| Resume | `--resume <id>`, or `--resume` alone for the newest session |
| Change the program | `NATAMUX_DROID_COMMAND` |
| Add arguments | `NATAMUX_DROID_ARGS` |

Start the agent from the dashboard, or:

```sh
natamux agent start T-42 --agent droid
```

## How hooks are added

Natamux writes `.factory/hooks.json` in the task's worktree each time a terminal opens, and adds it to `.git/info/exclude`. Each hook runs `natamux hook droid`.

If a file of your own with that name exists, Natamux leaves it alone, and the dashboard follows the terminal only.

## Events and states

| Hook event | Meaning | Task state |
|---|---|---|
| `UserPromptSubmit` | A prompt was sent | running |
| `PreToolUse`, `PostToolUse` | The agent works | no change |
| `Notification` (`permission_prompt`, `elicitation_dialog`) | Droid waits for you | needs input |
| `Stop` | The turn ended | idle |

Droid has no `PermissionRequest` and no failure hook. The terminal bell still moves a running task to needs input, and a failed turn does not move the task to failed.

## Skills

| Where | Folder | Shown as |
|---|---|---|
| Project | `.factory/skills/<name>/SKILL.md` | **Project** |
| You | `~/.factory/skills/<name>/SKILL.md` | **Only on this machine** |

Source: [Droid skills](https://docs.factory.com/cli/configuration/skills), checked 2026-11-17.

For the rules in general, see [Skills in a prompt](/docs/concepts/stages/#skills-in-a-prompt).

## Usage

Not supported. The dashboard shows no token numbers for Droid.

## Tested

Not run against a real Droid session. The hooks follow its docs and are tested with made-up input.

## Limits

- No failed state.
- No known sign-in screen line, so a task that waits for a login is not marked "sign in".
- If `droid` is not found, the task fails in the first 30 seconds with a message that names `NATAMUX_DROID_COMMAND`.
