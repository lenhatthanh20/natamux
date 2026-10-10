---
title: Stages
description: How a task moves through a pipeline of stages, and how Natamux checks and gates each one.
sidebar:
  order: 3
---

A stage is one step of the work on a task, such as writing a spec or implementing code. Every task goes through a **pipeline** of stages. For each stage, Natamux starts the work, checks the result, and waits for your approval when the stage asks for it.

![The stage progress view of a task](/screens/progress.png)

## The default pipeline

A project without a `.natamux.json` file uses this pipeline:

```text
Context → Spec → Plan → [ Test spec ‖ Implement ] → Review → Testing → Release
```

| Stage | Kind | Gate | What happens |
|---|---|---|---|
| Context | `context` | manual | Natamux collects what the task is about into `context.md`. |
| Spec | `agent` | manual | The agent writes `spec.md`. It does not change code. |
| Plan | `agent` | manual | The agent writes `plan.md`. It does not change code. |
| Test spec and Implement | group | | Two stages run side by side, see below. |
| Review | `review` | manual | You read the changes and approve them, or send the task back. |
| Testing | `testing` | auto | The agent adds missing tests, runs all tests and fixes failures. It does not commit. |
| Release | `release` | manual | The task finishes when its pull request is merged. |

The group in the middle holds two stages:

| Stage | Kind | Gate | What happens |
|---|---|---|---|
| Test spec | `agent` | manual | An agent writes `test-spec.md` with a **Cases** section. It does not read the code. |
| Implement | `agent` | auto | The agent writes tests and code. It does not commit: you read the changes in the **Changes** tab and commit them. |

To change the pipeline, see the [configuration reference](/docs/reference/configuration/).

A task keeps the list of stage ids it had when it was created. A later change to `.natamux.json` changes the settings of those stages (name, gate, checks), not the list of stages of existing tasks.

## Stage kinds

| Kind | Who works | When the stage finishes |
|---|---|---|
| `context` | Natamux, or an agent for a skill or MCP source | The context file is written. |
| `agent` | An agent in the task shell | The agent runs `natamux done` and the checks pass. |
| `review` | You | You approve. |
| `testing` | An agent, or a command if the stage has one | With a command: the command exits with 0. Without: the agent runs `natamux done` and the checks pass. |
| `release` | Your git host | The pull request is merged. |
| `manual` | You | You approve. Use it for a step that Natamux cannot do, such as a sign-off. |

A `review` or `manual` stage waits for you at once (state `review`). Other stages start in state `ready` and start working.

## Gates

The gate decides what happens after a stage passes its checks.

| Gate | Behavior |
|---|---|
| `auto` | The task goes on to the next stage. |
| `manual` | The task waits in state `review` until you approve it. |

## Checks

When an agent runs `natamux done` (or you press **Mark as done**), Natamux runs the checks of the stage. A stage can have these checks:

| Check | Setting | Passes when |
|---|---|---|
| Output file | `output` | The file exists in `.natamux-task/`, is not empty, and was changed after the stage started. |
| Sections | `sections` | The file has a heading for each name. Case does not matter, and any heading level counts. |
| Cases | `sections` includes `Cases` | The Cases section has at least one case: a sub-heading, or a list item at the left edge when there are no sub-headings. |
| New commit | `requireCommit` | There is at least one commit since the stage started. |
| Clean worktree | `requireClean` | No file is left uncommitted. |
| Command | `check` | The shell command exits with 0. It runs in the worktree and gets 10 minutes. |

Natamux runs the output and git checks first. It runs the `check` command only when those pass. `requireCommit` and `requireClean` fail for a task that has no workspace.

The default pipeline has no commit check, so agents leave their changes uncommitted. Set `requireCommit` on a stage to make its agent commit: the default prompt then asks for a commit instead.

Stage files are Markdown files in the `.natamux-task/` folder of the worktree. They are named like `spec.md` and can be up to 1 MB. Natamux keeps git from tracking them. See [Workspaces](/docs/concepts/workspaces/).

The progress view lists the checks that passed under **Show evidence**, and the reasons for those that failed.

### What happens when a check fails

1. The first time, the stage stays `running`. Natamux types the reasons into the agent terminal and asks the agent to fix them and run `natamux done` again.
2. If the checks fail again, the task goes to `needs-input` and waits for you.
3. Then you can fix the problem and press **Mark as done** to check again, send the task back, or press **Approve anyway**.

