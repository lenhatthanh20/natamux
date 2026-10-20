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

Copilot also runs the hooks in Claude Code's `.claude/settings.local.json`, which Natamux writes in the same worktree (see [Claude Code](/docs/agents/claude-code/#how-hooks-are-added)). They do nothing outside Claude Code, so each Copilot event is reported once. Copilot allows an `http` hook only on `https://`: if your own `.claude` settings have an `http` hook on `http://127.0.0.1`, Copilot skips that file and prints a warning at start.

Natamux also adds the worktree to `trustedFolders` in `~/.copilot/config.json`, so Copilot does not ask whether you trust it. See [Folder trust](/docs/concepts/agents-and-hooks/#folder-trust).

## Events and states

| Hook event | Meaning | Task state |
|---|---|---|
| `userPromptSubmitted` | A prompt was sent | running |
| `preToolUse`, `postToolUse`, `postToolUseFailure` | The agent works | no change |
| `notification` with type `permission_prompt` or `elicitation_dialog` | Copilot asks for permission, or asks you a question | needs input |
| `agentStop` | The turn ended | needs input |
| `errorOccurred` with `recoverable: false` | An error ended the work | failed |

A recoverable error is ignored. Other notifications, such as `shell_completed`, change nothing. Natamux does not use `permissionRequest`: Copilot sends it before it checks its permission rules, so also for each tool that `--allow-all-tools` lets run at once.

## Skills

| Where | Folder | Shown as |
|---|---|---|
| Project | `.github/skills`, `.claude/skills` or `.agents/skills`, then `<name>/SKILL.md` | **Project** |
| You | `~/.copilot/skills` or `~/.agents/skills`, then `<name>/SKILL.md` | **Only on this machine** |

- Natamux does not read Copilot plugin folders.
- `{skill:<name>}` becomes `/<name>` when Copilot CLI has the skill. Otherwise it becomes the path of a `SKILL.md` of that name, or `the <name> skill` when none exists.

Source: [Add skills to Copilot CLI](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/add-skills), checked 2026-10-11.

For the rules in general, see [Skills in a prompt](/docs/concepts/stages/#skills-in-a-prompt). Agents change quickly: check the vendor docs if a skill is not found.

## Usage

Supported. After a turn ends, Natamux reads the tokens from `~/.copilot/session-state/<session>/events.jsonl`. See [Usage](/docs/guides/dashboard/#usage).

## Tested

A real Copilot CLI 1.0.94 session.

## Limits

- Copilot runs in its own full screen, so its output does not go into the terminal history. Its scrollbar shows as a light line at the right edge. Copilot CLI (checked on 1.0.98) has no flag or setting to turn the full screen off.
- Copilot may ring the terminal bell when it starts. Natamux ignores a bell in the first 15 seconds after launch.
- Copilot's hooks may be off until its folder is trusted. Without `~/.copilot/config.json` (Copilot never ran), Natamux cannot trust the folder, and Copilot asks once. Until then, the terminal bell moves a running task to needs input.
