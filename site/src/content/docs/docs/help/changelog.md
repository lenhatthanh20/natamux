---
title: Changelog
description: What changed in each Natamux release.
sidebar:
  order: 4
---

All releases are on [GitHub releases](https://github.com/lenhatthanh20/natamux/releases).

## Unreleased

- The **Progress** view moved from a workspace tab into the details panel, next to the terminal. Key `4` now shows **Docs**. The details panel is open by default beside a workspace. See [Progress](/docs/concepts/stages/#progress).
- The **Docs** tab shows Markdown as formatted text. Switch to **Source** for the raw text. See [The Docs tab](/docs/concepts/stages/#the-docs-tab).
- Each terminal shows the state of its own stage, not the state of the whole task.
- **Remove finished workspaces** in the menu of a project, and on its "N finished" row. See [Sidebar](/docs/guides/dashboard/#sidebar).
- Any project can be removed. The app opens with no project: use **File → Add Project…** (no restart). The live socket tells open dashboards when the project list changes.
- A task whose agent waits for background work stays running and says what it waits for. It is not quiet. See [Waiting for background work](/docs/concepts/agents-and-hooks/#waiting-for-background-work).
- The **Needs you** section has an amber frame and a bell, with two-line rows. A task whose workspace was removed leaves it.
- Fix: after you reconnect, a terminal shows the screen of the shell, not its raw output. The pane in use sets the size of the shell. See [Terminal](/docs/guides/terminal/).

## v0.1.3 (2026-10-11)

- **Remove project** in the menu of a repository's group in the sidebar. See [Repositories](/docs/guides/dashboard/#repositories).
- Agents start in auto mode, so they do not stop to ask before each command. Set `"agents": { "permissionMode": "ask" }` in `.natamux.json` to get the prompts back. See [Auto mode](/docs/agents/claude-code/#auto-mode).
- The Implement and Testing stages leave the changes uncommitted. You commit them in the **Changes** tab. See [Checks](/docs/concepts/stages/#checks).
- **New workspace** is now **Start a task**, a repository is a "project", and the dialog shows the **Notes** field at once.
- The task details panel starts closed. The terminal header shows the agent's icon and name.
- Fix: a terminal showed only its last 1,000 lines after you switched tasks or reloaded the dashboard. A shell now keeps all its output, and each pane keeps 10,000 lines of scrollback. See [Terminal](/docs/guides/terminal/).
- Removing a workspace works when its worktree folder is already gone.
- Fix: "command not found" when an agent starts. Task shells are login shells, and the app adds the Homebrew folders when it cannot read the login shell PATH.
- Fix: agent hooks failed behind a company proxy. Task terminals add `127.0.0.1`, `localhost` and `::1` to `NO_PROXY`.
- Only macOS on Apple silicon is built. The Intel Mac and Linux builds are paused; the last release with them is v0.1.2.

## v0.1.2 (2026-10-10)

- The desktop app updates itself. It checks GitHub for a new release, downloads it in the background, checks it against `SHA256SUMS.txt` and installs it on restart. See [Updates](/docs/guides/desktop-app/#updates).
- **Check for Updates…** and **Check for Updates Automatically** in the app menu (macOS) or the **Help** menu (Linux).

## v0.1.1 (2026-10-10)

- The macOS app is now `Natamux.app` (it was `natamux.app`).
- Download files no longer carry the version in their name, such as `Natamux-mac-arm64.dmg`. Links to the latest release always get the newest build.
- `natamux task show` without an id shows the task of the terminal you run it in.
- `natamux agent start` on a task with no workspace makes the workspace first (`--base` picks the branch). The dashboard and the CLI start the agent you picked, not the task's default one.
- The sign-in page says where the token comes from: `NATAMUX_TOKEN` of the daemon.

## v0.1.0 (2026-10-10)

The first release. Run many coding agents in parallel, each task in its own git worktree and terminal, with every task's state on one dashboard.

### What is in it

- **Tasks and a dashboard.** A table and a board of every task, with a "Needs you" strip. Keyboard first (`⌘K` or `Ctrl+K`).
- **A worktree per task.** Each task gets its own branch and folder.
- **Five agents.** Claude Code, Codex, Gemini CLI, GitHub Copilot CLI and Cursor. Natamux adds hooks for each session.
- **Stages.** Context, Spec, Plan, Test spec and Implement side by side, Review, Testing, Release. Change them per project in `.natamux.json`. Each stage checks its result before it moves on.
- **Changes and history.** A file tree, side by side or inline diffs, commit and push, open a pull request, and a branch graph.
- **The `natamux` command**, short name `nmx`, for you and for agents.
- **Browser or desktop app.** No account and no telemetry.

### Downloads

| System | File |
|---|---|
| macOS, Apple silicon | `Natamux-0.1.0-mac-arm64.dmg` |
| macOS, Intel | `Natamux-0.1.0-mac-x64.dmg` |
| Linux, any distribution | `Natamux-0.1.0-linux-x86_64.AppImage` |
| Debian, Ubuntu | `Natamux-0.1.0-linux-amd64.deb` |
| Linux, no install | `Natamux-0.1.0-linux-x64.tar.gz` |

`SHA256SUMS.txt` lists the checksum of every file.

### Before you open it

The release is not signed with an Apple certificate.

- **macOS:** drag Natamux to Applications and open it once. macOS blocks it. Open **System Settings → Privacy & Security** and click **Open Anyway**. If there is no such button, run:

  ```sh
  xattr -dr com.apple.quarantine /Applications/Natamux.app
  ```

- **Ubuntu 24.04 and later, AppImage:** the system can block Chromium's sandbox. Install the `.deb`, or start the AppImage with `--no-sandbox`.

You need at least one agent installed and logged in, and `git`. See [Install](/docs/getting-started/install/).

### Known limits

- No Windows build.
- No automatic updates. Watch the repository's releases for new versions.
