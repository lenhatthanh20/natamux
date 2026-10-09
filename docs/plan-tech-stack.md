# Helmsman — Tech Stack Plan

Status: draft v0.1. Pairs with `plan-features.md`. Choices marked **Decide** are open.

## 1. Goals that drive the stack

- Local first, one user, one machine to start.
- A background daemon owns all state. Every client (dashboard, CLI, desktop shell, agents) talks to it.
- The dashboard works in a plain browser tab, so the desktop shell can come later.
- Small dependency tree. Fewer packages means a smaller supply-chain risk.
- Everything in one language (TypeScript) to keep the project small for one developer.

## 2. Architecture

```
          ┌────────────┐   ┌───────────┐   ┌──────────────┐
          │ Dashboard  │   │    CLI    │   │ Agent (hooks │
          │ (web, React)│   │ helmsman  │   │  + CLI calls)│
          └─────┬──────┘   └─────┬─────┘   └──────┬───────┘
                │ HTTP + WebSocket│                │
                └────────┬────────┴────────────────┘
                         ▼
                ┌──────────────────┐
                │  Daemon          │
                │  - task service  │
                │  - workspace svc │ ── git (worktree, diff, commit)
                │  - agent runner  │ ── PTY ── agent CLI (Claude Code …)
                │  - event bus     │
                │  - store         │ ── files + SQLite
                └──────────────────┘
```

- One daemon process per user. It binds to `127.0.0.1` only.
- Clients never touch git, files or PTYs directly.
- Agents report back through the CLI or hooks, which call the daemon.

## 3. Stack by layer

| Layer | Choice | Why |
|---|---|---|
| Language | TypeScript (strict) | One language for daemon, CLI and UI |
| Runtime | Node.js, current LTS | Mature PTY and git tooling |
| Package manager | pnpm, version pinned in `packageManager` | Fast, strict. Pin a known-good 10.x because some releases have shipped broken |
| Monorepo | pnpm workspaces, no extra build orchestrator at first | KISS; add one only if builds get slow |
| Daemon HTTP | Fastify | Small, fast, good TypeScript support |
| Realtime | WebSocket for events; plain HTTP for commands | Dashboard updates live |
| Schemas | Zod; one shared package for API and event types | One source of truth for client and server |
| PTY | node-pty | Standard way to run agent CLIs in a terminal |
| Terminal UI | xterm.js | Standard in-browser terminal |
| Git | Shell out to the `git` binary | Worktree support is complete there; no heavy library |
| Storage: tasks | Markdown files with YAML front matter in `.helmsman/tasks/` of each repo | Human-readable, diffable, git-friendly |
| Storage: runtime | SQLite through Node's built-in `node:sqlite` (Node 22.13 or newer) | Fast queries, event log, index of the task files. No native package to install. Marked "active development" in Node 22, so keep all SQL in one file (`task-index.ts`) |
| Index rule | The SQLite index is a cache that can be rebuilt from the files | No lock-in, easy recovery |
| UI framework | React + Vite | Large ecosystem, fast dev loop |
| UI components | Tailwind CSS + shadcn/ui primitives | Quick to build a clean, consistent dashboard |
| UI state | TanStack Query for server data; Zustand for local UI state | Small and well understood |
| Diff view | CodeMirror 6 merge view, or Monaco diff editor (**Decide**) | Both render split and unified diffs |
| CLI | A small TypeScript CLI using Node's built-in `parseArgs` and `fetch`, no extra package | Agents can use it from any shell |
| Lint and format | Biome | One fast tool instead of two |
| Unit tests | Vitest | Same toolchain as Vite |
| UI and end-to-end tests | Playwright | Drives the real dashboard |
| CI | GitHub Actions | Runs tests and builds in a clean environment |

## 4. Agent status events

Status must come from events, not screen scraping.

- **Native adapters:** each agent's hooks are turned into four signals (prompt sent, waiting for the user, turn ended, failed) by a small adapter. Claude Code's `http` hooks post straight to the daemon (`POST /hooks/claude-code`); agents that can only run a command use `helmsman hook <agent>`, which forwards the hook's JSON. See `agent-adapters.md` for what each agent offers and the design, and `claude-code-adapter.md` for the Claude Code mapping.
- **Generic adapter:** use process exit code, output-idle time and terminal bell. Mark states as "estimated" in the UI.
- **Git and CI:** watch the worktree for diff size; poll the GitHub API for PR and CI state.

Event shape (draft): `{ taskId, source, type, at, payload }`. The task file holds the current state. Each event is applied to that state and logged in SQLite, accepted or not. The log is append-only and is not rebuilt from the files.

## 5. Data model (draft)

Task file `.helmsman/tasks/<id>.md`:

