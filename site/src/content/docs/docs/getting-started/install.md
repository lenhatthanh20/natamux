---
title: Install Natamux
description: Download the desktop app for macOS, Windows or Linux, or run Natamux from source or in Docker.
sidebar:
  order: 1
---

Natamux is a desktop app for macOS (Apple silicon and Intel), Windows and Linux, all on 64-bit. The macOS and Linux apps [update themselves](/docs/guides/desktop-app/#updates); on Windows the app tells you about a new version and you run the new installer.

## Requirements

- **git.** Natamux makes a git worktree for each task. On Windows, install [Git for Windows](https://git-scm.com/download/win).
- **Windows only:** Windows 10 version 1809 or later, or Windows 11, on x64. Windows on Arm may run the x64 app, but it is not tested.
- **At least one coding agent**, installed and logged in: `claude`, `codex`, `gemini`, `copilot` or `cursor-agent`. See [Agents](/docs/agents/claude-code/).
- **`gh` (GitHub CLI)**, optional. Needed for opening pull requests, importing issues and CI status. Run `gh auth login` once.

## Download

Get the file for your system from the [latest release](https://github.com/lenhatthanh20/natamux/releases/latest).

<div class="downloads">
<a href="https://github.com/lenhatthanh20/natamux/releases/latest/download/Natamux-mac-arm64.dmg"><strong>macOS, Apple silicon</strong><span class="file">Natamux-mac-arm64.dmg</span></a>
<a href="https://github.com/lenhatthanh20/natamux/releases/latest/download/Natamux-mac-x64.dmg"><strong>macOS, Intel</strong><span class="file">Natamux-mac-x64.dmg</span></a>
<a href="https://github.com/lenhatthanh20/natamux/releases/latest/download/Natamux-win-x64.exe"><strong>Windows 10 and 11</strong><span class="file">Natamux-win-x64.exe</span></a>
<a href="https://github.com/lenhatthanh20/natamux/releases/latest/download/Natamux-linux-x86_64.AppImage"><strong>Linux</strong><span class="file">Natamux-linux-x86_64.AppImage</span></a>
</div>

Every file:

| System | File |
|---|---|
| macOS, Apple silicon (M1 and later) | [`Natamux-mac-arm64.dmg`](https://github.com/lenhatthanh20/natamux/releases/latest/download/Natamux-mac-arm64.dmg) |
| macOS, Intel | [`Natamux-mac-x64.dmg`](https://github.com/lenhatthanh20/natamux/releases/latest/download/Natamux-mac-x64.dmg) |
| Windows 10 and 11, x64 | [`Natamux-win-x64.exe`](https://github.com/lenhatthanh20/natamux/releases/latest/download/Natamux-win-x64.exe) |
| Linux, any distribution | [`Natamux-linux-x86_64.AppImage`](https://github.com/lenhatthanh20/natamux/releases/latest/download/Natamux-linux-x86_64.AppImage) |
| Debian, Ubuntu | [`Natamux-linux-amd64.deb`](https://github.com/lenhatthanh20/natamux/releases/latest/download/Natamux-linux-amd64.deb) |
| Linux, no install | [`Natamux-linux-x64.tar.gz`](https://github.com/lenhatthanh20/natamux/releases/latest/download/Natamux-linux-x64.tar.gz) |

Not sure which Mac you have? Open **Apple menu → About This Mac**. **Chip: Apple M…** means Apple silicon; **Processor: Intel** means Intel. You can also build the app from source with `scripts/package-desktop.sh`; see `packages/desktop/README.md`.

:::caution
The apps are **not signed** with an Apple or Windows certificate yet. Download them only from the Natamux releases page, and follow the first-open steps for your system below.
:::

## Install on macOS

1. Open the `.dmg` and drag **Natamux** to **Applications**.
2. Open Natamux. macOS blocks it the first time.
3. Open **System Settings → Privacy & Security** and click **Open Anyway**.
4. If there is no such button, remove the quarantine flag and open the app again:

   ```sh
   xattr -dr com.apple.quarantine /Applications/Natamux.app
   ```

## Install on Windows

1. Run `Natamux-win-x64.exe`. Windows SmartScreen may show **Windows protected your PC**, because the installer is not signed.
2. Click **More info**, then **Run anyway**.
3. The installer needs no administrator rights. It installs Natamux for your user only, adds it to the Start menu and the desktop, and opens it.

Terminals of your tasks run **PowerShell**. A project's setup script on Windows is `.natamux-setup.ps1`; see [Workspaces](/docs/concepts/workspaces/). Install your agents so that PowerShell finds them: open a new PowerShell window and run `claude --version` (or `codex`, `gemini`, `copilot`, `cursor-agent`).

## Install on Linux

### AppImage

Make the file runnable, then start it:

```sh
chmod +x Natamux-linux-x86_64.AppImage
./Natamux-linux-x86_64.AppImage
```

On Ubuntu 24.04 and later, the system can block Chromium's sandbox, and the app does not start. Either install the `.deb` instead, or start the AppImage with `--no-sandbox`:

```sh
./Natamux-linux-x86_64.AppImage --no-sandbox
```

### Debian and Ubuntu (.deb)

```sh
sudo apt install ./Natamux-linux-amd64.deb
```

The package installs the app as `natamux-desktop`, so the name `natamux` stays free for the command line.

### tar.gz

Unpack `Natamux-linux-x64.tar.gz` anywhere and run the app from the folder. The same sandbox note as for the AppImage applies.

## Verify the download

Download `SHA256SUMS.txt` from the same release, put it next to your file and check it.

```sh
# macOS
shasum -a 256 -c SHA256SUMS.txt --ignore-missing

# Linux
sha256sum -c SHA256SUMS.txt --ignore-missing
```

On Windows, in PowerShell, compare the hash with the line for `Natamux-win-x64.exe` in `SHA256SUMS.txt`:

```powershell
(Get-FileHash .\Natamux-win-x64.exe -Algorithm SHA256).Hash.ToLower()
Select-String Natamux-win-x64.exe .\SHA256SUMS.txt
```

On macOS and Linux you should see `OK` for the file you downloaded. On Windows the two hashes must be the same.

## Run from source

For contributors. Natamux is developed in a GitHub Codespace, where Docker, Node.js and pnpm are ready. In the codespace terminal:

```sh
scripts/dev.sh demo
```

Open port 5173 and sign in with the token `devtoken`. The demo has a task with a workspace, commits and changes. `scripts/dev.sh seed` makes tasks in every state with fake agents at work. See `docs/dev-setup.md` and `docs/docker.md` in the repository.

To start the desktop app from the source tree, see `packages/desktop/README.md`.

:::note
The development containers have no real coding agent. Use the desktop app to work with real agents.
:::

## Run with Docker

The production compose file runs the dashboard and the daemon in two hardened containers. It keeps tasks, stages and the dashboard, but its images contain no coding agents. See [Self-host with Docker](/docs/guides/self-host-docker/).

## Use the natamux command in your own shell

The desktop app puts the `natamux` and `nmx` commands in its own terminals, so agents can run `natamux done` and `natamux ask`. To use them in your own shell, add the app's `bin` folder to your `PATH`. The app rewrites the files in it each time it starts.

The folder is `bin` inside the app's user data folder:

| System | Folder |
|---|---|
| macOS | `~/Library/Application Support/Natamux/bin` |
| Windows | `%APPDATA%\Natamux\bin` |
| Linux | `~/.config/Natamux/bin` |

On macOS and Linux, add one line to `~/.zshrc` (zsh) or `~/.bashrc` (bash).

macOS:

```sh
export PATH="$HOME/Library/Application Support/Natamux/bin:$PATH"
```

Linux:

```sh
export PATH="$HOME/.config/Natamux/bin:$PATH"
```

Windows, in PowerShell (for your user, once):

```powershell
[Environment]::SetEnvironmentVariable('Path', "$env:APPDATA\Natamux\bin;" + [Environment]::GetEnvironmentVariable('Path', 'User'), 'User')
```

Open a new shell. The command also needs the daemon's token and address:

```sh
export NATAMUX_TOKEN="$(cat "$HOME/Library/Application Support/Natamux/daemon-token")"   # Linux: ~/.config/Natamux/daemon-token
export NATAMUX_URL=http://127.0.0.1:4317
natamux task list
```

In PowerShell:

```powershell
$env:NATAMUX_TOKEN = Get-Content "$env:APPDATA\Natamux\daemon-token"
$env:NATAMUX_URL = 'http://127.0.0.1:4317'
natamux task list
```

The app uses port 4317 when it is free, and another free port when it is not. Check the address in the dashboard's browser URL or in the daemon log. See [Desktop app](/docs/guides/desktop-app/) and the [CLI reference](/docs/reference/cli/).

## Next

[Quick start](/docs/getting-started/quick-start/).
