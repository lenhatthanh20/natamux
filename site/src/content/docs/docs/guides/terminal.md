---
title: Terminal
description: Work in the shells of a task, type to the agent, and use the ports and variables each shell gets.
sidebar:
  order: 2
---

Every task with a workspace has a **Terminal** tab. It holds real shells that run in the task's worktree folder. An agent runs in one of them.

![The terminal tab with the agent in Shell 1](/screens/terminal.png)

## Open the terminal

1. Select a task that has a workspace.
2. Double-click it, or press `Enter`. The workspace opens on the terminal while an agent works.
3. From another tab of the workspace, click **Terminal** or press `1`.

A task without a workspace shows **Start agent** and a way to make a workspace instead. See [Workspaces](/docs/concepts/workspaces/).

## Shells

The tab shows each shell in its own pane. Shell 1 is always there. The agent runs in Shell 1 and its pane carries an **Agent** label and the task state, for example `Running · Edit a.ts`.

| To do this | Do this |
|---|---|
| Add a shell | Click **New shell**, or press ``Ctrl+Shift+` `` |
| Maximize a pane | Click **Maximize Shell N**, or press `Mod+Shift+Enter` in the pane |
| Restore a pane | Click **Restore Shell N**, or press `Mod+Shift+Enter` again |
| Close a shell | Click **Close shell (ends the process)** in the pane header |
| Find text | Press `Mod+F`. On Windows and Linux, press `Ctrl+Shift+F` in a terminal |

`Mod` is ⌘ on macOS and Ctrl elsewhere. ``Ctrl+Shift+` `` is Ctrl on every platform.

Things to know:

- A task has at most 6 shells. **New shell** is off after that.
- Shell 1 has no close button. It stays for the agent.
- New shells are named in order: Shell 2, Shell 3, and so on.
- Panes sit side by side and fill the width. The last pane takes the rest of its row.
- On a phone, one shell fills the screen. Chips named **Shell 1**, **Shell 2** switch between them.
- Closing a shell ends its process and everything that runs in it.
- A shell keeps running when you close the dashboard. Open it later and the pane shows the last screen again. Natamux keeps up to 200,000 characters of output for this.
- Natamux runs at most 20 shells in all.

To jump to an extra shell of any workspace, press `Mod+J` outside a terminal, or use **Search or run a command**. The palette lists the extra shells under their workspace.

### Find in the terminal

`Mod+F` (`Ctrl+Shift+F` on Windows and Linux) opens **Find**. Press `Enter` for the next match and `Shift+Enter` for the previous one. **Match case** and **Use regular expression** are toggles. `Esc` closes the bar.

When you scroll up, **Jump to latest** brings you back to the end. Links to web addresses open in a new tab. Each pane keeps 5,000 lines of scrollback.

### Keys that belong to the shell

Inside a terminal, the shell gets your keys. On macOS the ⌘ shortcuts of Natamux still work. On Windows and Linux only the function keys and Ctrl+Shift shortcuts reach Natamux. `Ctrl+K`, `Ctrl+B` and `Ctrl+J` stay with the shell. See [Keyboard shortcuts](/docs/reference/shortcuts/).

## The agent in Shell 1

**Start agent** types the agent's command into Shell 1. You see it start like any program. See [Agents and hooks](/docs/concepts/agents-and-hooks/).

### Type to the agent

Click in the pane and type. Your keys go straight to the agent. This is how you answer a question.

When an agent asks something, the task goes to Needs input and a banner shows what it asks. The pane gets a colored bar. Click **Answer in terminal** to put the cursor in Shell 1. If you answered another way, click **Resume**.

### Send a message to the agent

You can send text to the agent without typing in the terminal. In the **Changes** tab, write a comment under the diff and click **Send to agent**. Natamux pastes it into Shell 1 as the agent's next prompt and presses Enter. See [Changes and history](/docs/guides/changes-and-history/).

This works only while an agent runs in Shell 1. Otherwise Natamux answers `No agent runs in the terminal of T-0001. Start the agent first`. That check keeps a comment from running as shell commands.

### Needs input or failed

A failed task shows a banner with the reason. Use **Start agent** or **Restart agent** to run the agent again. For the task states, see [Tasks](/docs/concepts/tasks/).

## A shell after a daemon restart

Shells belong to the daemon. When the daemon stops, it ends every shell, and an agent in a shell ends with it.

After a restart the pane shows **The shell ended.** (or **The terminal could not start.**) and a **Start a new shell** link. Click it. Natamux opens a fresh shell in the same workspace folder. The old screen and the processes in it are gone. Your files and your git state are not.

To run the agent again in Shell 1, use **Start agent**.

## Ports

Each task has its own block of 10 ports so dev servers of different tasks do not clash. The block starts at 20000 + 10 × the task number, and the number wraps after 1000 tasks. `T-0001` gets 20010 to 20019. The details panel shows the range as **Dev ports**, with a note that the first port is `$PORT`.

## Environment variables

Every shell starts with your daemon's environment plus these variables.

| Variable | Value |
|---|---|
| `NATAMUX_TASK_ID` | The task id, such as `T-0001` |
| `NATAMUX_TERMINAL` | The shell name, such as `1` |
| `NATAMUX_URL` | The daemon address on loopback, such as `http://127.0.0.1:4317` |
| `NATAMUX_TOKEN` | The daemon token |
| `PORT` | The first port of the task's block |
| `NATAMUX_PORT` | The same value as `PORT` |
| `TERM` | `xterm-256color` |

:::caution
In a shell, `NATAMUX_PORT` is the task's first dev port, not the daemon's port. Use `NATAMUX_URL` to reach the daemon.
:::

`NATAMUX_URL` and `NATAMUX_TOKEN` let the `natamux` command and agent hooks work in the shell without setup. The shell starts in your login shell (`$SHELL`, else `/bin/bash`, else `/bin/sh`). On Windows it is PowerShell.

Because the shell gets the token, treat what you run in it as trusted. See [Security and privacy](/docs/help/security-and-privacy/).
