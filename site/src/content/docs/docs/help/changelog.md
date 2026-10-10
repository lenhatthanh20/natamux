---
title: Changelog
description: What changed in each Natamux release.
sidebar:
  order: 4
---

All releases are on [GitHub releases](https://github.com/lenhatthanh20/natamux/releases).

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

You need at least one agent installed and logged in, and `git`. See [Install](/natamux/docs/getting-started/install/).

### Known limits

- No Windows build.
- No automatic updates. Watch the repository's releases for new versions.
