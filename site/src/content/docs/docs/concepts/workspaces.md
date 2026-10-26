---
title: Workspaces
description: How Natamux gives each task its own git worktree, branch, ports and terminals.
sidebar:
  order: 2
---

A workspace is where a task's work happens. Natamux makes a git worktree and a branch for each task. Agents that run side by side never touch the same files.

A workspace has four parts:

- a **worktree**: a folder with its own checkout of your repository
- a **branch** for the task
- a block of **ports** for dev servers
- one or more **terminals**, where agents and your commands run

## Create a workspace

Use the dashboard (see [Dashboard](/docs/guides/dashboard/)) or the CLI:

```bash
natamux workspace create T-0001
natamux workspace create T-0001 --base release/1.2
```

Natamux refuses to create a workspace when:

- the task has one already
- the task is `done` or `cancelled`
- the base branch does not exist
- git refuses

### Branch and folder names

| Item | Name | Example |
|---|---|---|
| Branch | `natamux/` + task id in lower case + a slug of the title | `natamux/t-0001-fix-login-redirect` |
| Folder | The worktrees folder + the task id | `.natamux/worktrees/T-0001` |

The slug uses lower case letters, digits and `-`. It has at most 40 characters. A title with no usable characters gives the slug `task`.

### Base branch

A new branch starts from the base branch. The base is a local branch, or a branch of `origin` written as `origin/<branch>`. In the new workspace dialog, pick it from the branch list. The list starts on the default branch. **Remote branches** lists the branches of `origin` that have no local branch of the same name, also ones you never fetched.

A base `origin/<branch>` works without a local copy:

- Natamux fetches only that branch first, into `origin/<branch>`. It makes no local branch for it.
- Without a network, a copy fetched earlier is used.
- When `origin` has no such branch, the task is made, but its workspace fails with "There is no branch <branch> on origin". Pick another base and create the workspace again.

Without `--base`, the base is the default branch of the repository, the first of these that is a local branch:

1. The default branch of `origin` (`origin/HEAD`, as the clone set it)
2. `main`
3. `master`
4. The branch of your main checkout

With no branch at all, the new branch starts from `HEAD` of your main checkout.

The new branch starts from the base as its remote has it now:

- Natamux fetches the base from its remote first, such as `origin/main`, so a branch you did not pull does not start the task from old code. Your checkout and your local branches do not change.
- When the local base has commits that are not pushed, or the fetch fails (no network, no remote), the branch starts from the local base.
- The new branch follows no remote branch until it is pushed.
- The diff size of a workspace counts from the same point, so commits you did not pull do not count as the task's work.

If a task already has a branch from an earlier workspace, and the branch still exists, Natamux uses that branch again. The work of the task carries on.

### Worktrees folder

By default, worktrees go in `worktrees/` inside the data folder (`.natamux/worktrees`). To use another folder, set `NATAMUX_WORKTREES_DIR` when you start the daemon. There is one subfolder per task id. See [Environment variables](/docs/reference/environment-variables/).

A project can also have its own worktree folder. Set it in **Worktree folder** when you add the project; see [Repositories](/docs/guides/dashboard/#repositories). Workspaces made before keep their place.

## Setup script

A new worktree has only the files that git tracks. To install packages or prepare files, add a setup script to your repository.

| Platform | File | Command Natamux runs |
|---|---|---|
| macOS and Linux | `.natamux-setup` | `sh ./.natamux-setup` |
| Windows | `.natamux-setup.ps1` | `& ./.natamux-setup.ps1` |

```bash
#!/bin/sh
pnpm install
```

Rules:

- Commit the file at the root of the repository, so every new worktree has it.
- The script runs once, right after the workspace is created.
- It runs in the first terminal of the task, in the worktree. You can read its output there.
- If the script fails, the workspace stays. The error shows in the terminal.
- Without a script file, nothing runs.

## Ports

Each task gets its own block of 10 ports, so dev servers of tasks that run at the same time do not clash.

The block comes from the number at the end of the task id. Task `T-0001` gets ports 20010 to 20019. `T-0002` gets 20020 to 20029. The numbers wrap after 1000 tasks. A task with your own id, such as `ABC-1`, shares the block of `T-0001`.

Natamux sets these variables in every terminal of the task:

| Variable | Value |
|---|---|
| `PORT` | The first port of the block, such as `20010`. Many dev servers read it. |
| `NATAMUX_PORT` | The same value. |
| `NATAMUX_TASK_ID` | The task id, such as `T-0001`. |
| `NATAMUX_TERMINAL` | The name of the terminal, such as `1`. |

:::note
Inside a task terminal, `NATAMUX_PORT` is the port block of the task. It is not the port of the daemon. To pass the daemon port, use `NATAMUX_URL`.
:::

Use `PORT` in your scripts:

```bash
pnpm dev --port "$PORT"
```

## Terminals

Each task has its own terminals, started in the worktree. The first terminal is called `1`. Natamux starts the agent there.

- A terminal keeps running when you close the dashboard.
- Stages that run side by side use more terminals, one each. A testing stage with a command runs it in a terminal of its own.
- Natamux allows at most 30 terminals at once, across all tasks.
- When a task is `done` or `cancelled`, Natamux closes its terminals.

See [Terminal](/docs/guides/terminal/).

## Stage files

Agents read and write stage files, such as `spec.md`, in a `.natamux-task/` folder in the worktree. Natamux adds this folder to the ignore list of your repository, so the files never reach a commit. Natamux copies them to its data folder when a stage ends and puts them back in a new worktree. See [Stages](/docs/concepts/stages/).

## Remove a workspace

Removing a workspace deletes the worktree folder. The task stays.

```bash
natamux workspace remove T-0001 --yes
natamux workspace remove T-0001 --yes --delete-branch
```

You can also remove it in the dashboard.

Rules:

- `--yes` is required. It confirms that the folder is deleted.
- Natamux refuses when the worktree has changes that are not committed. Commit or discard them first.
- Natamux closes the terminals of the task, then removes the worktree.
- With `--delete-branch`, Natamux deletes the branch only when it is fully merged. Otherwise the branch stays, and the task keeps its branch name. The command output says whether the branch was deleted.

:::caution
Removing a workspace also deletes the files that git ignores, such as `node_modules`. Commits stay in the branch.
:::

After you remove a workspace, you can create a new one. If the branch stayed, the new workspace continues on it.
