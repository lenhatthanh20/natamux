# Natamux — Feature Plan

Status: historical feature plan. Natamux is an independent product. The current native app, daemon and CLI are Rust; see [Rust migration](plan-rust-migration.md). Other tools are used as reference only; no code, text or structure is copied.

## 1. Vision

Natamux is an Agent Development Environment (ADE) built around one question: **what is every agent doing right now, and which one needs me?**

- Run many coding agents in parallel, each in its own isolated workspace.
- Manage the work as tasks, not as terminal tabs.
- Show the state of every task in one glance on a dashboard.

Tagline: *The next generation of an ADE.*

## 2. Principles

- Task first. A task owns its workspace, its agent runs, its diff and its pull request.
- Status is a fact, not a guess. It comes from agent events (hooks, exit codes, git, CI), never from parsing terminal text when an event exists.
- Agent-agnostic. Natamux is for the public, so it works with the agents people already use: Claude Code, Codex, Gemini CLI, Cursor, Copilot CLI, OpenCode and any other command line agent. Exact states come from each agent's hooks; an agent without hooks is followed through its terminal. See `agent-adapters.md`.
- Local first. Data lives on the user's machine, in plain files where possible.
- Keyboard first, fast to open, fast to read.
- Small surface. Few features done well beats a long list.

## 3. Reference research

Findings come from third-party comparison articles and project pages. They are not verified against each product. Re-check before relying on any row.

| Tool | Notable features | What Natamux takes from it |
|---|---|---|
| Orca (stablyai/orca, MIT) | Parallel worktrees; many CLI agents; GitHub and Linear tasks; SSH worktrees; CLI that agents can drive; supervised orchestration (run, task, dispatch, worker_done, escalation); plugin API with panels and events; mobile companion | Agents can drive the app through a CLI; worktree-per-task; plugin panels; status events |
| Conductor | Strong UX; worktree per agent; diff-first review; one-click PR; CI checks inline; closed source, Mac only | Diff-first review; one-click PR; polished first run |
| Superset | Any CLI agent; works on main branch too; built-in browser; terminal multiplexing; one-click PR; open source | Not forcing a worktree for every task; stable performance |
| Emdash | 30+ agents; local or remote worktrees; task intake from GitHub, Linear, Jira and others; reusable prompts and skills; shared MCP config; scheduled runs | Task intake from trackers; reusable prompts; scheduled runs |
| Vibe Kanban | Kanban board; issue to workspace; inline diff comments the agent can act on; rebase conflict help; PR with generated description. The company shut down in April 2026; the project continues in the community | Board view; review comments that go back to the agent |
| Claude Squad | Terminal app; tmux plus worktrees; agent-agnostic | A terminal-only mode is possible later |
| Cursor (Agents Window) | Up to 8 parallel agents; local, worktree, cloud and SSH; background agents | A single window for all agent runs |
| Warp (Oz) | Terminal-first; cloud orchestrator for background work; agents can attach to live PTY sessions | Optional remote and background runs (later) |
| Zed | Tabbed parallel agent threads | Fast switching without losing scroll or context |
| Sculptor | Container isolation; pairing mode | Optional container isolation (later) |
| Nimbalyst (was Crystal) | Parallel sessions plus visual editing of markdown and mockups. Crystal was deprecated in February 2026 | Rich markdown view for task notes |

Lesson from the market: several free ADEs stopped or shut down. Natamux must have a clear reason to exist and a clear way to be paid for (see section 9).

## 4. Differentiators

1. **Dashboard built for glancing.** A "needs me" queue first, everything else second.
2. **Task model with approval gates.** A task can pause for a human at chosen points (plan approved, review, ready to merge).
3. **Event-based status.** Reliable state from Claude Code hooks and git, not screen scraping.
4. **Plain-file tasks.** Tasks are readable files in the repo, so they work with git, grep and any editor.

Open question: is the combination in 1–4 strong enough on its own? Validate with 5–10 target users before building past the MVP.

## 5. Feature list

Tags: **MVP** = first usable release. **v1** = first public release. **Later** = after feedback.

### 5.1 Workspaces and agents

