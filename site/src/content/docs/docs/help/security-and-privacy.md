---
title: Security and privacy
description: What Natamux protects, what it collects, and how to report a vulnerability.
sidebar:
  order: 3
---

This page summarizes [SECURITY.md](https://github.com/lenhatthanh20/natamux/blob/main/SECURITY.md) and [PRIVACY.md](https://github.com/lenhatthanh20/natamux/blob/main/PRIVACY.md). The files on GitHub are the full text.

## Privacy

- Natamux has no account, no server of its own and no telemetry. It sends no usage data, crash reports or analytics.
- Your data stays on your machine:
  - Tasks, history and stage files: `.natamux/` in each repository.
  - Worktrees: `.natamux/worktrees/`, or the folder you set.
  - Desktop app token, update setting, downloaded updates and logs: its user data folder (`~/Library/Application Support/Natamux` on macOS, `~/.config/Natamux` on Linux).
  - Dashboard preferences and the token: your browser's `localStorage`.
- Data leaves your machine only when you or an agent you start makes it happen:
  - Coding agents send your prompts and code to their own companies, under their terms. Natamux does not see or change that.
  - GitHub actions (push, pull request, issue import) use your own `git` and `gh` login.
  - The desktop app asks `github.com` for the latest release about 30 seconds after it starts and every 6 hours, and downloads a new version from GitHub. The request carries no data about you or your tasks; GitHub sees your IP address and the app version. Untick **Check for Updates Automatically** to stop the timed checks. See [Updates](/docs/guides/desktop-app/#updates).

## Agents run with your rights

An agent can read and change files and run commands as your user. Natamux does not sandbox agents. Treat an agent like a person typing in your terminal, and use each agent's own permission settings.

Each task works in its own git worktree. Natamux never pushes, merges or deletes a branch unless you click the button or run the command.

## The daemon

- It listens on `127.0.0.1` (loopback) by default. Only the Docker setup listens on all interfaces inside its container, and `compose.prod.yaml` publishes the port to loopback only.
- Every API request needs the token in an `Authorization: Bearer` header. `/health` and the static dashboard files are the only exceptions.
- Without `NATAMUX_TOKEN`, the daemon makes a random token for each run and prints it once.
- The token travels in a header, never a cookie or a URL, so other websites cannot use your session. Terminals open with a one-time ticket that expires after 30 seconds.
- Do not run the daemon on a network address without TLS. Put a reverse proxy with TLS in front of it.

## Secrets on disk

- The desktop app keeps its token in its user data folder, readable only by your user (mode `0600`).
- The scripts that start agents are written with mode `0600`.
- Sign out in the dashboard's account menu to remove the token from the browser.

## Updates

- Updates come over HTTPS from the latest GitHub release of the repository.
- The app compares the SHA-256 of the download with `SHA256SUMS.txt` of the same release. On macOS it also checks the new app's signature and version before the swap.
- The checksum file comes from the same release, so it catches a broken download, not a forged release. Until the app is signed with a certificate, the trust rests on the GitHub repository.

## Hooks

For each agent it starts, Natamux adds hook settings that call `natamux hook` or the daemon's local address, with the task id and the token from the terminal's environment. For Copilot and Cursor, the hook file goes into the task's worktree and is kept out of git. For Codex, Natamux passes `--dangerously-bypass-hook-trust`. See [Agents and hooks](/docs/concepts/agents-and-hooks/).

## Folder trust

Natamux marks each task's worktree as trusted for the agent it starts, so the agent does not ask. For two agents this writes files outside the worktree:

- Cursor CLI: `~/.cursor/projects/<folder>/.workspace-trusted` (under `$CURSOR_DATA_DIR` when it is set).
- Copilot CLI: `~/.copilot/config.json`, or `$COPILOT_HOME/config.json` when it is set. Natamux adds the worktree to its `trustedFolders`.

Natamux does not remove these entries when it removes a task. See [Folder trust](/docs/concepts/agents-and-hooks/#folder-trust).

## Permission prompts

By default Natamux starts each agent with a flag that skips most permission prompts. Only Claude Code (`--permission-mode auto`) checks each action. Codex (`--sandbox workspace-write --ask-for-approval never`), Gemini CLI (`--approval-mode yolo`), Copilot CLI (`--allow-all-tools`) and Cursor CLI (`--force`) run commands without asking, in the task's worktree with your user rights. Set `"agents": { "permissionMode": "ask" }` in [`.natamux.json`](/docs/reference/configuration/#agents) to keep the prompts.

## The desktop app is unsigned

The first releases are not signed with an Apple or Windows certificate. The macOS app has an ad hoc signature only. Download it only from the repository's Releases page.

## Report a vulnerability

Do not open a public issue. Use [private vulnerability reporting](https://github.com/lenhatthanh20/natamux/security/advisories/new) on GitHub (**Security → Report a vulnerability**). Include the version, your platform, the steps to reproduce and what an attacker gains.

You get an answer within 7 days. Only the latest release gets security fixes.