```
---
id: T-0042
title: Fix timezone bug in reminders
state: running
priority: normal
labels: [bug]
agent: claude-code
branch: helmsman/t-0042-fix-timezone
workspace: <path>
pr: null
dependsOn: []
createdAt: 2026-10-08T09:00:00Z
updatedAt: 2026-10-08T09:00:00Z
---
Task description in markdown. Run notes are appended below.
```

SQLite tables: `tasks_index`, `events`, `workspaces`, `agent_runs`, `settings`.

## 6. Desktop shell (Decided)

Decision: **Electron + TypeScript**, for **macOS and Windows**. Linux is not a target for now.

Why:
- The daemon is Node and uses `node-pty`. Electron runs it with no extra sidecar process.
- Tauri would need a Rust toolchain and a per-OS sidecar for the daemon. Too much for one developer.
- Swift is macOS only and would need a second codebase for Windows. Rejected.
- Several comparable open source ADEs (Orca, Superset, Emdash, Crystal) use Electron + TypeScript. Checked on GitHub on 2026-10-08.

Order of work:
- The MVP still runs the dashboard in a browser tab served by the daemon. This keeps development possible in Codespaces.
- The Electron shell comes after the daemon and dashboard are stable.
- The shell only hosts the web dashboard and starts the daemon. No business logic lives in it.

Tooling: plain `tsc` for the shell (`packages/desktop`), no bundler; the daemon runs in an Electron utility process and serves the built dashboard itself (`HELMSMAN_WEB_DIR`). `electron-builder` for packaging and auto-update (step 11). Built on 2026-10-09, see `packages/desktop/README.md`.

### Native code per OS

Keep a small `native` area for things the web layer cannot do. Add only when needed:
- Notifications and their permission state.
- Global shortcuts and keyboard layout quirks.
- Windows: PATH, registry and launcher details for the `helmsman` command.
- macOS: Dock and menu bar details.

### Windows notes (verify before promising support)

- Terminals use ConPTY. Test agent CLIs under PowerShell, cmd and WSL.
- Keep worktree folder names short; long paths and `\` versus `/` cause problems with git.
- The `helmsman` command must be callable from the shell that the agent hooks use.
- Check how each agent CLI installs and runs on Windows before listing it as supported.

### Distribution costs

- macOS: Apple Developer account for code signing and notarization.
- Windows: a code signing certificate, or users see SmartScreen warnings.
- Auto-update channel and release hosting.
- CI on GitHub Actions runners for macOS and Windows.

## 7. Security

- The daemon listens on `127.0.0.1` only, with a random token required on every request and WebSocket.
- No remote command execution in the MVP. Remote workspaces come later with explicit design.
- Never store provider API keys. Agents use the user's own CLI login.
- Destructive actions (remove workspace, delete branch, delete task) always ask for confirmation first.
- Validate every API input with Zod. Never build shell commands from task text; pass arguments as arrays.
- Treat task descriptions and agent output as untrusted data when rendering (escape, no raw HTML).

## 8. Supply-chain hygiene

- Keep dependencies few. Review any package before adding it.
- Commit the lockfile. CI installs with the frozen lockfile.
- Check whether the pinned pnpm supports a minimum release age for new package versions; turn it on if it does.
- Disable install scripts by default; allow them only for packages that need them (such as `node-pty`).
- Develop and run in a dev container or Codespaces. On the personal machine, edit files only.
- Enable dependency and secret scanning in the GitHub repo.

## 9. Repository layout (draft)

```
helmsman/
├── docs/                     plans and decisions
├── packages/
│   ├── core/                 shared types, schemas, state machine
│   ├── daemon/               Fastify server, services, adapters
│   ├── cli/                  `helmsman` command
│   ├── web/                  React dashboard
│   ├── desktop/              Electron shell (after the MVP)
│   ├── native/               per-OS code, only when needed
│   └── claude-plugin/        Claude Code plugin (skills, hooks)
├── .devcontainer/
├── package.json
└── pnpm-workspace.yaml
```

## 10. Build order

1. `core`: task model, state machine, event types, with unit tests.
2. `daemon`: task store (files and SQLite), HTTP and WebSocket API, no agents yet.
3. `cli`: create and list tasks, send events.
4. `web`: dashboard table and needs-me strip, live over WebSocket.
5. Workspaces: git worktree create and clean up, with confirmation.
6. Agent runner: PTY, terminal tab, Claude Code adapter with hooks.
7. Diff view, notifications.
8. Run the MVP success checks from `plan-features.md`.
9. `desktop`: Electron shell that starts the daemon and hosts the dashboard.
10. Test on macOS and Windows in CI; add `native` code only where tests show a gap.
11. Signing, notarization and auto-update.

## 11. Open decisions

- CodeMirror or Monaco for diffs.
- License: open source core or source-available (see `plan-features.md`, section 9).
- Whether to reuse any part of Orca (MIT). Default: write from scratch and use it for ideas only.
