---
title: OpenCode
description: Run OpenCode in a Natamux task.
sidebar:
  order: 6
---

Natamux starts OpenCode with a plugin file in the task's worktree. The plugin reports each state.

## Before you start

Install OpenCode and connect a provider. Follow the [OpenCode documentation](https://opencode.ai/docs/). Check that `opencode` runs in your terminal.

OpenCode 2 may install as `opencode2`. If so, set `NATAMUX_OPENCODE_COMMAND=opencode2`.

## What Natamux runs

| | |
|---|---|
| Program | `opencode` |
| Command line | `opencode --auto [extra args] --prompt '<prompt>'` |
| Permission flag | `--auto`, left out with `"agents": { "permissionMode": "ask" }` in [`.natamux.json`](/docs/reference/configuration/#agents) |
| Model | `--model '<name>'` from the Model field of **Start a task**, in the form `provider/model` |
| Resume | `--session <id>`, or `--continue` for the newest session of the folder |
| Change the program | `NATAMUX_OPENCODE_COMMAND` |
| Add arguments | `NATAMUX_OPENCODE_ARGS` |

Start the agent from the dashboard, or:

```sh
natamux agent start T-42 --agent opencode
```

## How hooks are added

OpenCode has plugins, not hook files. Natamux writes `.opencode/plugins/natamux.js` in the task's worktree each time a terminal opens, and adds it to `.git/info/exclude`. The plugin runs `natamux hook opencode <event>` and never stops OpenCode when that fails.

If a file of your own with that name exists, Natamux leaves it alone, and the dashboard follows the terminal only.

## Events and states

| Plugin event | Meaning | Task state |
|---|---|---|
| `chat.message` | A prompt was sent | running |
| `tool.execute.before`, `tool.execute.after` | The agent works | no change |
| `permission.asked`, `permission.v2.asked` | OpenCode asks for permission | needs input |
| `session.idle` | The turn ended | idle |
| `session.error` | The turn failed | failed |

## Skills

| Where | Folder | Shown as |
|---|---|---|
| Project | `.opencode/skills`, `.claude/skills` or `.agents/skills`, then `<name>/SKILL.md` | **Project** |
| You | `~/.config/opencode/skills`, `~/.claude/skills` or `~/.agents/skills` | **Only on this machine** |

Source: [OpenCode skills](https://opencode.ai/docs/skills/), checked 2026-11-17.

For the rules in general, see [Skills in a prompt](/docs/concepts/stages/#skills-in-a-prompt).

## Usage

Not supported. The dashboard shows no token numbers for OpenCode.

## Tested

Not run against a real OpenCode session. The plugin follows OpenCode's docs and source, and is tested with made-up input.

## Limits

- The plugin folder is `.opencode/plugins/`, the one the OpenCode 2 docs name.
- No known sign-in screen line, so a task that waits for a login is not marked "sign in".
- If `opencode` is not found, the task fails in the first 30 seconds with a message that names `NATAMUX_OPENCODE_COMMAND`.