A stage that no agent works on, such as a context stage, waits for you at the first failure.

If a stage cannot start, for example when the issue for a GitHub context cannot be read, the task goes to `needs-input` and the reason is in the progress view.

## What `natamux done` does

An agent runs `natamux done` in the task shell when its stage is finished. The command:

- reports that the stage of this shell is done
- makes the daemon run the checks of the stage
- uses `NATAMUX_TASK_ID` and `NATAMUX_TERMINAL`, which Natamux sets in task shells

```bash
natamux done
natamux done --task T-0001 --stage implement
```

Use `--stage` only when two stages run side by side and Natamux cannot tell which one you mean. The default stage prompts end with the line `When you are done, run natamux done.`

The turn end that follows `natamux done` does not count as "stopped early".

## Control a stage by hand

| Action | Dashboard | CLI |
|---|---|---|
| Approve | **Approve**, or **Approve anyway** when the checks failed | `natamux stage approve T-0001` |
| Send back | **Send back** | `natamux stage back T-0001 --to spec --reason "..."` |
| Skip | **Skip** | `natamux stage skip T-0001` |
| Start | **Start** | `natamux stage start T-0001` |
| Mark as done | **Mark as done** | `natamux done --task T-0001` |
| Pass or fail | | `natamux stage pass T-0001`, `natamux stage fail T-0001` |

The buttons are in the stage card in the **Progress** view of the details. While a stage waits for review, the workspace also shows a **Ready for review** banner under its header with **Approve** and **Send back**, so you can approve while you read the diff, also with the details closed or on a phone.

Add `--stage <id>` when two stages run side by side.

### Send a stage back

Sending back starts a stage again. You can go back to the current stage or to an earlier one.

- `--to <id>` names the stage. Without it, Natamux goes back to the last agent stage before the current one.
- `--reason` is added to the prompt of the agent as `What to change: ...`.
- Requesting changes in a review, or typing a prompt to the agent during a review stage, also sends the task back.
- After a send back to an earlier stage, the task goes through the stages after it again.

### Skip a stage

Skipping ends a stage without checks and moves the task on. If you skip the last stage, the task is `done`.

### Pass and fail

`natamux stage pass` and `natamux stage fail` set the result of a stage by hand. Natamux uses them itself for a testing stage with a command: when the command ends, the shell reports the result.

## Stages that run side by side

A group holds 2 to 4 stages that work at the same time. Only `agent` and `manual` stages can be in a group.

- The group finishes when every stage in it is done.
- Each stage works in its own shell. The stage that changes code (an agent stage without an output file) keeps the first shell.
- The state of the group is the most urgent state of its stages: `needs-input`, `failed`, `blocked`, `review`, `running`, `ready`, `draft`, in that order.

## Testing with a command

A `testing` stage with a `command` runs it in a shell. Without a command, an agent does the testing.

```json
{ "id": "testing", "kind": "testing", "gate": "auto", "command": "pnpm test" }
```

If the command exits with 0, the stage passes. Any other exit code fails the stage and the task is `failed`. To run it again, retry the task, then start the stage:

```bash
natamux event T-0001 task.retried
natamux stage start T-0001
```

## Context sources

The first stage of the default pipeline reads the context of the task. The source decides where the text comes from. Pick it when you create the task, for example `natamux task create "Fix login" --context github-issue:42`.

| Source | What Natamux does |
|---|---|
| `manual` | Writes the title, notes and reference of the task into `context.md`. The stage passes by itself, so a new task does not wait for you. |
| `github-issue` | Reads the issue with the `gh` CLI (title, body, labels, comments) into `context.md`, and links the issue to the task. Needs a reference such as `#42`, `owner/name#42` or an issue address. |
| `skill` | An agent runs a skill you configured and writes `context.md`. |
| `mcp` | An agent runs a prompt you configured and writes `context.md`. |

For `github-issue`, `gh` must be installed and logged in. See [GitHub](/docs/guides/github/). Configure the sources in `context.sources`; see the [configuration reference](/docs/reference/configuration/).

Natamux starts the context stage as soon as you create the task. The other stages start when the task reaches them, or when you start them.

## Related

- [Tasks](/docs/concepts/tasks/): the states a stage can be in
- [Agents and hooks](/docs/concepts/agents-and-hooks/): how agents start and report
- [Configuration](/docs/reference/configuration/): the `.natamux.json` file
