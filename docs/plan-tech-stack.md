# Natamux — Tech Stack Plan

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
          │ (web, React)│   │  natamux  │   │  + CLI calls)│
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
| Storage: tasks | Markdown files with YAML front matter in `<data dir>/tasks/` | Human-readable, diffable, easy to back up |
| Storage: runtime | SQLite through Node's built-in `node:sqlite` (Node 22.13 or newer) | Fast restart, a copy of the event logs, index of the task files. No native package to install. Marked "active development" in Node 22, so keep all SQL in one file (`task-index.ts`) |
| Index rule | The SQLite index is a cache that can be rebuilt from the files. No other database (section 5) | No lock-in, easy recovery |
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

- **Native adapters:** each agent's hooks are turned into four signals (prompt sent, waiting for the user, turn ended, failed) by a small adapter. Every agent's hooks run `natamux hook <agent>`, which forwards the hook's JSON to `POST /hooks/<agent>`. Only a Claude Code session whose worktree file is the user's own gets `http` hooks that post straight to the daemon. See `agent-adapters.md` for what each agent offers and the design, and `claude-code-adapter.md` for the Claude Code mapping.
- **Generic adapter:** use process exit code, output-idle time and terminal bell. Mark states as "estimated" in the UI.
- **Git and CI:** watch the worktree for diff size; poll the GitHub API for PR and CI state.

Event shape (draft): `{ taskId, source, type, at, payload }`. The task file holds the current state. Each event is applied to that state and logged, accepted or not: first in `tasks/<id>.events.jsonl`, then in SQLite. The log is append-only; SQLite is rebuilt from it.

## 5. Data model (draft)

Task file `<data dir>/tasks/<id>.md`:

```
---
id: T-0042
title: Fix timezone bug in reminders
state: running
priority: normal
labels: [bug]
agent: claude-code
branch: natamux/t-0042-fix-timezone
workspace: <path>
pr: null
dependsOn: []
createdAt: 2026-10-08T09:00:00Z
updatedAt: 2026-10-08T09:00:00Z
---
Task description in markdown. Run notes are appended below.
```

SQLite tables (`task-index.ts`): `tasks_index`, `events`, `task_files`.

### Storage decision (2026-10-18)

Files are the source of truth. `index.sqlite` is a cache. Natamux uses no other database.

Why:
- One daemon per user writes all data, so no locking between processes is needed.
- The data is small. The daemon keeps every task in memory; SQLite only makes restarts fast.
- Files are easy to read, diff and back up, and survive a lost `index.sqlite`.
- A database server or an ORM adds an install step or a package for a single-user app.

Where data lives (`<data dir>` is `NATAMUX_DATA_DIR`, default `<cwd>/.natamux`; the desktop app uses its app data folder):

| Data | Place |
|---|---|
| Projects | `repos.json`, `repos-removed.json`, `repos-default-added` |
| Project settings, agent choice and permission mode | `.natamux.json` in each repository |
| Tasks | `tasks/<id>.md` |
| Task events | `tasks/<id>.events.jsonl`, copied into SQLite |
| Stage files, attachments | `.natamux-task/` in the worktree, archived in `tasks/<id>/` |
| Shells to restore | `tasks/<id>.open-shells.json`, `tasks/<id>.shells.json` |
| Templates | `templates/<id>.md` |
| Agent hook settings | `claude-code-settings.json`, `gemini-settings.json` |
| Task list cache, file stats, event copy | `index.sqlite` (tables `tasks_index`, `events`, `task_files`) |
| UI preferences | browser `localStorage` |

Look at this again when:
- more than one process must write the same data, such as a shared team server;
- data must sync between machines or users;
- thousands of tasks make start-up or lists slow;
- a feature needs queries across data that now sits in separate JSON files.

Then first move more data into `index.sqlite`, before any new database engine.

## 6. Desktop shell

Decision (2026-10-22): move to **Rust + GPUI**, as `plan-rust-migration.md` describes. The `dev` branch holds that work. **Electron + TypeScript** stays the shipped app on `main`, with bug fixes only, until the phase 5 cut over.

The rest of this section is the Electron decision of 2026-10-08, kept for history and for the app that ships until then. Electron targets **macOS and Windows**.

Why Electron was chosen:
- The daemon is Node and uses `node-pty`. Electron runs it with no extra sidecar process.
- Tauri would need a Rust toolchain and a per-OS sidecar for the daemon. Too much for one developer.
- Swift is macOS only and would need a second codebase for Windows. Rejected.
- Several comparable open source ADEs (Orca, Superset, Emdash, Crystal) use Electron + TypeScript. Checked on GitHub on 2026-10-08.

Order of work:
- The MVP still runs the dashboard in a browser tab served by the daemon. This keeps development possible in Codespaces.
- The Electron shell comes after the daemon and dashboard are stable.
- The shell only hosts the web dashboard and starts the daemon. No business logic lives in it.

Tooling: plain `tsc` for the shell (`packages/desktop`), no bundler; the daemon runs in an Electron utility process and serves the built dashboard itself (`NATAMUX_WEB_DIR`). `electron-builder` for packaging and auto-update (step 11). Built on 2026-10-09, see `packages/desktop/README.md`.

### Native code per OS

Keep a small `native` area for things the web layer cannot do. Add only when needed:
- Notifications and their permission state.
- Global shortcuts and keyboard layout quirks.
- Windows: PATH, registry and launcher details for the `natamux` command.
- macOS: Dock and menu bar details.

### Windows notes (verify before promising support)

- Terminals use ConPTY. Test agent CLIs under PowerShell, cmd and WSL.
- Keep worktree folder names short; long paths and `\` versus `/` cause problems with git.
- The `natamux` command must be callable from the shell that the agent hooks use.
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
- Rust: no `cargo` on the owner's Mac; builds run in CI. `Cargo.lock` is committed and every build uses `--locked`. CI runs `cargo deny`, `cargo audit` and `cargo vet`; Dependabot updates crates after a 7-day cooldown. Details: `plan-rust-migration.md`, "Safe local setup".

## 9. Repository layout (draft)

```
natamux/
├── docs/                     plans and decisions
├── packages/
│   ├── core/                 shared types, schemas, state machine
│   ├── daemon/               Fastify server, services, adapters
│   ├── cli/                  `natamux` command
│   ├── web/                  React dashboard
│   ├── desktop/              Electron shell (after the MVP)
│   ├── native/               per-OS code, only when needed
│   └── claude-plugin/        Claude Code plugin (skills, hooks)
├── xtask/                    `cargo xtask ci` and `cargo xtask layers`
├── crates/                   Rust crates (from phase 1 of plan-rust-migration.md)
├── supply-chain/             cargo vet audits and imports
├── .devcontainer/
├── Cargo.toml                Cargo workspace
├── rust-toolchain.toml       pinned Rust version
├── deny.toml                 cargo deny rules
├── package.json
└── pnpm-workspace.yaml
```

Inside each package, code is grouped by feature (`tasks`, `stages`, `workspaces`, `terminals`, …). The rules are in `AGENTS.md` (Code layout).

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