| Feature | Tag |
|---|---|
| Add a git repo; list its branches. Done: the daemon's own repository plus any added in **Projects** (kept in `<data dir>/repos.json`); each task belongs to one, and its workspace, branches and issues come from there. An added project is removed from the menu of its sidebar group, once it has no workspaces; nothing on disk changes | MVP |
| Create a workspace for a task (git worktree, branch named from the task) | MVP |
| Option: run a task in the main checkout, no worktree | v1 |
| Start an agent in a workspace terminal (Claude Code first; Codex, others via adapters). Done: Claude Code, with its hooks given through `--settings` | MVP |
| Embedded terminal per workspace (multiple tabs). Done: several terminals per task, as tabs; each keeps running when the dashboard closes, with recent output on reconnect. The agent runs in terminal 1 | MVP |
| Setup script per repo (install deps, copy env files) when a workspace is created. Done: `.natamux-setup` (or `.natamux-setup.ps1` on Windows) in the repository runs in the task's first terminal | v1 |
| Port allocation per workspace so dev servers do not clash. Done: each task has 10 ports from 20000 + 10 × its number; its terminals get `PORT` and `NATAMUX_PORT` | v1 |
| Clean up a workspace: confirm first, then remove worktree and branch. Done: also all finished (done or cancelled) workspaces of a project at once, from its sidebar group. A finished task says what is left (changes, push, pull request, branch) in a banner and a wrap-up, with the next step; the remove dialog refuses a worktree with changes that are not committed, names the programs it stops, and deletes the branch only when it is merged (also a squash merge of a merged pull request that origin has) | MVP |
| Fan-out: one prompt to several agents, compare results, keep the winner. Done: **Fan out** copies a task 1 to 5 times, each with its own workspace and Claude Code; the copies share a `tries-…` label and are compared side by side in the details (state, diff size, pull request); **Keep this one** cancels the other tries | v1 |
| SSH and remote workspaces | Later |
| Container isolation | Later |
| Scheduled and background runs | Later |

### 5.2 Task management

Scope: tasks only. No sprints, no teams, no rosters, no diaries.

| Feature | Tag |
|---|---|
| Task fields: id, title, description, state, priority, labels, agent, workspace, branch, PR link, created and updated time | MVP |
| States: `draft`, `ready`, `running`, `needs-input`, `review`, `blocked`, `failed`, `done`, `cancelled` | MVP |
| Create a task from the app, the CLI, or an agent | MVP |
| Subtasks (one level). Done: add them in the details of a task (which counts how many are done), with **Add subtask…** in the row menu, or with `natamux task create --parent`; a subtask shows its parent, its context names the parent, it starts from the parent's branch, and its pull request goes into the parent's branch; the next stage of the parent waits for open subtasks and pulls their merged work | v1 |
| Dependencies between tasks (a task waits until others are `done`). Done: set in the edit form; unknown tasks and loops are refused; a waiting task shows what it waits for and cannot start an agent | v1 |
| Approval gates: a task can require a human decision before it moves on. Done: each stage has a manual or auto gate (docs/design-task-stages.md) | v1 |
| Stages per project: Context → Spec → Plan → [Test spec ‖ Implement] → Review → Testing → Release by default, changed in `.natamux.json` (drop, reorder, add, run side by side). Done: the daemon starts each stage, takes `natamux done`, checks the result (file, sections, commits, clean worktree, a command), and sends failed checks back to the agent once. A stage can end its agent and close its terminal when it is done (`endShell`) | v1 |
| Run a hard stage as a Claude Code workflow (sub-agents, more accurate, more tokens). Done: the project settings choose the stages that offer it (Plan and Implement by default); each task ticks **Run with a workflow** on a stage, off by default; the stage prompt then asks for a workflow | v1 |
| Task context from typed notes, a GitHub issue (with comments), a skill or an MCP server, written to `context.md`. Any skill of the agents can be picked when starting a task. Done | v1 |
| Attach files (documents, screenshots) to a new task. Done: pick, drop or paste them in the Start a task dialog, or `natamux task create --attach`; up to 20 files, 20 MB each, 50 MB together. They are kept with the stage files, copied into `.natamux-task/attachments/`, listed in `context.md` and in the Docs tab | v1 |
| Stage files (`context.md`, `spec.md`, `plan.md`, `test-spec.md`) in the worktree, ignored by git, kept with the task, editable in the Docs tab. Done | v1 |
| Event log files: every event is also kept in `tasks/T-0001.events.jsonl`, and the index is rebuilt from them. Done | v1 |
| Task notes as markdown, with a run log. Done: edit title, notes, priority and labels; every change is in the activity list | MVP |
| Reusable task templates and prompts. Done: save a task as a template (notes, priority, labels) and pick it when adding a task; templates are markdown files in `<data dir>/templates` | v1 |
| Import tasks from GitHub issues. Done: pick open issues (`gh issue list`) and make draft tasks; an issue is imported once | v1 |
| A GitHub issue for a task made in the app. Done: opened when the spec is approved (or with the pull request, or by hand), and the pull request body says `Closes #N`; `natamux task link` links a ticket elsewhere | v1 |
| Import from Linear, Jira and others | Later |
| Search and filter by state, label, agent | MVP |

