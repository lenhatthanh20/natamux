---
title: FAQ
description: Short answers to common questions about Natamux.
sidebar:
  order: 2
---

## Does Natamux send my code anywhere?

No. Natamux has no account, no server of its own and no telemetry. Your code leaves your machine only when something you start makes it happen: the coding agents send prompts and code to their own companies, and git and `gh` talk to GitHub when you push or open a pull request. See [Security and privacy](/docs/help/security-and-privacy/).

## Which agents does it support?

Claude Code, Codex, Gemini CLI, GitHub Copilot CLI and Cursor. Natamux starts each one in the task's terminal and adds hooks, so the dashboard shows when it works, asks or finishes. Other terminal agents can report with `natamux ask` and `natamux done`. See [Agents](/docs/agents/other-agents/).

## Does it run on Windows?

Not yet. The current release has macOS (Apple silicon and Intel) and Linux builds only.

## What does it cost?

Natamux is open source under the MIT license and free. The agents you run have their own prices and plans.

## Do I need to log in to Natamux?

No. You log in to your agents (and `gh`) as you do today. The dashboard uses a local token that the desktop app handles for you.

## Can two agents work on one task?

A task has one worktree, and its agent runs in the task's first terminal. Some stages run side by side (Test spec and Implement), each with its own agent. To compare several approaches to the same job, use **fan out**: it copies a task up to five times, each copy with its own worktree, so you can keep the best try. See [Fan out](/docs/guides/fan-out/).

You can also open more terminals in a task for your own shells.

## Where are my tasks stored?

As Markdown files with a small header, in `<data folder>/tasks/T-0001.md`. A SQLite index is a cache that Natamux rebuilds from them.

| Setup | Data folder |
|---|---|
| Desktop app, macOS | `~/Library/Application Support/Natamux/data` |
| Desktop app, Linux | `~/.config/Natamux/data` |
| A daemon started in a repository | `.natamux/` in that folder, unless you set `NATAMUX_DATA_DIR` |
| Docker | The volume `natamux-data`, at `/data` |

Worktrees are in `worktrees/` inside the data folder, unless you set `NATAMUX_WORKTREES_DIR`.

## Does it change my main checkout?

No. Each task works in its own git worktree and branch. Natamux never pushes, merges or deletes a branch unless you click the button or run the command.

## What happens when I quit the app?

Quitting stops the daemon and every agent and shell. The app asks first when agents are working. Tasks, worktrees and changes stay. On macOS, closing the window keeps everything running. See [Desktop app](/docs/guides/desktop-app/).

## Can I use it in a browser instead of the desktop app?

Yes. The dashboard is a web page served by the daemon. Open it and paste the daemon token. See [Install Natamux](/docs/getting-started/install/) and [Self-host with Docker](/docs/guides/self-host-docker/).

## Does it update itself?

Not yet. Watch the [releases page](https://github.com/lenhatthanh20/natamux/releases/latest).

## How do I uninstall it?

1. Quit the app.
2. macOS: drag Natamux from Applications to the Trash. Linux: remove the AppImage or tar.gz folder, or remove the `.deb` with your package manager.
3. Optional: delete the user data folder (`~/Library/Application Support/Natamux` on macOS, `~/.config/Natamux` on Linux). This removes your tasks, the token and the logs.
4. Optional: remove worktrees you no longer need. In each repository, `git worktree list` shows them and `git worktree remove <path>` removes one.
5. Optional: remove the `PATH` line you added for the `natamux` command.
