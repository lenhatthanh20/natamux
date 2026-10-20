---
title: Claude Code
description: Run Claude Code in a Natamux task, and make a Claude Code session you started yourself report to the dashboard.
sidebar:
  order: 1
---

Claude Code is the default agent. Natamux starts it with hooks for that session only, so the dashboard knows when it works, asks or is done.

## Before you start

Install Claude Code and log in. Follow the [Claude Code documentation](https://code.claude.com/docs). Check that `claude` runs in your terminal.

## What Natamux runs

| | |
|---|---|
| Program | `claude` |
| Command line | `claude --settings <file> --permission-mode auto [extra args] '<prompt>'` |
| Permission flag | `--permission-mode auto`, left out with `"agents": { "permissionMode": "ask" }` in [`.natamux.json`](/docs/reference/configuration/#agents) |
| Change the program | `NATAMUX_CLAUDE_COMMAND` |
| Add arguments | `NATAMUX_CLAUDE_ARGS` |

Set the variables in the environment of the daemon (or the desktop app) before it starts:

```sh
export NATAMUX_CLAUDE_COMMAND=/opt/tools/claude
export NATAMUX_CLAUDE_ARGS="--model sonnet"
```

Start the agent from the dashboard (**Start a task**, or **Start agent** on a stage), or from the command line:

```sh
natamux agent start T-42 --agent claude-code
```

`claude-code` is the default value of `--agent`.

## Auto mode

Natamux starts Claude Code in auto mode, so it does not stop to ask "Do you want to proceed?" for each command. Claude Code still checks each action and asks when one looks risky. If your account or version has no auto mode, Claude Code says so in the task's terminal.

To get the prompts back, set `"agents": { "permissionMode": "ask" }` in [`.natamux.json`](/docs/reference/configuration/#agents). Claude Code then uses your own `permissions.defaultMode`. To use auto mode in every Claude Code session, also outside Natamux, set `"permissions": { "defaultMode": "auto" }` in `~/.claude/settings.json`.

## How hooks are added

Nothing changes in your own `~/.claude` files.

- **In the task's worktree.** Each Natamux terminal writes `.claude/settings.local.json` with one hook for each event below, and adds it to the repository's `.git/info/exclude`, so git ignores it. Each hook runs `natamux hook claude-code`, which sends the event to the daemon. So `claude` started by hand in a task terminal reports too. Other keys in that file, such as the permissions Claude Code saves there, stay.
- **With `--settings`.** If that file has hooks of your own, Natamux leaves it alone. It then starts Claude Code with `--settings <data dir>/claude-code-settings.json`, which holds one `http` hook for each event. These post to the daemon on loopback.

The hooks read `NATAMUX_TOKEN`, `NATAMUX_TASK_ID` and `NATAMUX_TERMINAL` from the environment of the Claude Code process. Natamux terminals set them. The token is never written into a settings file.

GitHub Copilot CLI reads `.claude/settings.local.json` too. `natamux hook claude-code` does nothing unless Claude Code runs it (Claude Code sets `CLAUDECODE=1`), so a Copilot session never reports as Claude Code.

## Events and states

| Hook event | Meaning | Task state |
|---|---|---|
| `UserPromptSubmit` | A prompt was sent | running |
| `PreToolUse`, `PostToolUse` | The agent works | no change (a tool that ran after a question moves the task back to running) |
| `PermissionRequest` | Claude Code is about to ask for permission | needs input |
| `Notification` (`permission_prompt`, `elicitation_dialog`, `agent_needs_input`) | Claude Code waits for you | needs input |
| `Stop` | The agent finished its answer | needs input. When background work is still running (a background shell, a monitor or a scheduled prompt), the agent only paused: the task stays running and says `Waiting for Bash: pnpm test`, until the work wakes the agent again. See [Waiting for background work](/docs/concepts/agents-and-hooks/#waiting-for-background-work) |
| `StopFailure` | An API error ended the turn | failed. The session goes on: type to the agent, or click **Start agent**, and the stage runs again |

Other events are ignored. A stage is done only when the agent runs `natamux done` or you mark it done. See [Stages](/docs/concepts/stages/).

## A session you start yourself

In a Natamux task terminal, plain `claude` is enough: the worktree has the hooks (see [How hooks are added](#how-hooks-are-added)). Do not add `--settings "$(natamux hooks claude-code)"` there, or Claude Code sends each event twice.

Elsewhere, such as a folder that Natamux did not prepare, or a worktree whose `.claude/settings.local.json` has hooks of your own, give it the hooks:

```sh
export NATAMUX_TOKEN=<token the daemon printed>
export NATAMUX_TASK_ID=T-42   # optional inside the task's workspace
claude --settings "$(natamux hooks claude-code)"
```

`natamux hooks claude-code` prints the settings JSON. It needs no token and does not call the daemon. It points the hooks at `NATAMUX_URL`, or `http://127.0.0.1:4317` when that is not set.

The daemon finds the task from `NATAMUX_TASK_ID`. Without it, it uses the task whose workspace holds the session's working directory.

:::caution
If your machine uses an HTTP proxy, Claude Code may send the hooks through it, and the proxy can refuse the call. Set `NO_PROXY="localhost,127.0.0.1"` and `no_proxy` before you start `claude`.
:::

## The Natamux plugin

The repository has a Claude Code plugin with one skill, `natamux-task`. It teaches Claude Code to work inside a task: read the task, ask you a question, report each stage with `natamux done`, and file follow-up tasks.

Install it in Claude Code:

```text
/plugin marketplace add lenhatthanh20/natamux
/plugin install natamux@natamux
```

The skill matters only in a session that Natamux started, because it needs `NATAMUX_TASK_ID`. The `natamux` command must be on the `PATH`, with `NATAMUX_TOKEN` and `NATAMUX_URL` set. Natamux terminals set them.

## Skills

Natamux looks for skills in the worktree of the task and on your machine:

| Where | Folder | Shown as |
|---|---|---|
| Project | `.claude/skills/<name>/SKILL.md` | **Project** |
| You | `~/.claude/skills/<name>/SKILL.md` | **Only on this machine** |
| Plugin, on in the repository's `.claude/settings.json` or `.claude/settings.local.json` | `<install path>/skills/<name>/SKILL.md` | **Plugins** |
| Plugin, on in `~/.claude/settings.json` only | the same | **Only on this machine** |

- A plugin is on when `enabledPlugins` has `"<plugin>@<marketplace>": true`. Natamux finds its install path in `~/.claude/plugins/installed_plugins.json`.
- A plugin skill is called `/<plugin>:<skill>`. A plugin that is on but not installed on this machine gives no skills, and the picker says so.
- `{skill:<name>}` becomes `/<name>` when Claude Code has the skill in the worktree. Otherwise it becomes the path of a `SKILL.md` of that name, or `the <name> skill` when none exists.
- `.claude/settings.local.json` is usually not committed, so a new worktree does not have it. A plugin that only this file turns on is found in your main checkout, and `{skill:<plugin>:<skill>}` becomes the path of its `SKILL.md`.

Sources: [skills](https://code.claude.com/docs/en/skills) and [plugins reference](https://code.claude.com/docs/en/plugins-reference), checked 2026-10-11.

For the rules in general, see [Skills in a prompt](/docs/concepts/stages/#skills-in-a-prompt). Agents change quickly: check the vendor docs if a skill is not found.

## Usage

Supported. After a turn ends, Natamux reads the tokens from the session transcript and its sub-agent files. See [Usage](/docs/guides/dashboard/#usage).

## Tested

Real Claude Code sessions. The permission path (`PermissionRequest` and the permission `Notification`) was tested with made-up input only.

## Limits

- `Stop` does not fire when you interrupt Claude Code with Esc. The task stays running. After 10 minutes with no hook, the dashboard marks it quiet.
- Because `Stop` fires after every answer, a task shows "needs input" at the end of each turn, even when the agent only asked a question in plain text.
- `--settings` adds to your own hooks. It does not replace them.
- In its default TUI, Claude Code may leave empty rows under its footer after a turn: when its live area gets shorter, the rows it freed stay empty. Natamux sizes the terminal right; this is how Claude Code draws. `/tui fullscreen` avoids it, and a wider pane makes the footer shorter.
