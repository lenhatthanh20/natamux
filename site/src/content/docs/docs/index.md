---
title: What is Natamux
description: Natamux runs many coding agents in parallel, each task in its own worktree and terminal, and shows every task's state on one dashboard.
sidebar:
  order: 1
---

Natamux is an open-source Agent Development Environment (ADE). You give each piece of work its own task, and Natamux gives each task its own git worktree, branch and terminal. Coding agents such as Claude Code, Codex, Gemini CLI, GitHub Copilot CLI and Cursor run in those terminals in parallel, and one dashboard shows what every task is doing and which one needs you.

<figure class="shot">
<img class="shot-dark" src="/screens/ui/table-dark.webp" alt="The Natamux dashboard. The sidebar lists the workspaces of a project, with a tinted card for a task whose agent waits for an answer. The main area is the All tasks table with state filters, and the title bar says 5 need you." width="1600" height="1000" decoding="async" />
<img class="shot-light" src="/screens/ui/table-light.webp" alt="The Natamux dashboard. The sidebar lists the workspaces of a project, with a tinted card for a task whose agent waits for an answer. The main area is the All tasks table with state filters, and the title bar says 5 need you." width="1600" height="1000" decoding="async" />
<figcaption>All tasks in one table. The card in the sidebar is a task whose agent asked a question.</figcaption>
</figure>

<div class="downloads">
<a href="/docs/getting-started/install/"><strong>Install</strong><span>macOS · Windows · Linux</span></a>
<a href="/docs/getting-started/quick-start/"><strong>Quick start</strong><span>first task in ten minutes</span></a>
<a href="/docs/concepts/review-loop/"><strong>The review loop</strong><span>two agents build, a third reviews</span></a>
<a href="/docs/guides/workflows/"><strong>Build a workflow</strong><span>paths, send back, CI</span></a>
<a href="/docs/guides/mix-agents/"><strong>Mix agents</strong><span>an agent per stage</span></a>
</div>

## The problem

Coding agents are good at one job at a time. When you run several, you start to juggle:

- Terminal tabs, and no way to tell which agent is working, which is stuck and which finished an hour ago.
- Branches that collide, because two agents edit the same checkout.
- Permission prompts and questions that wait unseen.
- Work that nobody checked before it reaches a pull request.
- Different agents with different tools and no common view.
- On a team, each person runs agents their own way, so there is no shared process for spec, review, CI and QA.

## Agents that check each other

Every task goes through [stages](/docs/concepts/stages/). The default pipeline is:

```text
Context → Spec → Plan → [ Test spec ‖ Implement ] → Automated tests → Review → Release
```

You approve the spec, the plan and the test spec. After you approve the plan, two agents work at once: one writes test cases from the spec without reading the code, the other writes the code. Then the tests run, and a third agent reviews the changes in a session of its own. When the review finds problems, it sends the task back to Implement by itself, at most two times, and then asks you. See [The review loop](/docs/concepts/review-loop/).

<figure class="shot">
<img class="shot-dark" src="/screens/ui/task-side-by-side-dark.webp" alt="An open task with two agent terminals side by side. Implement runs in Shell 1 and Test spec waits in Shell 2. A banner says Ready for review: Test spec, and the Progress panel shows Context, Spec and Plan done, Test spec in review and Implement running." width="1600" height="1000" loading="lazy" decoding="async" />
<img class="shot-light" src="/screens/ui/task-side-by-side-light.webp" alt="An open task with two agent terminals side by side. Implement runs in Shell 1 and Test spec waits in Shell 2. A banner says Ready for review: Test spec, and the Progress panel shows Context, Spec and Plan done, Test spec in review and Implement running." width="1600" height="1000" loading="lazy" decoding="async" />
<figcaption>Test spec and Implement run side by side, each agent in its own shell.</figcaption>
</figure>

## What you get

### A worktree and a terminal per task

A task gets its own branch, such as `natamux/t-0001-fix-login`, and its own folder, so agents never step on each other. Its shells keep running when you close the dashboard, and come back with their screen when you open it again. See [Workspaces](/docs/concepts/workspaces/) and [Terminal](/docs/guides/terminal/).

### One place to see what needs you

