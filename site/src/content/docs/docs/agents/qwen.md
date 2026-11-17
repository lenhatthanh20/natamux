---
title: Qwen Code
description: Run Qwen Code in a Natamux task.
sidebar:
  order: 7
---

Natamux starts Qwen Code with a hook file in the task's worktree.

## Before you start

Install Qwen Code and sign in. Follow the [Qwen Code project page](https://github.com/QwenLM/qwen-code). Check that `qwen` runs in your terminal.

## What Natamux runs

| | |
|---|---|
| Program | `qwen` |
| Command line | `qwen --approval-mode yolo [extra args] -i '<prompt>'` |
| Prompt | `-i` runs the prompt and stays open. Without it, Qwen runs the prompt once and ends |
| Permission flag | `--approval-mode yolo`, left out with `"agents": { "permissionMode": "ask" }` in [`.natamux.json`](/docs/reference/configuration/#agents) |
| Model | `--model '<name>'` from the Model field of **Start a task** |
| Resume | `--resume <id>`, or `--continue` for the newest session of the folder |
| Change the program | `NATAMUX_QWEN_COMMAND` |
| Add arguments | `NATAMUX_QWEN_ARGS` |

Start the agent from the dashboard, or:

```sh
natamux agent start T-42 --agent qwen
```

## How hooks are added

Natamux writes `.qwen/settings.json` in the task's worktree each time a terminal opens, and adds it to `.git/info/exclude`. Each hook runs `natamux hook qwen`.

If a file of your own with that name exists, Natamux leaves it alone, and the dashboard follows the terminal only.

:::caution
Qwen Code does not run project hooks until you approve them. It asks once per workspace, and asks again when the file changes. Only `--approval-mode yolo` on the command line skips the question. The `auto` permission mode passes it. With `ask`, the first start waits at that question: answer it in the task's terminal.
:::

## Events and states

| Hook event | Meaning | Task state |
|---|---|---|
| `UserPromptSubmit` | A prompt was sent | running |
| `PreToolUse`, `PostToolUse`, `PostToolUseFailure` | The agent works | no change |
| `PermissionRequest`, `Notification` (`permission_prompt`) | Qwen asks for permission | needs input |
| `Stop` | The turn ended | idle |
| `StopFailure` | The turn failed | failed |

## Skills

| Where | Folder | Shown as |
|---|---|---|
| Project | `.qwen/skills/<name>/SKILL.md` | **Project** |
| You | `~/.qwen/skills` or `~/.agents/skills` | **Only on this machine** |

Source: the skills page in the [Qwen Code docs](https://github.com/QwenLM/qwen-code), checked 2026-11-17.

For the rules in general, see [Skills in a prompt](/docs/concepts/stages/#skills-in-a-prompt).

## Usage

Not supported. The dashboard shows no token numbers for Qwen Code.

## Tested

Not run against a real Qwen Code session. The hooks follow its docs and are tested with made-up input.

## Limits

- Project hooks need the approval above.
- No known sign-in screen line, so a task that waits for a login is not marked "sign in".
- If `qwen` is not found, the task fails in the first 30 seconds with a message that names `NATAMUX_QWEN_COMMAND`.
