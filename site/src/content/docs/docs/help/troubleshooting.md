---
title: Troubleshooting
description: Fixes for the most common problems when you install and run Natamux.
sidebar:
  order: 1
---

Find the problem, then follow the fix. If nothing helps, check the [daemon log](#where-the-logs-are) and open an issue on [GitHub](https://github.com/lenhatthanh20/natamux/issues).

## macOS says the app is blocked

The app is not signed with an Apple certificate, so macOS blocks it the first time.

1. Open Natamux once.
2. Open **System Settings → Privacy & Security** and click **Open Anyway**.

If there is no button, remove the quarantine flag:

```sh
xattr -dr com.apple.quarantine /Applications/Natamux.app
```

## The AppImage does not start on Ubuntu

Ubuntu 24.04 and later can block Chromium's sandbox. Either install the `.deb`, or start the AppImage with `--no-sandbox`:

```sh
./Natamux-linux-x86_64.AppImage --no-sandbox
```

## "command not found" when I start an agent

**Start agent** types the agent's command into the task's terminal. If the program is not installed, or not on the PATH, the shell prints `command not found`.

The commands are `claude`, `codex`, `gemini`, `copilot` and `cursor-agent`.

1. Check in your own terminal: `command -v claude`.
2. Install the agent and log in to it once, outside Natamux.
3. If the program has another name or path, set `NATAMUX_<AGENT>_COMMAND` for the daemon, with `<AGENT>` one of `CLAUDE`, `CODEX`, `GEMINI`, `COPILOT` or `CURSOR`:

   ```sh
   export NATAMUX_CLAUDE_COMMAND=/opt/tools/claude
   ```

4. Restart Natamux so the daemon gets the new variable.

The desktop app uses the PATH of your login shell. If you installed the agent after you opened the app, quit the app and open it again. Use `NATAMUX_<AGENT>_ARGS` to add options to an agent's command line. See [Environment variables](/docs/reference/environment-variables/).

## The dashboard rejects my token

The dashboard shows "The daemon rejected that token. Paste the token it printed when it started."

- **Desktop app:** you are signed in automatically. If you see the token field anyway, quit and open the app again.
- **Browser, Docker:** use the value of `NATAMUX_TOKEN` you set.
- **Browser, daemon you started yourself without `NATAMUX_TOKEN`:** the daemon makes a new random token on every run and prints it once. Use the one from the latest start, or set `NATAMUX_TOKEN` to keep it.
- **CLI:** `natamux` needs `NATAMUX_TOKEN` and, when the daemon is not on `http://127.0.0.1:4317`, `NATAMUX_URL`. The desktop app's token is in `daemon-token` in its [user data folder](/docs/guides/desktop-app/#where-things-are).

## The port is in use

- **Desktop app:** it uses port 4317 when free and any free port otherwise, so it starts anyway. The CLI looks at 4317 by default, so set `NATAMUX_URL` to the real address. The log line "starting the daemon on port N" shows it.
- **Daemon you started yourself:** set another port with `NATAMUX_PORT`.
- **Docker:** set `NATAMUX_PORT` (default 8080).
- Only one copy of the desktop app runs. Opening it twice focuses the first window.

## The daemon stopped

The desktop app shows "The Natamux daemon stopped" with the last lines of the log, or "Natamux could not start its daemon". Read those lines and the [log](#where-the-logs-are). If a repository was moved, remove its project and add the new folder with **File → Add Project…**.

## A task does not change state while the agent works

Natamux learns an agent's state from its hooks. If the task stays "ready" or never reaches "needs input":

1. Start the agent with **Start agent** in the task, not by hand. Natamux adds the hook settings only for sessions it starts.
2. Check that the `natamux` command exists in the task's terminal: `command -v natamux`. The desktop app puts it first on the PATH. In Docker, hooks need `NATAMUX_URL=http://host:8080/api`.
3. Set `NATAMUX_DEBUG=1` in the terminal to see why a hook did not arrive. `natamux hook` is silent otherwise and never blocks the agent.
4. A proxy on your machine can refuse calls to the daemon. Add `127.0.0.1` to `NO_PROXY`.

Agents without hooks are followed by their output and bell. They can report with `natamux ask` and `natamux done`. See [Agents and hooks](/docs/concepts/agents-and-hooks/).

## Removing a workspace fails

Natamux refuses to remove a worktree that has files not committed: "T-0001 has N changes that are not committed. Commit or discard them first". Nothing is ended or deleted when this happens.

1. Open the **Changes** tab.
2. Commit the changes, or discard them in the terminal (`git restore .` and `git clean -fd` inside the worktree).
3. Remove the workspace again.

The branch is deleted only when it is fully merged. Otherwise it stays, and you delete it with git when you are sure.

## Push or pull request fails

- **Push** uses your own git login. For github.com it also asks `gh auth git-credential`, so `gh auth login` or `GH_TOKEN` is enough.
- **Create pull request** needs the `gh` command, logged in. It refuses when the task already has a pull request.

## Where the logs are

| What | Where |
|---|---|
| Desktop app, daemon | `logs/daemon.log` in the user data folder, or **File → Show Daemon Log**. macOS: `~/Library/Application Support/Natamux`. Linux: `~/.config/Natamux` |
| Docker | `docker compose -f compose.prod.yaml logs` |
| A task's history | The **Timeline** and events of the task in the dashboard |
