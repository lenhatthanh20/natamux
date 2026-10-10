---
title: Fan out
description: Run several tries of one task in parallel, compare the results and keep the best one.
sidebar:
  order: 5
---

Agents do not give the same answer twice. **Fan out** copies a task so that several agents try the same work at once, each in its own workspace. You compare the results and keep one.

## Run more tries

1. Select the task in the table, or open its workspace, so that its details show.
2. Open the **More actions** menu in the details panel and click **Run more tries...**.
3. In **Run [N] more tries**, pick how many, from 1 to 5.
4. Keep **Start Claude Code in each** ticked to start the agents now. Clear it to make the tries as drafts and start them later.
5. Click **Run tries**, or press `Mod+Enter`.

The menu item shows only for a task that is not finished and whose agent is not running or waiting. A task that works now would run beside its own agent.

A toast says **Made N tries.** The dashboard then filters the list with the label `tries-t-0007` (with the id of your task), so the original and its copies sit side by side.

## What Natamux makes

For a task `T-0007`, Natamux:

1. Adds the label `tries-t-0007` to `T-0007`. The details panel shows it as **Tries of T-0007**.
2. Makes N copies. Each copy is a new task titled `<title> (try 2)`, `(try 3)` and so on. A copy keeps the notes, priority, labels, dependencies and repository of the original, and gets the same `tries-…` label.
3. With **Start Claude Code in each**, makes a workspace for each copy, with its own worktree and branch, and starts Claude Code in it.

The tries do not meet: each has its own folder, branch and [dev ports](/docs/guides/terminal/#ports).

:::note
Fan out starts Claude Code only. The other agents are not offered here. To use another agent, clear the tick and start each try yourself with **Start agent**. See [Agents](/docs/concepts/agents-and-hooks/).
:::

:::caution
Try numbers always start at 2. Run fan out twice on the same task and you get two tasks titled `(try 2)`. They still have different ids. They also share the label, so all of them are listed as tries.
:::

## Compare the tries

Open the details of any task in the group. When the group has more than one task, a **Tries** section lists all of them:

| Column | Shows |
|---|---|
| Task id | Click it to open that try. Your current try is in bold. |
| State | The state badge |
| Size | Lines added and removed and the number of files, for example `+42 −7, 3 files` |
| Pull request | The pull request number, once one exists |

To read what a try did, open its workspace and use the **Changes** tab. See [Changes and history](/docs/guides/changes-and-history/). To watch all agents at once, use **Agents** in the sidebar.

## Keep one try

1. In the **Tries** section, click **Keep this try** next to the best result.
2. Confirm **Keep this try, cancel the others**.

Natamux cancels the other tries that are not finished and says **Cancelled N other tries.** Tries that are done or cancelled already are left alone. Their workspaces and branches are kept, so you can still read their code. Remove them from the sidebar when you do not need them.

The kept try goes on as a normal task. Commit, push and open the pull request from it. The button is not shown for a try that is done or cancelled. The original task is one of the tries too, so you can keep it.

## Limits

- At most 5 copies in one run, so up to 6 tries in a group with the original.
- Each try with a workspace is a worktree on your disk. A task can have at most 6 shells and Natamux runs at most 20 shells in all.