State changes come from events first (agent finished, agent asked a question, CI failed, PR merged). A human can always override a state.

### 5.3 Dashboard (task status at a glance)

The dashboard is the home screen.

Goals:
- Answer "what needs me?" in under 3 seconds.
- Update live, with no refresh.
- Work in the desktop app and in a plain browser tab.

| Element | Tag |
|---|---|
| **Needs me** strip at the top: tasks in `needs-input`, `review`, `failed`, `blocked`, sorted by waiting time | MVP |
| A running task with no sign of life for 10 minutes is marked "quiet" and listed in the strip | MVP |
| Details of a task: what happened, from whom and when (activity list) | MVP |
| Task table: title, state badge, agent, last activity ("2m ago"), elapsed time, branch, diff size (+/−), PR and CI status. Done; elapsed time is the age of the task | MVP |
| Counts per state in a header bar; click to filter | MVP |
| Board view (columns by state), toggle with the table. Done; the choice is kept per browser | v1 |
| Progress view: every task against the stages of its pipeline, with turning, breathing and done markers; a Progress view in each task's details panel, beside the terminal, with the time and evidence of each stage. Done | v1 |
| One-line live summary per running task (what the agent is doing now). Done for Claude Code: the tool it is about to run (`PreToolUse`), such as "Edit src/a.ts", under the title of a running task | v1 |
| Keyboard: `j/k` move, `enter` open, `a` approve, `r` request changes, `/` search | MVP |
| Desktop and system notifications when a task needs me or finishes. Done in the browser: a notification when a task starts to wait or is done while the tab is in the background, and the count in the tab title | MVP |
| Compact "wall" mode for a second monitor | Later |
| Mobile-friendly read-only page (reachable on the local network) | Later |
| History and stats (tasks per day, time in each state) | Later |

### 5.4 Review and delivery

| Feature | Tag |
|---|---|
| Diff view per task (unified and split). Done: changed files list, a Monaco diff of uncommitted changes, and a git graph of the task branch against its base | MVP |
| Inline comments that are sent back to the agent as a follow-up prompt. Done: a comment under the diff, about the file and the line the cursor is on, is pasted into the agent's terminal as its next prompt. Lines selected in the diff can be added, and are quoted to the agent | v1 |
| Roll back uncommitted changes. Done: in the Changes tab, **Rollback…** on the list, on a file or folder row, or `Mod+Alt+Z` asks which files go back to the last commit; new files are deleted, conflicts are refused. A snapshot made first lets the toast **Undo** the last rollback | v1 |
| Commit from the app with a message draft. Done: commit all changes with a message; **Generate** and **Commit…** draft the message with the task's agent, with a simple draft when the agent fails | v1 |
| One-click push and PR creation (GitHub first). Done: push, and a pull request through `gh` whose address is kept on the task; a pull request that the agent or a person opened from the task branch is found and linked | v1 |
| CI status shown on the task. Done: while a dashboard is open, the daemon reads the open pull requests of each repository with one `gh api graphql` call once a minute (every 5 minutes once all checks have finished); the table shows its state and checks, and a merged pull request finishes a task in review | v1 |
| Files explorer of the worktree. Done: a **Files** view in the details lists the worktree a folder at a time; a file dragged onto a terminal of the task types its path; one file at a time opens in an editor tab (Monaco), saved with a version check so an agent's newer file is not replaced unseen | v1 |
| Rebase or merge conflict help | Later |
| Built-in browser to preview the running app | Later |

