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
  - Desktop app token, update setting, downloaded updates and logs: its user data folder (`~/Library/Application Support/Natamux` on macOS, `%APPDATA%\Natamux` on Windows, `~/.config/Natamux` on Linux).
  - Dashboard preferences and the token: your browser's `localStorage`.
- Data leaves your machine only when you or an agent you start makes it happen:
  - Coding agents send your prompts and code to their own companies, under their terms. Natamux does not see or change that.
  - GitHub actions (push, pull request, issue import) use your own `git` and `gh` login.
  - When you tick **Check for Updates Automatically** (off by default), the desktop app asks `github.com` for the latest release about 30 seconds after it starts and every 6 hours, and downloads a new version from GitHub. The request carries no data about you or your tasks; GitHub sees your IP address and the app version. Untick it to stop the timed checks. See [Updates](/docs/guides/desktop-app/#updates).

## Agents run with your rights

An agent can read and change files and run commands as your user. Natamux does not sandbox agents. Treat an agent like a person typing in your terminal, and use each agent's own permission settings.

Each task works in its own git worktree. Natamux never pushes, merges or deletes a branch unless you click the button or run the command.

## The daemon

- It listens on `127.0.0.1` (loopback) by default. Only the Docker setup listens on all interfaces inside its container, and `compose.prod.yaml` publishes the port to loopback only.
- Every API request needs a token in an `Authorization: Bearer` header. `/health` and the static dashboard files are the only exceptions.
- Without `NATAMUX_TOKEN`, the daemon makes a random token for each run and prints it once.
- The token travels in a header, never a cookie, so other websites cannot use your session. Terminals open with a one-time ticket that expires after 30 seconds. Only the legacy web dashboard sends `?token=` on `/events`.
- The daemon checks the `Host` header against an allow-list (`127.0.0.1`, `localhost`, `::1`, plus `NATAMUX_ALLOWED_HOSTS`) and the `Origin` of WebSocket handshakes. A web page on another site cannot reach it, not even by DNS rebinding.
- Do not run the daemon on a network address without TLS. Put a reverse proxy with TLS in front of it.
- Task terminals get an [agent token](/docs/reference/http-api/#the-agent-token), not the daemon's token. It is per task and lives in memory only. An agent cannot approve, skip or pass a stage, change project settings or approve repo commands, and what it does is recorded as the agent's. The `natamux` command in a task terminal uses it too. This stops an agent that follows bad instructions, for example from a web page or an issue. It is not a sandbox: an agent runs as your user, so it can still read the daemon's token file.

## Repo trust

- A repo can carry commands: stage checks, path conditions, command stages and `args` in `.natamux.json`, and the setup scripts `.natamux-setup` and `.natamux-setup.ps1`. They run with your rights.
- Natamux runs none of them until you approve them for that repo, in the app dialog or with `natamux repo trust <repo> --approve <hash>`. A change to any of them needs a new approval. A repo with no commands needs none. Repos added before this check were approved once.
- An untrusted repo gets no repo commands, no setup script, no stage `args`, no auto mode and no folder trust for agents.
- Approval does not limit what an agent does inside its worktree.

## Secrets on disk

- The app folder and the daemon data folder are private to your user (mode `0700`).
- The desktop app keeps its token in its user data folder, readable only by your user (mode `0600`).
- The scripts that start agents are written with mode `0600`.
- In a browser, Sign out in the dashboard's account menu removes the token from that browser. The desktop app is native: it has no page and no Sign out.

## Updates

- Updates come over HTTPS from the latest GitHub release of the repository.
- `SHA256SUMS.txt` of each release is signed with [minisign](https://jedisct1.github.io/minisign/). The secret key stays offline with the maintainer, never on GitHub, so a stolen GitHub account cannot sign a release. The public key is built into the app:

  ```
  RWSgtwsXHppEP5n4kgg1Q3yNIYODQNGsMQBKF93tjSRwzL1BrjeGoBpo
  ```

- Before it downloads an update, the app checks that signature, and that it was made for that release (`natamux <tag>`) and for a version newer than its own. A missing or wrong signature stops the update. The Linux `.deb` update checks the hash again as root before it installs. Then the app compares the SHA-256 of the download with `SHA256SUMS.txt`. On macOS it also checks the new app's signature and version before the swap.
- If the signing key leaks, [SECURITY.md](https://github.com/lenhatthanh20/natamux/blob/main/SECURITY.md) has the plan: an advisory, a new key, and a manual reinstall for users. A second key kept offline is planned to avoid the reinstall.
- Versions up to v0.1.23 do not check the signature, so update to a newer version. Until the app is signed with a certificate, a manual download is checked as in [Verify the download](/docs/getting-started/install/#verify-the-download).

## Hooks

For each agent it starts, Natamux adds hook settings that call `natamux hook` or the daemon's local address, with the task id and the token from the terminal's environment. For Copilot and Cursor, the hook file goes into the task's worktree and is kept out of git. For Codex, Natamux passes `--dangerously-bypass-hook-trust`. See [Agents and hooks](/docs/concepts/agents-and-hooks/).

## Folder trust

In an approved repo, Natamux marks each task's worktree as trusted for the agent it starts, so the agent does not ask and the repo's own agent config (`.codex`, `.cursor`, `.claude`, MCP servers) runs. For two agents this writes files outside the worktree:

- Cursor CLI: `~/.cursor/projects/<folder>/.workspace-trusted` (under `$CURSOR_DATA_DIR` when it is set).
- Copilot CLI: `~/.copilot/config.json`, or `$COPILOT_HOME/config.json` when it is set. Natamux adds the worktree to its `trustedFolders`.

Natamux removes these entries when it removes the worktree. In an untrusted repo none of this is set. See [Folder trust](/docs/concepts/agents-and-hooks/#folder-trust).

## Permission prompts

In an approved repo, by default Natamux starts each agent with a flag that skips most permission prompts. Only Claude Code (`--permission-mode auto`) checks each action. Codex (`--sandbox workspace-write --ask-for-approval never`), Gemini CLI (`--approval-mode yolo`), Copilot CLI (`--allow-all-tools`) and Cursor CLI (`--force`) run commands without asking, in the task's worktree with your user rights. Set `"agents": { "permissionMode": "ask" }` in [`.natamux.json`](/docs/reference/configuration/#agents) to keep the prompts.

## The desktop app

- It is native, with no web view. Links open in your browser, and only `http`, `https` and `mailto` links open, also from a terminal.
- The terminal ignores the clipboard escape code (OSC 52), and the Docs tab loads no images from the web or from files.
- Before it sends its token to the daemon, the app checks a proof from `/health` that the daemon knows the token.

## The desktop app is unsigned

The first releases are not signed with an Apple or Windows certificate. The macOS app has an ad hoc signature only. Download it only from the repository's Releases page.

## Report a vulnerability

Do not open a public issue. Use [private vulnerability reporting](https://github.com/lenhatthanh20/natamux/security/advisories/new) on GitHub (**Security → Report a vulnerability**). Include the version, your platform, the steps to reproduce and what an attacker gains.

You get an answer within 7 days. Only the latest release gets security fixes.

## Known limits

- An agent can read the daemon token file. The agent token stops an agent that follows the API, not one that looks for that file.
- `auto` mode lets agents run commands without asking; `ask` keeps their prompts. Neither is a sandbox.
