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
  - Tasks, history and stage files: `data/` of the desktop app's user data folder. Only a daemon started by hand in a repository uses `.natamux/` there.
  - Worktrees: `data/worktrees/`, or the folder you set.
  - Desktop app token, update and offline settings (`settings.json`), downloaded updates and logs: its user data folder (`~/Library/Application Support/Natamux` on macOS, `%LOCALAPPDATA%\Natamux` on Windows, or `%APPDATA%\Natamux` on an older install, `~/.config/Natamux` on Linux).
- Data leaves your machine when you or an agent you start makes it happen, and for the automatic items below:
  - Coding agents send your prompts and code to their own companies, under their terms. Natamux does not see or change that.
  - GitHub actions (push, pull request, issue import) use your own `git` and `gh` login, with `gh auth git-credential` as the credential helper for github.com. Some run by themselves: `git fetch` when a workspace is made, `git ls-remote` when the New workspace dialog lists remote branches, `git pull --ff-only` before a stage that follows a merged subtask, and `gh issue create` when `tracker.createIssue` is `on-spec-approved` or `on-pr`.
  - While a dashboard or the app is connected and a task has an open or draft pull request or a pushed branch, the daemon runs `gh api graphql` every 60 seconds (checks pending, or a pushed branch without a pull request) or every 5 minutes, and `gh pr list` and `gh pr view` when an agent stops.
  - For a task with **Check on CI** ticked (off by default), each CI round force-pushes a snapshot commit to `natamux-ci/<branch>` on `origin`: the last commit plus changes to tracked files. Untracked files are never sent. The push goes to one approved URL and is refused when `origin` has more than one push URL. When the task ends or is deleted, Natamux deletes the `natamux-ci/` branch of every branch a round pushed. Strict mode can send only the last commit, ask a person to approve the exact tree, or forbid CI pushes.
  - When you tick **Check for Updates Automatically** (off by default), the desktop app starts the system `curl` to ask `https://github.com/lenhatthanh20/natamux/releases/...` for the latest release about 30 seconds after it starts and every 6 hours, and downloads a new version from GitHub. The request carries no data about you or your tasks; GitHub sees your IP address and the app version. Untick it to stop the timed checks. See [Updates](/docs/guides/desktop-app/#updates).
  - **Draft commit message** (a click) runs the task's agent once with the task title, the list of changed files and up to 60,000 characters of the uncommitted diff. It never runs in a repo that is not trusted.
  - The install scripts connect once per install to `github.com` (and `install.ps1` to `api.github.com`, unless `NATAMUX_VERSION` is set).
  - Natamux installs no crash handler, so a crash dump follows the policy of the operating system.

Every connection, and how to turn them off with offline mode, is in [For a security review](/docs/help/security-review/).

## Agents run with your rights

An agent can read and change files and run commands as your user. Natamux does not sandbox agents. Treat an agent like a person typing in your terminal, and use each agent's own permission settings.

Each task works in its own git worktree. Natamux pushes, merges or deletes a branch only when you click the button or run the command. The exceptions are the automatic git and `gh` commands and the CI snapshot push listed above.

## Strict mode

For work with strict security rules, turn on **Settings > Security > Strict mode**. Agents then start in their ask mode, agent config is approved per worktree and checked right before each start, agent tokens reach only their own task, CI gets only what you allow, and typed terminal input is never traced. The status bar shows **Strict**; its tooltip lists the rules. `natamux security` prints the same.

- Only the app changes the setting. An agent token cannot.
- If `security.json` changes outside Natamux, Natamux keeps the stricter settings and shows a warning until you save them again. This holds while the daemon runs, also when its saved copy is changed or deleted with it; at a restart the saved copy on disk is trusted.
- Strict mode stops mistakes and repo content. It does not stop an agent that looks for the daemon token on purpose: see the [configuration reference](/docs/reference/configuration/#strict-mode-securityjson) and `SECURITY.md`.

## The daemon

- It listens on `127.0.0.1` (loopback) by default. The daemon refuses to start when `NATAMUX_HOST` is not loopback, unless `NATAMUX_ALLOW_REMOTE_BIND` is exactly `1`. The Docker image sets both inside its container, and `compose.prod.yaml` publishes the port on `${NATAMUX_BIND:-127.0.0.1}`, which is loopback unless you change it.
- Every API request needs a token in an `Authorization: Bearer` header. `GET /health` needs no token, and a terminal WebSocket uses a one-time ticket.
- Without `NATAMUX_TOKEN`, the daemon makes a random token for each run and prints it once.
- The token travels in a header, never a cookie, so other websites cannot use your session. `/events` also accepts the token as `?token=` in the address; a reverse proxy can log it. Terminals open with a one-time ticket that expires after 30 seconds.
- The daemon checks the `Host` header against an allow-list (`127.0.0.1`, `localhost`, `::1`, plus a non-wildcard `NATAMUX_HOST` and `NATAMUX_ALLOWED_HOSTS`) and the `Origin` of WebSocket handshakes. A web page on another site cannot reach it, not even by DNS rebinding.
- Do not run the daemon on a network address without TLS. Put a reverse proxy with TLS in front of it.
- Task terminals get an [agent token](/docs/reference/http-api/#the-agent-token), not the daemon's token. It is per task and lives in memory only. An agent cannot approve, skip or pass a stage, change project settings or approve repo commands, and what it does is recorded as the agent's. The `natamux` command in a task terminal uses it too. This stops an agent that follows bad instructions, for example from a web page or an issue. A subtask stays in its parent's project, and an agent cannot fan out a task, send the events only the daemon sends (`stage.checked`, `stage.started`, `stage.reopened`, `pr.merged`) or report a command stage done. Every send-back by an agent counts for `maxSendBacks`. It is not a sandbox: an agent runs as your user, so it can still read the daemon's token file.

## Repo trust

- A repo can carry commands: stage checks, path conditions, command stages and `args` in `.natamux.json`, and the setup scripts `.natamux-setup` and `.natamux-setup.ps1`. They run with your rights. The agent config files in the project folder, such as `.claude/settings.json`, `.mcp.json`, `.codex/config.toml`, `.codex/hooks.json`, `.cursor/mcp.json` and `.gemini/settings.json`, are part of what you approve: they hold hooks and MCP servers that an agent runs. A setup script that is a link never runs.
- Natamux runs none of them until you approve them for that repo, in the app dialog or with `natamux repo trust <repo> --approve <hash>`. A change to any of them needs a new approval. When **Start agent** is refused because a task's worktree has other agent config than what you approved, the error has a **Review config** button: it opens the dialog for that worktree and lists the changed files. Start the agent again after you approve. A repo with no commands, no setup script and no agent config file needs none. After an update, a project that has an agent config file asks for one new approval. Repos added before this check were approved once.
- An untrusted repo gets no repo commands, no setup script, no stage `args`, no auto mode and no folder trust for agents. A start that is refused for this takes back the folder trust Natamux wrote for Cursor and Copilot.
- **Overwrite** in the project settings asks first when the merged text has commands that came only from the changed file. Draft commit message starts no agent in an untrusted repo. `natamux statusline` runs only the `statusLine` command of your user-level settings.
- Known limits: a file that Natamux itself writes into a workspace (for example `.claude/settings.local.json` or `.cursor/hooks.json`) is not part of the approval, also when a repo commits one. The full rules, also for links and for each task's worktree, are in [SECURITY.md](https://github.com/lenhatthanh20/natamux/blob/main/SECURITY.md#repo-trust).
- Approval does not limit what an agent does inside its worktree.

## Secrets on disk

- The app folder and the daemon data folder are private to your user (mode `0700`).
- The desktop app keeps its token in its user data folder, readable only by your user (mode `0600`).
- The scripts that start agents are written with mode `0600`. This does not hide the first prompt: it is also on the agent's command line, which other local accounts on a shared Linux host can read in `/proc` unless `/proc` is mounted with `hidepid=2`.
- Task files, event logs, saved screens, attachments, the SQLite index and the logs are written with mode `0600`. At start the daemon tightens wider files an older version left in the top two levels of the data folder. Worktrees keep their modes.
- Deleting a task removes every file Natamux kept for it in the data folder (task file, events, usage, shells, screens, attachments, launch and stage-command scripts) and its index rows, with SQLite `secure_delete` on. If Natamux stops in the middle, the next start finishes the delete.
- Logs and git errors never show the user name and password of a URL, secret URL query values (`token`, `access_token`, `key`, `password`, `secret`, `sig`), or words that start with `ghp_`, `github_pat_`, `gho_` or `sk-`. Other secrets in free text are not found.
- Modes `0700` and `0600` are set on macOS and Linux only. On Windows the folders keep their default permissions.
- The desktop app is native: it has no web page and no Sign out.

## Updates

- Updates come over HTTPS from the latest GitHub release of the repository.
- `SHA256SUMS.txt` of each release is signed with [minisign](https://jedisct1.github.io/minisign/). The secret key stays offline with the maintainer, never on GitHub, so a stolen GitHub account cannot sign a release. The public key is built into the app:

  ```
  RWSgtwsXHppEP5n4kgg1Q3yNIYODQNGsMQBKF93tjSRwzL1BrjeGoBpo
  ```

- Before it downloads an update, the app checks that signature, and that it was made for that release (`natamux <tag>`) and for a version newer than its own. A missing or wrong signature stops the update. The Linux `.deb` update checks the hash again as root before it installs. Then the app compares the SHA-256 of the download with `SHA256SUMS.txt`. On macOS it also runs `codesign --verify --deep --strict`, which checks that the new app's code signature is intact, not who signed it (the minisign signature proves who published), and its version, before the swap.
- If the signing key leaks, [SECURITY.md](https://github.com/lenhatthanh20/natamux/blob/main/SECURITY.md) has the plan: an advisory, a new key, and a manual reinstall for users. A second key kept offline is planned to avoid the reinstall.
- Versions up to v0.1.23 do not check the signature, so update to a newer version. Until the app is signed with a Developer ID, a manual download is checked as in [Verify the download](/docs/getting-started/install/#verify-the-download).

## Hooks

For each agent it starts, Natamux adds hook settings that call `natamux hook` or the daemon's local address, with the task id and the token from the terminal's environment. For most agents the hook file goes into the task's worktree and is kept out of git; the file of each agent is in the table of [Agents and hooks](/docs/concepts/agents-and-hooks/). For Codex, Natamux passes `--dangerously-bypass-hook-trust`; Codex then also skips its trust review of the session's other hooks. See [Agents and hooks](/docs/concepts/agents-and-hooks/).

## Folder trust

In an approved repo, Natamux marks each task's worktree as trusted for the agent it starts, so the agent does not ask and the repo's own agent config (`.codex`, `.cursor`, `.claude`, MCP servers) runs. For two agents this writes files outside the worktree. Gemini CLI gets `GEMINI_CLI_TRUST_WORKSPACE=true` and Muse Code gets `--trust-workspace`, with no file written:

- Cursor CLI: `~/.cursor/projects/<folder>/.workspace-trusted` (under `$CURSOR_DATA_DIR` when it is set).
- Copilot CLI: `~/.copilot/config.json`, or `$COPILOT_HOME/config.json` when it is set. Natamux adds the worktree to its `trustedFolders`.

Natamux removes these entries when it removes the worktree. In an untrusted repo none of this is set. See [Folder trust](/docs/concepts/agents-and-hooks/#folder-trust).

## Permission prompts

In an approved repo, by default Natamux starts each agent with a flag that skips most permission prompts. Only Claude Code (`--permission-mode auto`) checks each action. The other built-in agents run commands without asking, in the task's worktree with your user rights. Set `"agents": { "permissionMode": "ask" }` in [`.natamux.json`](/docs/reference/configuration/#agents) to keep the prompts. The flag of every agent is listed in the [configuration reference](/docs/reference/configuration/#agents).

## The desktop app

- It is native, with no web view. Links open in your browser, and only `http`, `https` and `mailto` links open, also from a terminal.
- The terminal ignores the clipboard escape code (OSC 52), and the Docs tab loads no images from the web or from files.
- Before it sends its token to the daemon, the app checks a proof from `/health` that the daemon knows the token.

## The desktop app is not notarized

The apps are not signed with an Apple Developer ID or a Windows certificate, and the macOS app is not notarized, so macOS warns on first open. Release macOS apps are signed with a self-made certificate; only dry runs are ad hoc. Windows and Linux builds are not signed. Download them only from the repository's Releases page. The install script removes the macOS quarantine flag. It checks the signature of `SHA256SUMS.txt` only when `minisign` is installed; otherwise it prints a warning and checks the hash alone.

## Report a vulnerability

Do not open a public issue. Use [private vulnerability reporting](https://github.com/lenhatthanh20/natamux/security/advisories/new) on GitHub (**Security → Report a vulnerability**). Include the version, your platform, the steps to reproduce and what an attacker gains.

You get an answer within 7 days. Only the latest release gets security fixes.

## Known limits

- An agent can read the daemon token file. The agent token stops an agent that follows the API, not one that looks for that file.
- `auto` mode lets agents run commands without asking; `ask` keeps their prompts. Neither is a sandbox.
