---
title: Desktop app
description: What the Natamux desktop app starts, where it keeps its data and logs, and what happens when you quit.
sidebar:
  order: 6
---

The desktop app is a window around the dashboard. It holds no task logic of its own. It starts the daemon, signs in for you and shows the dashboard. For installation, see [Install Natamux](/docs/getting-started/install/).

## What the app does at start

1. Starts the daemon on port 4317, or on another free port when 4317 is taken. The daemon listens on `127.0.0.1` only.
2. Passes the daemon the PATH of your login shell, so agents installed with Homebrew, nvm and similar tools are found even when you start the app from the Dock. The shell can take seconds right after you log in, so the app reads it while the daemon starts. Until the PATH arrives, the dashboard shows your tasks, but changes, branches, terminals and agents wait.
3. Puts the `natamux` and `nmx` commands first on that PATH, for the terminals of your tasks.
4. Opens the dashboard at `http://127.0.0.1:<port>/`, already signed in. On the first launch it has no project; add one from the menu or the sidebar.

Only one copy of the app runs at a time. Opening it again focuses the existing window.

## Menu

| Item | What it does |
|---|---|
| **File → Add Project…** (`Cmd/Ctrl+Shift+O`) | Pick a git repository and add it as a project. No restart |
| **File → Show Daemon Log** | Opens the daemon log file |
| **Check for Updates…** | Looks for a new version now. On the app menu on macOS, the **Help** menu on Windows and Linux |
| **Check for Updates Automatically** | Turns the timed update checks off and on. Next to the item above |

The app also has the standard Edit, View and Window menus.

Links to other sites open in your browser. Only `http`, `https` and `mailto` links open at all.

## Updates

The app updates itself, the way other desktop apps do.

1. About 30 seconds after it starts, and then every 6 hours, the app asks GitHub for the latest release.
2. When there is a newer version, it downloads it in the background. The menu item shows **Downloading Update 0.1.5…**.
3. It checks the download against `SHA256SUMS.txt` of the release. On macOS it also checks the new app's signature and version.
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

Intel Mac and Linux installs of 0.1.2 update themselves again from the first release that has their files.

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

- **Quit** stops the daemon, and with it every agent and shell. If agents are working, the app asks first ("An agent is still working" or "N agents are still working"). Tasks, worktrees and their changes stay. When you open the app again, the agents of running, waiting and in-review tasks start again.
- **On macOS, closing the window** keeps the app, the daemon and the agents running. Click the Dock icon to open the window again. Quit from the app menu to stop everything.
- **On Windows and Linux, closing the window** quits the app, with the same question when agents work.

## Environment variables you may need

| Variable | Use |
|---|---|
| `NATAMUX_DESKTOP_SMOKE=1` | Loads the dashboard, makes a task with a worktree, runs a command in its shell, and quits. Exit code 0 means all of it worked. The task, its worktree and its branch stay in the project. For checks, not daily use |
| `NATAMUX_DESKTOP_SMOKE_SHOT=<file.png>` | With the smoke check, also saves a picture of the window |
| `NATAMUX_<AGENT>_COMMAND` | The program that starts an agent, for example `NATAMUX_CLAUDE_COMMAND`. Set it in the environment the app starts from. See [Environment variables](/docs/reference/environment-variables/) |

## Limits

- The apps are not signed or notarized yet. See [Install Natamux](/docs/getting-started/install/).
- The update check reads only the latest release. Pre-release versions are never offered.
