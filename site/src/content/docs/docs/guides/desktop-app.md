---
title: Desktop app
description: What the Natamux desktop app starts, where it keeps its data and logs, and what happens when you quit.
sidebar:
  order: 8
---

The desktop app is a window around the dashboard. It holds no task logic of its own. It starts the daemon, signs in for you and shows the dashboard. For installation, see [Install Natamux](/docs/getting-started/install/).

## What the app does at start

The window opens at once and shows a spinner in its center until the dashboard is ready.

1. Starts the daemon on port 4317, or on another free port when 4317 is taken. The daemon listens on `127.0.0.1` only.
2. Passes the daemon the PATH of your login shell, so agents installed with Homebrew, nvm and similar tools are found even when you start the app from the Dock. The shell can take seconds right after you log in, so the app reads it while the daemon starts. Until the PATH arrives, the dashboard shows your tasks, but changes, branches, terminals and agents wait.
3. Puts the `natamux` and `nmx` commands first on that PATH, for the terminals of your tasks.
4. Opens the dashboard at `http://127.0.0.1:<port>/`, already signed in. On the first launch it has no project; add one from the menu or the sidebar.

Only one copy of the app runs at a time. Opening it again focuses the existing window.

## Menu

| Item | What it does |
|---|---|
| **Settings…** (`Cmd/Ctrl+,`) | Opens the [Settings](/docs/guides/dashboard/#settings) dialog of the dashboard. On the app menu on macOS, the **File** menu on Windows and Linux |
| **File → Add Project…** (`Cmd/Ctrl+Shift+O`) | Pick a git repository and add it as a project. No restart |
| **File → Show Daemon Log** | Opens the daemon log file |
| **File → Keep Computer Awake While Agents Run** | On by default. While an agent runs, the computer does not go to sleep on its own. The display can still turn off, and closing a laptop lid still puts it to sleep |
| **Check for Updates…** | Looks for a new version now. On the app menu on macOS, the **Help** menu on Windows and Linux |
| **Check for Updates Automatically** | Off by default. Tick it to check every 6 hours. Next to the item above |

The app also has the standard Edit, View and Window menus.

The app icon shows how many tasks need you: in the Dock on macOS, in the launcher on Linux. Windows shows no count.

Links to other sites open in your browser. Only `http`, `https` and `mailto` links open at all.

## Updates

The app updates itself, the way other desktop apps do. It looks for a new version when you choose **Check for Updates…**. Timed checks are off by default; tick **Check for Updates Automatically** to turn them on.

1. You choose **Check for Updates…**, or, with timed checks on, about 30 seconds after the app starts and then every 6 hours, the app asks GitHub for the latest release.
2. When there is a newer version, it downloads it in the background. The menu item shows **Downloading Update 0.1.5…**.
   After **Check for Updates…**, a small popup shows **Checking for updates…** and then the download progress. **Hide** or `Esc` closes it; the update goes on.
3. It checks the signature of `SHA256SUMS.txt` of the release, then the download against `SHA256SUMS.txt`. On macOS it also checks the new app's signature and version. A release without a valid signature is not installed.
4. A dialog says **Natamux 0.1.5 is ready** with **Restart Now**, **Later** and **What's New**. The menu item becomes **Restart to Update to 0.1.5**.
5. **Restart Now** asks first when agents are working, as quitting does. Then the app installs the new version and opens again. Tasks, worktrees and settings stay.

How the new version goes in depends on how you installed the app:

| Installed as | Update |
|---|---|
| macOS app in Applications, or any folder you can write | Swapped in when the app quits. If you just quit while an update is ready, the next start is the new version |
| AppImage | The file is replaced as soon as the download is checked. **Restart Now** starts the new one |
| `.deb` | **Restart Now** installs it with `pkexec dpkg -i`, which asks for your password. If that fails, the app shows the `sudo apt install` command to run |
| Windows installer | The app tells you about the new version: the menu item becomes **Download Natamux 0.1.5…** and opens the release page. Run the new `Natamux-win-x64.exe`; it replaces the old version and keeps your data |
| `.tar.gz`, or a macOS app run from the disk image | The app tells you about the new version and opens the release page. Download it yourself |

Releases from 0.1.18 have files for every system again, so an Intel Mac or Linux install of an older version, such as 0.1.2, updates itself again.

When a check fails, the app tries again at the next check. **Check for Updates…** shows the error. Every step is written to `logs/update.log`.

The check sends no data about you or your tasks. See [Security and privacy](/docs/help/security-and-privacy/).

## Where things are

The app keeps its files in the user data folder.

| System | Folder |
|---|---|
| macOS | `~/Library/Application Support/Natamux` |
| Windows | `%APPDATA%\Natamux` |
| Linux | `~/.config/Natamux` |

| Path in that folder | Holds |
|---|---|
| `settings.json` | Whether timed update checks are on. An older version also kept its repository here |
| `daemon-token` | The daemon token, readable only by you. It stays the same between runs |
| `data/` | Tasks, the task index and worktrees (`data/tasks`, `data/worktrees`) |
| `logs/daemon.log` | The daemon's output |
| `logs/update.log` | Update checks, downloads and installs |
| `updates/` | The downloaded update, until it is installed |
| `bin/` | The `natamux` and `nmx` commands |

To use the commands in your own shell, see [Use the natamux command in your own shell](/docs/getting-started/install/#use-the-natamux-command-in-your-own-shell).

## Logs

Choose **File → Show Daemon Log**, or open `logs/daemon.log` yourself. Each start adds a line with the time and the port. If the daemon stops, the app shows a dialog with the last lines and the log path.

## Quit and close

- **Quit** stops the daemon, and with it every agent and terminal. If agents are working, the app asks first ("An agent is still working" or "N agents are still working"). Tasks, worktrees and their changes stay. When you open the app again, the agents of running, waiting and in-review tasks start again, and the other terminals come back when you open their task. See [A terminal after a daemon restart](/docs/guides/terminal/#a-terminal-after-a-daemon-restart).
- **On macOS, closing the window** keeps the app, the daemon and the agents running. Click the Dock icon to open the window again. Quit from the app menu to stop everything.
- **On Windows and Linux, closing the window** quits the app, with the same question when agents work.

## Environment variables you may need

| Variable | Use |
|---|---|
| `NATAMUX_DESKTOP_SMOKE=1` | Loads the dashboard, makes a task with a worktree, runs a command in its terminal, and quits. Exit code 0 means all of it worked. The task, its worktree and its branch stay in the project. For checks, not daily use |
| `NATAMUX_DESKTOP_SMOKE_SHOT=<file.png>` | With the smoke check, also saves a picture of the window |
| `NATAMUX_<AGENT>_COMMAND` | The program that starts an agent, for example `NATAMUX_CLAUDE_COMMAND`. Set it in the environment the app starts from. See [Environment variables](/docs/reference/environment-variables/) |

## Limits

- The apps are not signed or notarized yet. See [Install Natamux](/docs/getting-started/install/).
- The update check reads only the latest release. Pre-release versions are never offered.
