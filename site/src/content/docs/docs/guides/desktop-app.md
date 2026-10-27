---
title: Desktop app
description: What the Natamux desktop app starts, where it keeps its data and logs, and what happens when you quit.
sidebar:
  order: 8
---

The Rust desktop app starts a Rust daemon beside it, connects to it, and shows the dashboard. For installation, see [Install Natamux](/docs/getting-started/install/).

## What the app does at start

1. Starts the sibling `natamux-daemon` on port 4317, or another free port when 4317 is taken. It listens on `127.0.0.1` only.
2. Sends the PATH of your login shell to the daemon through a private pipe. If the login shell fails or takes too long, it uses the app's PATH.
3. Waits for the daemon's health check, then opens the window. The dashboard is also available at `http://127.0.0.1:<port>/`; enter the token from `daemon-token` when the browser asks.
4. Ships the Rust `natamux` and `nmx` commands beside the app and daemon. A command in a task terminal connects to this daemon.

If the daemon stops unexpectedly, the app shows an error and offers to restart the app.

## Menu

| Item | What it does |
|---|---|
| **Settings…** (`Cmd/Ctrl+,`) | Opens Settings, including the automatic update switch |
| **Check for Updates…** | Looks for a new version now. On the app menu on macOS, the **Help** menu on Windows and Linux |
| **Check for Updates Automatically** | Off by default. Tick it to check every 6 hours. Next to the item above |

The app also has View, Task and Tools menus. Links to release pages open in your browser.

## Notifications and the Dock

The Dock icon shows how many tasks need you. With **Notify me** on, a task that needs you or is done while the app is in the back shows a macOS notification with a sound. See [Notifications in the desktop app](/docs/guides/dashboard/#notifications-in-the-desktop-app).

## Updates

The app updates itself, the way other desktop apps do. It looks for a new version when you choose **Check for Updates…**. Timed checks are off by default; tick **Check for Updates Automatically** to turn them on.

1. You choose **Check for Updates…**, or, with timed checks on, about 30 seconds after the app starts and then every 6 hours, the app asks GitHub for the latest release.
2. The app checks the signed release and downloads the right file for an install it can update itself. A manual check shows a short status message while it runs.
3. It checks the signature of `SHA256SUMS.txt` of the release, then the download against `SHA256SUMS.txt`. On macOS it also checks the new app's signature and version. A release without a valid signature is not installed.
4. A dialog says the update is ready and offers **Restart Now** or **Later**. A separate **What's New** link opens the release page. The menu has **Restart to Update** while the downloaded update is ready.
5. **Restart Now** asks first if an editor has unsaved drafts. Then the app installs the new version and opens again. Tasks, worktrees and settings stay.

How the new version goes in depends on how you installed the app:

| Installed as | Update |
|---|---|
| macOS app in Applications, or any folder you can write | Swapped in after you choose **Restart Now**. If the swap fails, the old app is restored |
| AppImage | **Restart Now** replaces the file and starts the new one |
| `.deb` | **Restart Now** installs it with `pkexec dpkg -i`, which asks for your password. If that fails, the app shows the `sudo apt install` command to run |
| Windows installer | The app offers the release page. Run the new `Natamux-win-x64.exe`; it replaces the old version and keeps your data |
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
| `daemon-port` | The running daemon's loopback port, for the Rust CLI |
| `data/` | Tasks, the task index and worktrees (`data/tasks`, `data/worktrees`) |
| `logs/daemon.log` | The daemon's output |
| `logs/update.log` | Update checks, downloads and installs |
| `updates/` | The downloaded update, until it is installed |
| app folder | The `natamux` and `nmx` commands ship beside `natamux-daemon` |

To use the commands in your own shell, see [Use the natamux command in your own shell](/docs/getting-started/install/#use-the-natamux-command-in-your-own-shell).

## Logs

Open `logs/daemon.log` to read the daemon's output. If it stops, the app shows an error with a restart action.

## Quit and close

- **Quit** stops the daemon, and with it every agent and terminal. The app asks before dropping unsaved editor drafts. Tasks, worktrees and their changes stay.
- **Closing the window** quits the app on macOS, Windows and Linux.

## Environment variables you may need

| Variable | Use |
|---|---|
| `--smoke` | Starts the app and daemon, checks the window and quits. For CI, not daily use |
| `NATAMUX_<AGENT>_COMMAND` | The program that starts an agent, for example `NATAMUX_CLAUDE_COMMAND`. Set it in the environment the app starts from. See [Environment variables](/docs/reference/environment-variables/) |

## Limits

- The apps are not signed or notarized yet. See [Install Natamux](/docs/getting-started/install/).
- The update check reads only the latest release. Pre-release versions are never offered.
