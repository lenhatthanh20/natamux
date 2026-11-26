# Data inventory

Every place Natamux keeps data on your machine, how private it is, and how it goes away. For a company's security review and for anyone who wants to remove Natamux fully.

- **App folder**: `~/Library/Application Support/Natamux` (macOS), `%LOCALAPPDATA%\Natamux` (Windows, a new install; kept out of the roaming profile), `~/.config/Natamux` (Linux, or `$XDG_CONFIG_HOME/Natamux`). A Windows install that already has `%APPDATA%\Natamux` and no `%LOCALAPPDATA%\Natamux\data` keeps using `%APPDATA%\Natamux`. Natamux never moves it by itself, because git worktrees and task files store absolute paths. To move it: quit Natamux, move the folder to `%LOCALAPPDATA%\Natamux`, then run `git worktree repair` in each project.
- **Data folder**: `data/` in the app folder. A daemon you start by hand uses `NATAMUX_DATA_DIR`, or `.natamux/` in the folder it runs in.
- Modes `0700` (folders) and `0600` (files) are set on macOS and Linux. On Windows the files keep the folder's default permissions.

## Data folder

| Path | Content | Sensitivity | Mode | Writer | Retention | Removed on task delete | Removed by `purge-data` |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `tasks/<task>.md` | Task title, notes (the prompt), stage state | High: your prompts | 0600 | daemon | Until the task is deleted, or `data.retentionDays` | yes | yes |
| `tasks/<task>.events.jsonl` | Event log: stage changes, git results, notices | High: may quote prompts and paths | 0600 | daemon | as the task | yes | yes |
| `tasks/<task>.screens/` | Saved terminal screens: what the terminal showed, also typed text it echoed | High: may hold anything you typed or printed | 0600 | daemon | as the task | yes | yes |
| `tasks/<task>.attachments/`, `.shells.json`, `.usage.json`, archive | Attachments, open shells, token use, finished stage files | High | 0600 | daemon | as the task | yes | yes |
| `tasks/<task>.deleting` | Marker of a delete in progress (empty) | None | 0600 | daemon | Until the delete ends | yes | yes |
| `index.sqlite`, `-wal`, `-shm` | Search index of the tasks: titles, notes, states | High | 0600 | daemon | Rows go with the task (`secure_delete` on, WAL emptied) | rows | yes |
| `logs/audit.log`, `logs/audit.log.1` | One JSON line per blocked action or security change: `ts`, `event`, `id`, `result`, `settingsVersion` only | Low: task ids and hosts | 0600 | daemon | Past `data.logMaxBytes` the file moves to `audit.log.1` (the older one is replaced) and a new file starts: up to twice the cap on disk | no (ids only) | yes |
| `agent-launch-<task>*.sh`, `stage-command-<task>.sh` | Scripts that start an agent or a check: prompt, model, settings. The first prompt is also on the agent's command line (see below) | High | 0600 | daemon | as the task | yes | yes |
| `sandbox/<task>/` | Isolation only: launch scripts, git config with your name and email, `tmp` | Medium | 0700 | daemon | as the task | yes | yes |
| `security.json`, `security-written.json` | Security settings and the daemon's copy of its last save | Low | 0600 | daemon (`PUT /security`) | Until you change it | no | no (settings) |
| `repos.json`, `custom-agents.json`, templates | Projects (folder paths), your agents, saved templates | Low | 0600 | daemon | Until you change it | no | no (settings) |
| `claude-code-settings.json`, `gemini-settings.json` | Hook settings Natamux gives the agents | None | 0600 | daemon | Rewritten at each start | no | no |
| `worktrees/<task>/` | Git worktrees: your code | Your code | as git makes them | git | Until the worktree is removed | only clean ones | **never** |

## App folder

| Path | Content | Sensitivity | Mode | Writer | Retention | Removed by `purge-data` |
| --- | --- | --- | --- | --- | --- | --- |
| `daemon-token` | Token of the app's daemon | High: full API access | 0600 | app | Until removed | yes |
| `daemon-port` | Port of the running daemon | None | 0600 | app | Until removed | yes |
| `logs/app.log`, `logs/daemon.log`, `logs/update.log` | Link changes, daemon output, update steps. Lines Natamux writes (git errors, HTTP errors, the pull request watcher, app and update lines) have URLs and known tokens masked; the raw output of the daemon and of programs it starts in `daemon.log` is not masked | Medium: paths, task ids, error text | 0600 | app, daemon | Cut to `data.logMaxBytes` at each app start | yes |
| `settings.json`, `view.json` | Update and offline settings, theme and layout | None | 0600 | app | Until you change it | no |
| Settings file of the machine (outside the app folder: `/Library/Application Support/Natamux/settings.json`, `/etc/natamux/settings.json`, `C:\ProgramData\Natamux\settings.json`) | Keys `offline`, `strict`, `updates` that an administrator sets for every user | None | set by the administrator | administrator | Until the administrator changes it | no |
| `updates/` | Downloaded update files | None | 0700 | app | Until the next update | no |

The first prompt is also on the agent's command line while the agent runs. Mode `0600` on the launch script does not hide it: on a shared Linux host, other local accounts can read a command line in `/proc` unless `/proc` is mounted with `hidepid=2`.

`NATAMUX_INPUT_TRACE=1` writes every typed key to `logs/daemon.log`, passwords too. Strict mode blocks it. The support bundle leaves those lines out.

## Where test markers may appear

The privacy test (`crates/natamux-daemon/tests/routes/privacy.rs`) writes a prompt, typed terminal input and a fake token in a git remote URL, then scans the data folder (not the worktrees). Each marker may appear only under these paths; the test reads this list.

