# @helmsman/desktop

The desktop app: an Electron shell for macOS and Windows. It holds no logic of its own. It starts the daemon and shows the dashboard that the daemon serves.

## What it does

- **First run:** asks for the repository whose tasks Helmsman runs. **File → Open Repository…** changes it later; the daemon starts again.
- **Starts the daemon** in an Electron utility process, with:
  - `HELMSMAN_WEB_DIR` set, so the daemon serves the built dashboard and the API (`/api/...`) on one port;
  - port 4317 when free, else any free port;
  - a token kept in the app's data folder, so it stays the same between runs;
  - data in `<user data>/data`;
  - the PATH of your login shell, so the agents installed with Homebrew, nvm and the like are found when the app starts from the Dock;
  - a `helmsman` command first on that PATH, for `helmsman done` and `helmsman ask` in the agents' terminals. It runs the CLI with the app's own runtime.
- **Opens the dashboard** at `http://127.0.0.1:<port>/`. The preload script puts the token where the dashboard keeps it, so the page starts signed in.
- **Keeps the window on the dashboard.** Links to other sites open in your browser; only `http`, `https` and `mailto` open at all. The page runs sandboxed, with no Node.js.
- **Quitting** asks first when agents are working, because it stops them. Tasks, worktrees and changes stay. On macOS, closing the window keeps the app and the agents running.

The user data folder is `~/Library/Application Support/Helmsman` on macOS and `%APPDATA%\Helmsman` on Windows. The daemon log is in its `logs` folder (**File → Show Daemon Log**).

## Run it from the repository

```sh
pnpm install
pnpm -r build
pnpm --filter @helmsman/desktop rebuild-native   # node-pty for Electron's runtime
pnpm --filter @helmsman/desktop start
```

`rebuild-native` builds node-pty for Electron. A daemon run with plain Node.js afterwards needs `pnpm rebuild node-pty` again.

## Check it without a screen

`HELMSMAN_DESKTOP_SMOKE=1` loads the dashboard, prints what it found, and quits. `HELMSMAN_DESKTOP_SMOKE_SHOT=<file.png>` also saves a picture of the window. On Linux:

```sh
HELMSMAN_DESKTOP_SMOKE=1 xvfb-run -a pnpm --filter @helmsman/desktop start -- --no-sandbox
```

## Not here yet

Packaging, signing, notarization and auto-update (build order step 11), and a CI run on macOS and Windows (step 10).
