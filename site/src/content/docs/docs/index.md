---
title: What is Natamux
description: Natamux runs many coding agents in parallel and shows every task's state on one dashboard.
sidebar:
  order: 1
---

Natamux is an open-source Agent Development Environment (ADE). You give each piece of work its own task, and Natamux gives each task its own git worktree and terminal. Coding agents run in those terminals in parallel, and one dashboard shows what every task is doing.

![The Natamux dashboard: a table of tasks, a "Needs you" strip at the top and the workspace list in the sidebar](/natamux/screens/dashboard.png)

## The problem

Coding agents are good at one job at a time. When you run several, you start to juggle:

- Terminal tabs, and no way to tell which agent is working, which is stuck and which finished an hour ago.
- Branches that collide, because two agents edit the same checkout.
- Permission prompts and questions that wait unseen.
- Different agents with different tools and no common view.

## What Natamux does

- **Tasks.** Each piece of work is a task with notes, a priority and a state: draft, ready, running, needs input, review or done.
- **A worktree and a terminal per task.** A task gets its own branch and folder, so agents never step on each other. The shell keeps running when you close the dashboard.
- **Stages.** A task moves through stages: Context, Spec, Plan, Test spec and Implement side by side, Review, Testing, Release. Each stage checks its result, and you approve where a stage asks for it. You can change the stages per repository. See [Stages](/natamux/docs/concepts/stages/).
- **Needs you.** A strip at the top of the dashboard lists the tasks that wait for you: an agent asked a question, a stage is ready for review, a task is blocked or failed.
- **Five agents.** Claude Code, Codex, Gemini CLI, GitHub Copilot CLI and Cursor. Natamux adds hooks to each session, so the dashboard knows when an agent works, asks or finishes. See [Agents and hooks](/natamux/docs/concepts/agents-and-hooks/).
- **Changes and history.** A file tree with side-by-side or inline diffs, commit, push, open a pull request, and a branch graph.
- **A command line.** `natamux`, short name `nmx`, for you and for agents (`natamux done`, `natamux ask`). See the [CLI reference](/natamux/docs/reference/cli/).
- **Browser or desktop app.** Use the desktop app, or run the dashboard in a browser tab.
- **No account and no telemetry.** Natamux runs on your machine and sends nothing anywhere. See [Security and privacy](/natamux/docs/help/security-and-privacy/).

## How the parts fit

```text
 Desktop app or browser tab        natamux / nmx (CLI)
           |                              |
           |  HTTP + WebSocket            |  HTTP
           v                              v
        +-------------- daemon ---------------+
        | tasks, stages, git worktrees,       |
        | terminals, GitHub                   |
        +-----------------+-------------------+
                          |  one terminal per task
                          v
              agent (claude, codex, ...)
                          |
                          |  hooks: "working", "asks", "done"
                          +--------> back to the daemon
```

- The **daemon** is a background process on your machine. It owns all task state. It listens on `127.0.0.1` only and needs a token on every request.
- The **dashboard** is the web page the daemon serves. The desktop app is a window around it and starts the daemon for you.
- The **CLI** talks to the daemon over HTTP. Agents call it from their terminals.
- **Agents** are the command-line tools you already have installed. Hooks tell the daemon what each agent does.

Tasks are plain Markdown files with a small header, so they are easy to read and back up. A SQLite index is only a cache of them.

## Where to go next

1. [Install Natamux](/natamux/docs/getting-started/install/): download the app or run it from source.
2. [Quick start](/natamux/docs/getting-started/quick-start/): from first launch to a merged change.
3. [Tasks](/natamux/docs/concepts/tasks/) and [Workspaces](/natamux/docs/concepts/workspaces/): the core ideas.
4. [Desktop app](/natamux/docs/guides/desktop-app/) or [self-host with Docker](/natamux/docs/guides/self-host-docker/).
5. [Troubleshooting](/natamux/docs/help/troubleshooting/) and the [FAQ](/natamux/docs/help/faq/).
