---
title: Install Natamux
description: Download the desktop app for macOS or Linux, or run Natamux from source or in Docker.
sidebar:
  order: 1
---

Natamux is a desktop app for macOS and Linux. There is no Windows build yet, and no automatic updates: watch the [releases page](https://github.com/lenhatthanh20/natamux/releases/latest) for new versions.

## Requirements

- **git.** Natamux makes a git worktree for each task.
- **At least one coding agent**, installed and logged in: `claude`, `codex`, `gemini`, `copilot` or `cursor-agent`. See [Agents](/natamux/docs/agents/claude-code/).
- **`gh` (GitHub CLI)**, optional. Needed for opening pull requests, importing issues and CI status. Run `gh auth login` once.

## Download

Get the file for your system from the [latest release](https://github.com/lenhatthanh20/natamux/releases/latest).

| System | File |
|---|---|
| macOS, Apple silicon (M1 and later) | [`Natamux-mac-arm64.dmg`](https://github.com/lenhatthanh20/natamux/releases/latest/download/Natamux-mac-arm64.dmg) |
| macOS, Intel | [`Natamux-mac-x64.dmg`](https://github.com/lenhatthanh20/natamux/releases/latest/download/Natamux-mac-x64.dmg) |
| Linux, any distribution | [`Natamux-linux-x86_64.AppImage`](https://github.com/lenhatthanh20/natamux/releases/latest/download/Natamux-linux-x86_64.AppImage) |
| Debian, Ubuntu | [`Natamux-linux-amd64.deb`](https://github.com/lenhatthanh20/natamux/releases/latest/download/Natamux-linux-amd64.deb) |
| Linux, no install | [`Natamux-linux-x64.tar.gz`](https://github.com/lenhatthanh20/natamux/releases/latest/download/Natamux-linux-x64.tar.gz) |

:::caution
The apps are **not signed** with an Apple certificate. Download them only from the Natamux releases page, and follow the first-open steps below.
:::

## Install on macOS

1. Open the `.dmg` and drag **Natamux** to **Applications**.
2. Open Natamux. macOS blocks it the first time.
3. Open **System Settings → Privacy & Security** and click **Open Anyway**.
4. If there is no such button, remove the quarantine flag and open the app again:

   ```sh
   xattr -dr com.apple.quarantine /Applications/Natamux.app
   ```

## Install on Linux

### AppImage

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

You should see `OK` for the file you downloaded.

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

The production compose file runs the dashboard and the daemon in two hardened containers. It keeps tasks, stages and the dashboard, but its images contain no coding agents. See [Self-host with Docker](/natamux/docs/guides/self-host-docker/).

## Use the natamux command in your own shell

The desktop app puts the `natamux` and `nmx` commands in its own terminals, so agents can run `natamux done` and `natamux ask`. To use them in your own shell, add the app's `bin` folder to your `PATH`. The app rewrites the files in it each time it starts.

The folder is `bin` inside the app's user data folder:

| System | Folder |
|---|---|
| macOS | `~/Library/Application Support/Natamux/bin` |
| Linux | `~/.config/Natamux/bin` |

Add one line to `~/.zshrc` (zsh) or `~/.bashrc` (bash).

macOS:

```sh
export PATH="$HOME/Library/Application Support/Natamux/bin:$PATH"
```

Linux:

```sh
export PATH="$HOME/.config/Natamux/bin:$PATH"
```

Open a new shell. The command also needs the daemon's token and address:

```sh
export NATAMUX_TOKEN="$(cat "$HOME/Library/Application Support/Natamux/daemon-token")"   # Linux: ~/.config/Natamux/daemon-token
export NATAMUX_URL=http://127.0.0.1:4317
natamux task list
```

The app uses port 4317 when it is free, and another free port when it is not. Check the address in the dashboard's browser URL or in the daemon log. See [Desktop app](/natamux/docs/guides/desktop-app/) and the [CLI reference](/natamux/docs/reference/cli/).

## Next

[Quick start](/natamux/docs/getting-started/quick-start/).
