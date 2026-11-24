# Privacy

Natamux runs on your machine. It has no account, no server of its own, and no telemetry.

## What Natamux collects

Nothing. Natamux sends no usage data, crash reports or analytics anywhere. Its own code reaches the internet in these ways, all listed below: the update check of the desktop app (only when you turn it on), your `gh` for pull request status, and your `git` and `gh` for GitHub work. Offline mode turns them off.

## What stays on your machine

- Tasks, their history and stage files: in `data/` of the desktop app's user data folder, as Markdown and JSON files, plus a local SQLite index. Only a daemon you start by hand in a repository uses `.natamux/` there.
- Worktrees: in `data/worktrees/`, or the folder you set.
- The daemon token, the update setting, the offline setting, downloaded updates and the logs of the desktop app: in its user data folder (`~/Library/Application Support/Natamux` on macOS, `%APPDATA%\Natamux` on Windows, `~/.config/Natamux` on Linux). `settings.json` there holds the update setting and the offline setting.
- App preferences (theme, layout): in `view.json` in the desktop app's user data folder.
- With isolation on (Linux): `data/sandbox/<task>/` holds the task's launch scripts, a git config with your git name and email, and the private `tmp` folder of its terminals. It goes with the task. `data/sandbox/no-hooks/` is an empty folder for the daemon's own git.

`NATAMUX_INPUT_TRACE=1` makes the daemon write every key typed in a terminal, passwords too, to its log. Leave it unset unless you debug typing. In Strict mode the trace is blocked: the daemon logs only `input trace blocked by Strict mode`, and a trace that was on stops at the next key once Strict mode is on.

