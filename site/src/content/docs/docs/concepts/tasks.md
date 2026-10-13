---
title: Tasks
description: What a task is in Natamux, the states it moves through, and where Natamux stores it.
sidebar:
  order: 1
---

A task is one unit of work for one agent. It has a title, notes, a state, and a place in a pipeline of [stages](/docs/concepts/stages/). Most tasks also have a [workspace](/docs/concepts/workspaces/): a git worktree and branch of their own.

Task ids look like `T-0001`. The number grows by one for each new task.

## Task fields

| Field | Meaning |
|---|---|
| `title` | Short name, 1 to 200 characters. |
| `description` | Notes, in Markdown. They become the first prompt and the task context. |
| `state` | Where the task is now. See [States](#states). |
| `priority` | `low`, `normal` (default), `high` or `urgent`. |
| `labels` | Free text tags. |
| `agent` | The agent that works on the task, such as `claude-code`. Empty until one is chosen. |
| `branch`, `workspace` | The git branch and worktree folder of the task. Empty until a workspace exists. |
| `pr`, `prStatus` | The pull request of the task, and the state of the pull request and its checks. |
| `dependsOn` | Ids of tasks that must be done first. |
| `parentId` | The task this one is a part of. |
| `repo` | The repository the task belongs to. Empty means the repository with the id `default`. |
| `stage`, `stages`, `lanes` | The current stage, the stages of the task, and the state of each stage that runs side by side. |
| `context`, `issue` | Where the task context comes from, and the issue or ticket that tracks the task. |
| `createdAt`, `updatedAt` | Timestamps. |

## States

A task is always in one of nine states. When the task has stages, the state describes the current stage. A task that moves from Spec to Plan goes back to `ready` or `review` at the start of the new stage.

| State | Meaning | How a task gets there |
|---|---|---|
| `draft` | Created, not started. | You create a task. |
| `ready` | The stage is waiting to start. | You mark a draft ready. A stage starts and waits for its agent, a workspace, or the tasks it depends on. You unblock or retry a task. |
| `running` | An agent or command works on the stage. | The stage starts. You type a prompt to the agent. A task that waited for you gets going again. |
| `needs-input` | The agent waits for you. | The agent asks a question or a permission. The agent ends its turn without running `natamux done`. The checks of the stage failed on the last try. A terminal bell rings from an agent without hooks. |
| `review` | The stage waits for your approval. | A stage with a manual gate passed its checks. A review or manual stage starts. |
| `blocked` | You paused the task. | You block a `ready` or `running` task. |
| `failed` | The stage failed. | The agent reports a failure. The agent program is not found. A stage command exits with an error. |
| `done` | The task is finished. | The last stage finishes, or the pull request is merged. |
| `cancelled` | You stopped the task. | You cancel it from any state except `done` and `cancelled`. |

`done` and `cancelled` are final. A finished task cannot move again. When a task ends, Natamux closes its terminals.

### How states change

```text
draft ──► ready ──► running ──► review ──► done
            ▲  │      │  ▲        │
            │  │      │  │        └──► (next stage, or running again
            │  │      ▼  │             when you send it back)
            │  │   needs-input
            │  │
            │  └──► blocked ──► ready
            │
          failed ◄── running          (retry: failed ──► ready)

Any state that is not final ──► cancelled
```

Common moves:

- `draft` to `ready` to `running`: you start a stage, or the first prompt reaches the agent.
- `running` to `needs-input`: the agent needs you.
- `needs-input` to `running`: you answer, or the agent continues by itself.
- `running` to `review`: the agent runs `natamux done`, the checks pass, and the gate is manual.
- `review` to the next stage: you approve.
- `review` to `running`: you send the task back to an earlier stage.
- `failed` to `ready`: you retry.
- `blocked` to `ready`: you unblock.

With an automatic gate, a stage that passes its checks goes straight to the next stage. See [Stages](/docs/concepts/stages/).

:::note
Natamux ignores an event that does not fit the current state. For example, a hook that reports a finished turn does nothing for a task that is `blocked`.
:::

## Priorities

Every task has a priority: `low`, `normal`, `high` or `urgent`. The default is `normal`. Set it when you create a task or change it later.

```bash
natamux task create "Fix login redirect" --priority high
natamux task edit T-0001 --priority urgent
```

## Labels

Labels are free text tags. Repeat `--label` for more than one.

```bash
natamux task create "Fix login redirect" --label bug --label auth
natamux task edit T-0001 --label bug
```

`task edit --label` replaces the whole list. Search on the dashboard also matches labels. [Fan-out](/docs/guides/fan-out/) adds a label such as `tries-t-0001` to the original task and its copies.

## Dependencies

A task can depend on other tasks. It waits until every task it depends on is `done`.

```bash
natamux task create "Add profile page" --depends-on T-0004 --depends-on T-0005
natamux task edit T-0006 --depends-on T-0004
```

How waiting works:

- The stage of a waiting task stays `ready`. Its agent does not start.
- When the last task it depends on is done, Natamux starts the stage.
- Starting an agent by hand for a waiting task fails with a message such as `T-0006 waits for T-0004 to be done`.
- A dependency on a task that does not exist is rejected. A task that no longer exists is not waited for.
- A dependency that would make a loop is rejected. A task cannot wait for itself.

## Subtasks

A subtask is a task with a `parentId`. It shows the chip `part of T-0001`. Subtasks go one level deep: a subtask cannot have subtasks.

Add a subtask in the task details on the dashboard. A subtask is a normal task, with its own state, workspace and stages.

## The Needs you list

A task needs you when it waits for a person:

| Reason | Which tasks |
|---|---|
| Asks | State `needs-input`. |
| Review | State `review`. |
| Failed | State `failed`. |
| Blocked | State `blocked`. |
| Quiet | State `running`, with no sign of life for 10 minutes. The agent may be stuck, interrupted or closed. A task whose agent waits for background work is never quiet. |

The dashboard counts these tasks in the title bar ("2 need you"). Click the count to list them. The command palette has a **Needs you** scope with the same list. See [Needs you](/docs/guides/dashboard/#needs-you).

## Where Natamux stores tasks

Natamux keeps its files in the data folder. The default is `.natamux/` in the folder where the daemon starts. Set `NATAMUX_DATA_DIR` to change it. See [Environment variables](/docs/reference/environment-variables/).

| Path | What it holds |
|---|---|
| `tasks/T-0001.md` | The task. Markdown with YAML front matter. This is the source of truth. |
| `tasks/T-0001.events.jsonl` | The event history of the task, one JSON line per event. |
| `tasks/T-0001/` | A copy of the stage files, saved when a stage ends. |
| `index.sqlite` | A cache of the task files and events. Natamux rebuilds it from the files. |

The front matter holds the fields. The body of the file is the description:

```markdown
---
id: T-0001
title: Fix login redirect
state: running
priority: high
labels:
  - bug
agent: claude-code
branch: natamux/t-0001-fix-login-redirect
workspace: /path/to/.natamux/worktrees/T-0001
stage: implement
---

Users land on the home page after login. Send them back to the page they came from.
```

The example is shortened. A real file also holds the stage fields, timestamps and other fields from the table above.

### Event history

Natamux records everything that happens to a task as an event: who sent it, the type, the time, and extra data. The event log is how Natamux knows the state, and how the progress view shows what happened and why.

Each event has a source: `agent`, `human`, `git`, `ci` or `system`. Examples of types are `task.readied`, `agent.asked`, `stage.checked`, `stage.approved`, `git.committed` and `pr.opened`.

```bash
natamux events T-0001
natamux events T-0001 --json
```

Some events only record a fact and change no state, such as `git.committed` and `workspace.created`.

You can send an event yourself. This is useful for tools that Natamux does not know.

```bash
natamux event T-0001 task.blocked --payload '{"reason":"waiting for design"}'
```

See the [CLI reference](/docs/reference/cli/) for all commands.
