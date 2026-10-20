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

Yes, on Windows 10 (version 1809 or later) and Windows 11, x64. Download `Natamux-win-x64.exe`; task terminals run PowerShell. The installer is not signed yet, so SmartScreen asks first: click **More info**, then **Run anyway**. There are also builds for macOS (Apple silicon and Intel) and Linux. See [Install Natamux](/docs/getting-started/install/).

## Which agent reviews the work?

The **Review** stage uses the task's agent unless you choose another one in **Project settings → Stages → Review → Agent**. There is no global setting; each project keeps it in its `.natamux.json`. See [Choose the review agent](/docs/concepts/review-loop/#choose-the-review-agent).

## What does it cost?

Natamux is open source under the MIT license and free. The agents you run have their own prices and plans.

## Do I need to log in to Natamux?

No. You log in to your agents (and `gh`) as you do today. The dashboard uses a local token that the desktop app handles for you.

## Can two agents work on one task?

Yes. A task has one worktree, and its agent runs in Terminal 1. In the default pipeline, **Test spec** starts a second agent in Terminal 2 while Implement runs, and **Review** starts a third one in Terminal 5 that can send the work back. Each of them can be another agent or model. See [The review loop](/docs/concepts/review-loop/).

To compare several approaches to the same job, use **fan out**: it copies a task up to five times, each copy with its own worktree, so you can keep the best try. See [Fan out](/docs/guides/fan-out/).

You can also open more terminals in a task for your own shells.

## How is this different from tmux, or from one agent alone?

tmux gives you panes. Natamux adds what sits around them: a git worktree and branch per task, the state of every agent on one dashboard, and a workflow that each task follows. One agent alone does one job at a time, in one checkout, and you are the process. With Natamux the process is a file: stages, approvals, reviews and checks. See [What is Natamux](/docs/).

## Can I share a workflow with my team?

Yes. The workflow is the `stages` list in `.natamux.json` at the root of the repository. Commit it, and everyone who opens the project runs the same stages. You can also move a workflow to another project with **Import…** and **Export** in **Project settings**. See [Build a workflow](/docs/guides/workflows/) and the [configuration reference](/docs/reference/configuration/).

## Does it work with monorepos and any language?

Yes. Natamux does not depend on a language. It works with git, and every check is a shell command you write: the `check` and `command` of a stage, and the condition of a branch path. A `.natamux-setup` script prepares each new worktree, for example with an install. Each task also gets a block of ten ports in `$PORT`, so dev servers do not collide. See [Workspaces](/docs/concepts/workspaces/) and [Stages](/docs/concepts/stages/).

## Which agent should I use for which stage?

There is no fixed answer. A common split is the agent you trust most for Implement, and a different one for Review, so the review has fresh eyes. Each stage sets its `agent` and `args` in `.natamux.json`. See [Mix agents in one task](/docs/guides/mix-agents/) and the [compatibility table](/docs/agents/).

## What does the note under the recordings mean?

The recordings on the homepage and in these docs show the real Natamux dashboard. The agents in them are stand-in scripts, so their terminal text is short and the runs are fast. The dashboard, the stages, the branches and the send backs are real. A real agent writes much more.

## Where are my tasks stored?

As Markdown files with a small header, in `<data folder>/tasks/T-0001.md`. A SQLite index is a cache that Natamux rebuilds from them.

| Setup | Data folder |
|---|---|
| Desktop app, macOS | `~/Library/Application Support/Natamux/data` |
| Desktop app, Windows | `%APPDATA%\Natamux\data` |
| Desktop app, Linux | `~/.config/Natamux/data` |
| A daemon started in a repository | `.natamux/` in that folder, unless you set `NATAMUX_DATA_DIR` |
| Docker | The volume `natamux-data`, at `/data` |

Worktrees are in `worktrees/` inside the data folder, unless you set `NATAMUX_WORKTREES_DIR`.

## Does it change my main checkout?

No. Each task works in its own git worktree and branch. Natamux never pushes, merges or deletes a branch unless you click the button or run the command.

## What happens when I quit the app?

Quitting stops the daemon and every agent and terminal. The app asks first when agents are working. Tasks, worktrees and changes stay, and the agents of running, waiting and in-review tasks start again when you open the app. The other terminals of a task come back when you open the task, and each agent in them goes on with its last conversation. See [A terminal after a daemon restart](/docs/guides/terminal/#a-terminal-after-a-daemon-restart). On macOS, closing the window keeps everything running. See [Desktop app](/docs/guides/desktop-app/).

## Can I use it in a browser instead of the desktop app?

Yes. The dashboard is a web page served by the daemon. Open it and paste the daemon token. See [Install Natamux](/docs/getting-started/install/) and [Self-host with Docker](/docs/guides/self-host-docker/).

## Does it update itself?

Yes, from version 0.1.2. The desktop app checks GitHub for a new release, downloads it in the background and installs it when you restart. Version 0.1.1 and older cannot update, so install 0.1.2 by hand once. See [Updates](/docs/guides/desktop-app/#updates).

## How do I uninstall it?

1. Quit the app.
2. macOS: drag Natamux from Applications to the Trash. Windows: **Settings → Apps → Installed apps → Natamux → Uninstall** (Windows 10: **Apps & features**). Linux: remove the AppImage or tar.gz folder, or remove the `.deb` with your package manager.
3. Optional: delete the user data folder (`~/Library/Application Support/Natamux` on macOS, `%APPDATA%\Natamux` on Windows, `~/.config/Natamux` on Linux). This removes your tasks, the token and the logs.
4. Optional: remove worktrees you no longer need. In each repository, `git worktree list` shows them and `git worktree remove <path>` removes one.
5. Optional: remove the `PATH` line you added for the `natamux` command.