These files are readable only by your user on macOS and Linux (`0600` files, `0700` folders). Log lines that Natamux writes mask the password or token of a URL and a token with a known prefix such as `ghp_` (see [SECURITY.md](SECURITY.md#secrets-on-disk)). The raw output of the daemon and of the programs it starts also goes to `daemon.log` and is not masked.

Deleting a task removes all its files in the data folder and its rows in the index. It does not remove what was already pushed to a remote or a CI provider.

You can delete any of these folders at any time. Every path, how private it is and how it goes away is in [docs/data-inventory.md](docs/data-inventory.md).

## Data lifecycle

The `data` key of `security.json` (saved through `PUT /security`):

- `retentionDays` (default `0`, off): finished tasks older than N days are deleted at daemon start and every 24 hours, through the normal delete. A task with uncommitted changes, or with commits only its branch holds, is kept. Both the worktree's `HEAD` and the task's branch are checked, and Natamux's own `natamux-ci/*` copies on the remote do not count as pushed.
- `logMaxBytes` (default 1 MB): the size cap of the audit, app, daemon and update logs.
- `auditLog` (on in Strict mode, off otherwise): `logs/audit.log` in the data folder gets one line per blocked action or security change (a full file moves to `logs/audit.log.1` and a new one starts), with only a time, the event, a task id or host, a result word and a hash of the settings. No titles, prompts, reasons or URLs.

`natamux support-bundle` saves a file for a bug report with versions, the security state, task counts and the last 200 daemon log lines (secrets masked, typed input left out; only for the app's own daemon, not one you start by hand): no task text. `natamux purge-data --yes` removes the task data, logs and token from this machine and keeps the worktrees; it refuses while a worktree has uncommitted work, and for a data folder that is not an absolute path or holds no `index.sqlite`.

## When data leaves your machine

When you or an agent you start makes it happen, and for the automatic items below:

- **Coding agents.** Every coding agent you start (for example Claude Code, Codex, Gemini CLI, Copilot CLI and Cursor) sends your prompts and code to its own company, under those companies' terms. Natamux starts them; it does not see or change what they send.
- **GitHub.** Pushing a branch, opening a pull request, importing issues, or a GitHub issue as task context (`gh issue view`) uses your own `git` and `gh` login to talk to GitHub. These commands and pushes use `gh auth git-credential` as the credential helper for github.com. Some run by themselves: `git fetch` of the base branch when a workspace is made, `git ls-remote` when the New workspace dialog lists remote branches, `git pull --ff-only` before a stage that follows a merged subtask, and `gh issue create` when `tracker.createIssue` is `on-spec-approved` or `on-pr` (default `manual`).
- **CI check.** Only for a task with **Check on CI** ticked (off by default) on a stage with `"ci": true`. Each CI round runs `git push --force` of a snapshot commit to `natamux-ci/<branch>` on `origin`. The snapshot holds the last commit plus changes to tracked files, also uncommitted ones, never untracked files. Natamux polls `gh run list` every 15 seconds for up to 30 minutes, and when the task ends or is deleted it deletes the `natamux-ci/` branch of every branch a round pushed. You do not click for each round.
- **Pull request status.** While a dashboard or the app is connected and a task has an open or draft pull request or a pushed branch, the daemon runs `gh api graphql` every 60 seconds while checks are pending or a pushed branch has no pull request, and every 5 minutes otherwise. It also runs `gh pr list` and `gh pr view` when an agent stops. It sends what `gh` sends for that: your GitHub login and the names of the repository and the branch.
- **Agents after a restart.** When the daemon starts, it starts again the agents of tasks in running, idle, needs-input or review state.
- **Links.** Links in the dashboard open in your browser.
- **Update check (desktop app).** Off by default. When you tick **Check for Updates Automatically** (app menu on macOS, **Help** menu on Linux and Windows, or **Settings → General → Updates**), the app checks about 30 seconds after it starts, and every 6 hours. The app starts the system `curl` (found through `PATH`; it reads the proxy variables, or the proxy of the network settings, but not `~/.curlrc`) with a HEAD request to `https://github.com/lenhatthanh20/natamux/releases/...` and follows redirects to GitHub's file hosts. The request has no data about you, your repositories or your tasks. GitHub sees your IP address and the app's version. When a newer release exists, the app also fetches `SHA256SUMS.txt`, `SHA256SUMS.txt.minisig`, then the file. Untick it to turn the timed checks off. **Check for Updates…** checks once, when you choose it.

## Privacy policies of other services

Natamux itself talks only to GitHub. The coding agents are separate programs that you install and sign in to, and they follow their own companies' policies.

| Service | Used for | Policy |
|---|---|---|
| GitHub | Releases, update check, `git` and `gh` | [GitHub Privacy Statement](https://docs.github.com/site-policy/privacy-policies/github-general-privacy-statement) |
| Anthropic | Claude Code | [Privacy Policy](https://www.anthropic.com/legal/privacy) |
| OpenAI | Codex | [Privacy Policy](https://openai.com/policies/privacy-policy/) |
| Google | Gemini CLI | [Privacy Policy](https://policies.google.com/privacy) |
| GitHub | Copilot CLI | [GitHub Privacy Statement](https://docs.github.com/site-policy/privacy-policies/github-general-privacy-statement) |
| Cursor | Cursor CLI | [Privacy Policy](https://cursor.com/privacy) |

For other agents, see the agent's own website.

## Offline mode

Off by default. It is on when `NATAMUX_OFFLINE` is exactly `1`, or `"offline": true` is in `settings.json` of the desktop app's user data folder, or in the settings file of the machine (`/Library/Application Support/Natamux/settings.json` on macOS, `/etc/natamux/settings.json` on Linux, `%ProgramData%\Natamux\settings.json` on Windows), which an administrator can set for every user. It is read once at start: restart to change it. Your own `settings.json` reaches the daemon only through the app that starts it; the machine file and the variable are read by every program. Natamux then makes no update check (the manual one is refused too), runs no `gh`, runs no `git fetch`, `pull`, `push`, `ls-remote` or `clone`, runs no pull request watcher, and does not resume the CI gate. Its git also fetches no missing object of a partial clone and downloads no LFS file. A CI stage started offline ends with an error and waits for you. It does not limit the coding agents or what you type in a terminal. Egress `loopback` in the network settings (below) does the same, and is read at each call. The full list is in [For a security review](https://natamux.github.io/docs/help/security-review/).

## Network settings

The `network` key of `security.json` in the daemon's data folder narrows where data can go: egress `open`, `allowlist` or `loopback`, allowed git remotes and `gh` hosts, the updater, a proxy, and whether task programs keep your git and GitHub credentials. The keys and rules are in [SECURITY.md](SECURITY.md#network-and-credentials).

| Path | Enforced by | Evidence |
| --- | --- | --- |
| Daemon git (fetch, pull, push, CI push, remote branch list) | Natamux: URL resolved, checked, then pinned | `crates/natamux-git/tests/egress.rs` |
| `gh` (pull requests, issues, CI runs, PR status) | Natamux: call kind and host checked, `GH_HOST`/`GH_REPO` pinned | `crates/natamux-git/src/gh_egress_tests.rs` |
| Update check and download | Natamux: host checked, each redirect checked | `crates/natamux-update/src/transport_tests.rs` |
| Credentials in terminals, checks, commit agent | Natamux: removed from the environment (`strip`) | `crates/natamux-core/src/child_env_tests.rs`, agent and check tests |
| Agent and terminal traffic (model API, tools) | The network (firewall, proxy); Natamux only with isolation and Landlock ABI 4 (TCP ports) | `crates/natamux-pty/tests/sandbox_linux.rs` |
| Coding agents' own uploads to their company | The agent and the network, not Natamux | none |

## The website

The Natamux website on GitHub Pages is static. It sets no cookies and has no analytics. GitHub may keep its own server logs, under the [GitHub Privacy Statement](https://docs.github.com/site-policy/privacy-policies/github-general-privacy-statement).

## Questions

Open an issue, or for anything sensitive use the private report described in [SECURITY.md](SECURITY.md).