```text
marker prompt: tasks/ index.sqlite
marker typed-input: tasks/
marker remote-token:
```

A remote URL with a token never reaches the data folder: git errors and log lines are masked first.

## Settings for data (`data` in `security.json`)

| Key | Default | What it does |
| --- | --- | --- |
| `retentionDays` | `0` (off) | At daemon start and every 24 h, Done and Cancelled tasks not changed for more than N days are deleted through the normal delete. A task with uncommitted changes, or with commits no other branch, remote branch or tag holds, is kept and logged `refused`. The check covers the worktree's `HEAD` and the task's branch, and Natamux's own `origin/natamux-ci/*` refs do not count as a remote branch. Only settings saved through `PUT /security` delete anything: a hand-edited file never starts a delete. |
| `logMaxBytes` | `1048576` | Size cap of the audit, app, daemon and update logs (at least 4096). The audit log keeps one older file of the same size. |
| `auditLog` | on in Strict mode, else off | Writes `logs/audit.log`, one line for every blocked agent action, also one a route refuses after the route check. In Strict mode it cannot be turned off. |

Audit events: `security.changed`, `security.outside-change`, `ci.blocked`, `agent.blocked`, `egress.denied`, `agent-api.denied`, `task.delete`, `task.purge` (a delete finished at start), `retention.run`, `support-bundle.create`. `settingsVersion` is the first 16 hex digits of the SHA-256 of `security.json`. An id or result that is not a plain word (letters, digits, `._:-/@`, up to 80 characters) is written as `-`, so a title or a reason never reaches the log.

## Draft commit message

**Draft commit message** (a click, never automatic) runs the task's agent once with the task title, the list of changed files and up to 60,000 characters of the uncommitted diff. The text goes to the agent's company like any other prompt. It never runs in a repo that is not trusted.
## Support bundle

`natamux support-bundle [--out <file>]` (or `GET /support-bundle`, desktop token only; an agent token gets 403) saves a file with mode 0600 holding: the daemon version, OS and CPU type, uptime, the security state and its keys, isolation and egress, the data settings, the number of tasks in each state, and the last 200 lines of `logs/daemon.log` with URLs and known tokens masked and input-trace lines left out. The log lines are there only when the folder above the data folder holds the app's `daemon-token` file: a daemon you start by hand gives none. It holds no task text, events or environment. Read it before you send it: other free text in log lines (paths, error messages) stays.

## Removing Natamux

1. Push or save the work in your worktrees.
2. Quit the app.
3. Run `natamux purge-data --yes`. It refuses while a worktree in `data/worktrees/` has uncommitted or untracked files (and lists them), while the daemon runs (on the app's saved port, or on `NATAMUX_PORT`, else 4317), in a task's terminal, and for a data folder that is not an absolute path or holds no `index.sqlite`. It removes the task data, the index, the logs (both audit files too), the scripts named `agent-launch-<task>[-N].sh` or `stage-command-<task>.sh` and the sandbox folders, and the token and port files. It keeps the worktrees and the settings. Running it again changes nothing.
4. Remove what is left:
   - **macOS**: move `Natamux.app` to the Trash, then remove `~/Library/Application Support/Natamux` (this also removes the worktrees and settings).
   - **Linux**: remove the AppImage or the unpacked folder, then `~/.config/Natamux`.
   - **Windows**: uninstall from **Settings → Apps**, then remove `%LOCALAPPDATA%\Natamux` (or `%APPDATA%\Natamux` on an older install, see the app folder above).
5. The `natamux` command: remove the `natamux` and `nmx` links or files from the folder you put them in.

Natamux does not remove data already pushed to a remote or a CI provider (`natamux-ci/*` branches, CI logs), or what coding agents sent to their companies.

## Not kept by Natamux

- **Coding agents' own files**: `~/.claude`, `~/.claude.json`, `~/.codex`, `~/.gemini`, `~/.copilot`, Cursor's folders and the like hold sessions, transcripts and logins. Natamux does not change or remove them; each agent's documentation says how to clear them.
- **`gh` and `git`**: your `gh` login (`~/.config/gh`), git credential helpers and the system keychain.
- **Worktrees**: your code, with the hook files Natamux writes into them (listed in `.git/info/exclude`).

## Backups

Time Machine, Windows File History or another backup tool copies the app folder like any other folder, prompts and screens included. Leave the app folder out of backups that leave your control, or encrypt them. A deleted task stays in older backups.

## Install notes (F7)

- **Check the download**: each release has `SHA256SUMS.txt` and a Minisign signature `SHA256SUMS.txt.minisig`. Check with `minisign -Vm SHA256SUMS.txt -P RWSgtwsXHppEP5n4kgg1Q3yNIYODQNGsMQBKF93tjSRwzL1BrjeGoBpo` (the release key, also in the [install guide](https://natamux.github.io/docs/getting-started/install/)), then compare the file's SHA-256. The app's own updater checks both before it installs.
- **Signing**: the macOS app is signed with a self-made certificate, not an Apple Developer ID, and is not notarized; the Windows installer has no Authenticode signature; the Linux packages are not signed. macOS asks you to allow the app once and Windows SmartScreen warns. Your IT team may need to allow the app by its hash.
- **Pin a version**: keep the release file you checked and install that one; turn off **Check for Updates Automatically** so the app does not offer newer ones.
- **Updates off**: leave the update check off (the default), or set `network.updater: false` in `security.json`, or turn on offline mode. Then the app never contacts GitHub for updates.