### 5.5 Agent integration

| Feature | Tag |
|---|---|
| Adapter interface: one function per agent that turns its hook input into a signal (prompt sent, waiting for the user, turn ended, failed) | MVP |
| Claude Code adapter using its hooks for status events | MVP |
| `natamux hook <agent>` and `POST /hooks/:agent`, so agents that can only run a command can report | MVP |
| Codex, Gemini CLI, Cursor and GitHub Copilot CLI adapters from their hooks. Done: each agent starts from the dashboard with hooks for its session only (docs/agent-adapters.md); Copilot checked in a real session, the others against their documented input | v1 |
| Generic adapter for any CLI (status from process and output idle time, terminal bell). Done: output in a task's first terminal is a sign of life (so a silent agent shows as quiet), and the terminal bell from an agent without hooks moves a running task to needs input | v1 |
| OpenCode plugin | v1 |
| Agent Client Protocol (ACP) client | Later |
| Natamux CLI that agents can call: create task, update state, ask a question, report done. Done: `task create`, `event`, `ask`, `done`, plus `task edit` and `agent start` | MVP |
| Natamux plugin for Claude Code (skills that use the CLI). Done: `plugins/natamux`, with a `natamux-task` skill, installable from this repository as a marketplace | v1 |
| Tokens and estimated cost (USD, API list prices) per task and per stage, for each agent that writes its usage to disk. Done: Claude Code, Codex, Gemini CLI, Copilot CLI (Cursor writes none); a **Usage** section in the details, a line on each stage card, a **Cost** column in the table | v1 |
| Account limits and plan meters per agent | Later |

### 5.6 Extensibility and sharing

| Feature | Tag |
|---|---|
| Plugin API: panels, commands, event subscriptions | Later |
| Share task templates and skills as files | Later |
| Team sync of tasks | Later |

## 6. Non-goals

- Sprint planning, retrospectives, team rosters, diaries.
- A code editor that competes with VS Code or Cursor.
- A knowledge or memory base. Many tools already do this, and it is not the focus.
- Hosting or reselling model compute. Users bring their own agent subscriptions.

## 7. Release plan

1. **MVP**: daemon, CLI, Claude Code adapter, worktree tasks, terminal, task states, dashboard (table plus needs-me strip), diff view, notifications. Runs in a browser tab, no desktop shell.
2. **v1**: desktop app (now Rust and GPUI), board view, approval gates, dependencies, GitHub import and PR, inline review comments, templates, generic adapter, Claude Code plugin.
3. **Later**: remote workspaces, scheduling, plugin API, mobile read-only page, team sync.

## 8. Success checks for the MVP

- Start 3 agents on 3 tasks from one repo in under 2 minutes.
- The dashboard shows the right state for each within 2 seconds of a change.
- A user can name which task needs them from the dashboard alone.
- Closing and reopening the app restores all tasks and states.

## 9. Product and business notes

- License and pricing are not decided. Options: open source core with a paid team tier, or source-available. Decide before the first public release.
- Keep the name, code and text independent from any employer or other product. Do not use the names Orca or Atlas.
- Orca is MIT licensed. If any Orca code is ever reused, keep its copyright and license notice.
- Check the author's employment contract on outside work before publishing. This is a personal responsibility, not a design item.

## 10. Open questions

- Which agent CLIs work well on Windows? Check before listing them as supported.
- Is the Claude Code hook event set rich enough for every state? Verify before the adapter design is fixed.
- Should gates be per task or per template?
- Which tracker comes first after GitHub?

## Sources

- Orca: https://github.com/stablyai/orca
- ADE comparisons: https://superset.sh/compare/best-agent-development-environment
- Open source agent workspaces: https://nimbalyst.com/blog/open-source-agent-workspace-alternatives-2026/
- Cursor Agents Window: https://cursor.com/changelog/3-0
- Vibe Kanban: https://vibecodinghub.org/tools/vibe-kanban
- Parallel agent tools overview: https://dev.to/stravukarl/best-tools-for-managing-parallel-ai-coding-agents-in-2026-14l8
