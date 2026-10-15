---
title: Use the command line
description: Create tasks, move stages and let agents report from any terminal with natamux, short name nmx.
sidebar:
  order: 6
---

`natamux` (short name `nmx`) is the command line of Natamux. You use it to create and move tasks from a script or your own shell. Agents use it in their task terminals to say they are done or need you. Every option is in the [CLI reference](/docs/reference/cli/).

## Where it works

- **In a task terminal** of the desktop app or the dashboard, the command is ready. Natamux sets `NATAMUX_TOKEN`, `NATAMUX_URL`, `NATAMUX_TASK_ID` and `NATAMUX_TERMINAL`, so you can type `natamux done` with no options.
- **In your own shell**, add the app's `bin` folder to your `PATH` and set the token. See [Use the natamux command in your own shell](/docs/getting-started/install/#use-the-natamux-command-in-your-own-shell).

## Plan work from your shell

```sh
# A task in the backlog, with a priority and labels
nmx task create "Fix the login redirect" --priority high --label bug --label auth

# A task whose context is a GitHub issue
nmx task create "Add CSV export" --context github-issue:42

# A task with your own id, and a screenshot for the agent
nmx task create "ABC-12: Fix the date picker" --attach picker.png

# A task that waits for two others
nmx task create "Add the profile page" --depends-on T-0004 --depends-on T-0005

nmx task list --state needs-input
```

Start a task with an agent, in its own worktree:

```sh
nmx agent start T-0007 --agent codex
nmx agent start T-0008 --base develop
```

## Move a task through its stages

```sh
nmx stage approve T-0007                                   # approve the stage that waits
nmx stage back T-0007 --to spec --reason "Add the error case"
nmx stage skip T-0007
nmx stage start T-0007
```

When two stages run side by side, add `--stage <id>`, for example `--stage test-spec`. Turn on the choices of a task before its stage starts:

```sh
nmx stage workflow T-0007 on --stage implement   # Claude Code splits the work into sub-agents
nmx stage ci T-0007 on --stage implement         # wait for CI before the stage passes
```

See [Stages](/docs/concepts/stages/).

## What agents run

Natamux's stage prompts already tell agents what to run. The commands are useful for any agent, also one Natamux does not know:

| Command | When | Effect |
|---|---|---|
| `natamux done` | The stage's work is finished | The daemon runs the checks of the stage, then moves on or waits for your approval |
| `natamux ask "Keep the old columns?"` | The agent needs a decision | The task goes to **Needs input** and the question shows in the sidebar |
| `natamux stage back $NATAMUX_TASK_ID --reason "..."` | A review found problems | The task goes back to the last agent stage, with the reason in its prompt. See [The review loop](/docs/concepts/review-loop/) |
| `natamux task create "Follow-up: ..."` | The agent finds work for later | A draft task in the same project |

The [Claude Code plugin](https://github.com/lenhatthanh20/natamux/tree/main/plugins/natamux) teaches Claude Code these commands. Install it in Claude Code with `/plugin marketplace add lenhatthanh20/natamux`, then `/plugin install natamux@natamux`.

## Scripts and JSON

Add `--json` to get JSON, for example to feed another tool:

```sh
nmx task list --json
nmx events T-0007 --json
```

Exit code 0 means success, 1 a failed request, and 2 a wrong command line or a missing token. For anything the CLI does not cover, use the [HTTP API](/docs/reference/http-api/).

## Related

- [CLI reference](/docs/reference/cli/): every command and option
- [Tasks](/docs/concepts/tasks/): fields, states and dependencies
- [Other agents](/docs/agents/other-agents/): report from an agent with no hooks
