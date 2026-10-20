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
| Command line | `GEMINI_CLI_TRUST_WORKSPACE=true GEMINI_CLI_SYSTEM_SETTINGS_PATH=<file> gemini --approval-mode yolo [extra args] -i '<prompt>'` |
| Full screen | Off: the settings file sets `ui.useAlternateBuffer` to `false`, so Gemini runs inline and its output goes into the terminal history |
| Permission flag | `--approval-mode yolo`, left out with `"agents": { "permissionMode": "ask" }` in [`.natamux.json`](/docs/reference/configuration/#agents) |
| Change the program | `NATAMUX_GEMINI_COMMAND` |
| Add arguments | `NATAMUX_GEMINI_ARGS` |

`GEMINI_CLI_TRUST_WORKSPACE` trusts the task's worktree, so Gemini does not ask when you turned on folder trust. See [Folder trust](/docs/concepts/agents-and-hooks/#folder-trust). On Windows, Natamux sets the variables with `$env:` in PowerShell. Start the agent from the dashboard, or:

```sh
natamux agent start T-42 --agent gemini
```

## How hooks are added

The daemon writes `gemini-settings.json` into its data folder. Gemini CLI reads it as its system settings. It holds only the hooks and `ui.useAlternateBuffer`, so your other settings stay as they are. Each hook runs `natamux hook gemini`, which prints `{}` because Gemini reads the answer as JSON.

## Events and states

| Hook event | Meaning | Task state |
|---|---|---|
| `BeforeAgent` | A prompt was sent | running |
| `BeforeTool`, `AfterTool` | The agent works | no change |
| `Notification` with type `ToolPermission` | Gemini waits for permission | needs input |
| `AfterAgent` | The turn ended | needs input |

## Skills

| Where | Folder | Shown as |
|---|---|---|
| Project | `.gemini/skills/<name>/SKILL.md` or `.agents/skills/<name>/SKILL.md` | **Project** |
| You | `~/.gemini/skills/<name>/SKILL.md` or `~/.agents/skills/<name>/SKILL.md` | **Only on this machine** |
| Extension | `~/.gemini/extensions/<extension>/skills/<name>/SKILL.md` | **Only on this machine** |

- Gemini CLI has no explicit call: the model picks a skill by its description. So `{skill:<name>}` never becomes a call. It becomes the path of a `SKILL.md` of that name, or `the <name> skill` when none exists.

Source: [Gemini CLI skills](https://geminicli.com/docs/cli/skills/), checked 2026-10-11.

For the rules in general, see [Skills in a prompt](/docs/concepts/stages/#skills-in-a-prompt). Agents change quickly: check the vendor docs if a skill is not found.

## Usage

Supported. After a turn ends, Natamux reads the tokens from the session chat file. Not run against a real session yet. See [Usage](/docs/guides/dashboard/#usage).

## Tested

Not run against a real Gemini CLI session. The hooks follow its documentation and are tested with made-up input.

## Limits

- Gemini CLI has no failure hook. A turn that ends in an error leaves the task running. After 10 minutes with no sign of life, the dashboard marks it quiet.
- Other notification types are ignored.