The title bar counts the tasks that wait for you: an agent asked a question, a stage is ready for review, a task failed or is blocked, or an agent has been quiet for 10 minutes. In the sidebar, a question or a failure becomes a card with an **Answer** button. Turn on **Notify me** to get a system notification. See [Needs you](/docs/guides/dashboard/#needs-you).

<figure class="shot">
<img class="shot-dark" src="/screens/ui/task-needs-you-dark.webp" alt="A task that needs input. The banner says the agent ended its turn, the sidebar card has an Answer button, and the stage card in Progress offers Approve anyway, Mark as done and Start agent." width="1600" height="1000" loading="lazy" decoding="async" />
<img class="shot-light" src="/screens/ui/task-needs-you-light.webp" alt="A task that needs input. The banner says the agent ended its turn, the sidebar card has an Answer button, and the stage card in Progress offers Approve anyway, Mark as done and Start agent." width="1600" height="1000" loading="lazy" decoding="async" />
<figcaption>A task that needs your input, with the agent's message and the ways to go on.</figcaption>
</figure>

### Three views of the work

Switch the task list between **Table**, **Board** and **Progress**. The progress view lines up every task against its stages, with the next action for each one. The **Agents** view shows up to six live agent terminals in a grid. See [Dashboard](/docs/guides/dashboard/).

<figure class="shot">
<img class="shot-dark" src="/screens/ui/progress-dark.webp" alt="The Progress view: one row per task, one column per stage from Context to Release, with a marker for the state of each stage and a Next button such as Answer or Review." width="1600" height="1000" loading="lazy" decoding="async" />
<img class="shot-light" src="/screens/ui/progress-light.webp" alt="The Progress view: one row per task, one column per stage from Context to Release, with a marker for the state of each stage and a Next button such as Answer or Review." width="1600" height="1000" loading="lazy" decoding="async" />
<figcaption>The Progress view: where each task is in its pipeline, and what it needs next.</figcaption>
</figure>

### Stages you can shape

Each project can change its pipeline in **Project settings**: pick stages from a catalog of 28 (Research, Security review, Manual testing, Release notes, and more), run stages side by side, choose which ones wait for you, give a stage its own agent or model, check a stage on CI, or run it as a Claude Code workflow. You can split the pipeline into paths with a condition, such as a review path and a develop path. A review agent or a CI check can send a task back to an earlier stage, and each stage can use its own agent, such as Claude Code to write, Cursor to review and Copilot to watch CI. Settings are saved in `.natamux.json`, so the whole team can commit and share one process. See [Project settings](/docs/guides/project-settings/), [Build a workflow](/docs/guides/workflows/) and [Mix agents in one task](/docs/guides/mix-agents/).

### Review and ship from the dashboard

The **Changes** tab shows side-by-side or inline diffs, image diffs, and a box to send a comment on a line straight to the agent. **Generate** asks the agent for a commit message. **Commit all**, **Push** and **Create PR** finish the job, and the task is done when the pull request is merged. See [Changes and history](/docs/guides/changes-and-history/) and [GitHub](/docs/guides/github/).

<figure class="shot">
<img class="shot-dark" src="/screens/ui/changes-generate-dark.webp" alt="The Changes tab: a list of changed files, a side by side diff, and under the list a commit message drafted by Generate with the Commit all button." width="1600" height="1000" loading="lazy" decoding="async" />
<img class="shot-light" src="/screens/ui/changes-generate-light.webp" alt="The Changes tab: a list of changed files, a side by side diff, and under the list a commit message drafted by Generate with the Commit all button." width="1600" height="1000" loading="lazy" decoding="async" />
<figcaption>Read the diff, let the agent draft the commit message, then commit.</figcaption>
</figure>

### Five agents, and any other

Natamux starts Claude Code, Codex, Gemini CLI, GitHub Copilot CLI and Cursor in the task's terminal, and adds hooks to each session, so the dashboard knows when an agent works, asks or finishes. Any other terminal agent works too, with the terminal bell and `natamux ask` / `natamux done`. See [Agents and hooks](/docs/concepts/agents-and-hooks/).

### A command line for you and your agents

`natamux`, short name `nmx`, creates tasks, moves stages, and lets agents report: `natamux ask "Which API?"`, `natamux done`. See [Use the command line](/docs/guides/command-line/).

### Desktop app or browser

Use the desktop app for macOS, Windows or Linux, which starts everything for you and updates itself, or run the daemon and open the dashboard in a browser tab, also [in Docker](/docs/guides/self-host-docker/).

### No account and no telemetry

Natamux runs on your machine, with no account, no server of its own and no analytics. Besides the update check of the desktop app, it talks to GitHub only through your own `git` and `gh` login, for example to follow your pull requests and CI. Your agents talk to their own services, with their own logins. See [Security and privacy](/docs/help/security-and-privacy/).

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

1. [Install Natamux](/docs/getting-started/install/): the desktop app for macOS, Windows or Linux.
2. [Quick start](/docs/getting-started/quick-start/): from first launch to a merged change.
3. [The review loop](/docs/concepts/review-loop/): how the default pipeline runs three agents.
   Then [Build a workflow](/docs/guides/workflows/) and [Mix agents in one task](/docs/guides/mix-agents/) to shape your own.
4. [Tasks](/docs/concepts/tasks/), [Workspaces](/docs/concepts/workspaces/) and [Stages](/docs/concepts/stages/): the core ideas.
5. [Troubleshooting](/docs/help/troubleshooting/) and the [FAQ](/docs/help/faq/).
