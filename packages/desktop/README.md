# @natamux/desktop

The desktop app: an Electron shell. Releases are built for macOS on Apple silicon only; Intel Mac and Linux builds are paused, and there is no Windows build. It holds no logic of its own. It starts the daemon and shows the dashboard that the daemon serves.

## What it does

- **First run:** opens the dashboard with no project. Add one in the dashboard, or with **File → Add Project…**, which adds it without a restart.
- **Starts the daemon** in an Electron utility process, with:
  - `NATAMUX_WEB_DIR` set, so the daemon serves the built dashboard and the API (`/api/...`) on one port;
  - port 4317 when free, else any free port;
  - a token kept in the app's data folder, so it stays the same between runs;
  - data in `<user data>/data`;
  - the PATH of your login shell, so the agents installed with Homebrew, nvm and the like are found when the app starts from the Dock. When it cannot be read, the daemon log says why and which PATH is used instead;
  - a `natamux` command first on that PATH, for `natamux done` and `natamux ask` in the agents' terminals. It runs the CLI with the app's own runtime. Its folder is also in `NATAMUX_BIN_DIR`, so the daemon puts it first again for each agent after the shell's profile has run.
- **Opens the dashboard** at `http://127.0.0.1:<port>/`. The window opens as the daemon starts, on a blank page in the theme's background color, and loads the dashboard once the daemon is ready. The preload script puts the token where the dashboard keeps it, so the page starts signed in. The daemon log gets one `startup marks:` line with the times of the start.
- **Keeps the window on the dashboard.** Links to other sites open in your browser; only `http`, `https` and `mailto` open at all. The page runs sandboxed, with no Node.js.
- **Updates itself** from the latest GitHub release: checks 30 seconds after start and every 6 hours, downloads in the background, checks `SHA256SUMS.txt`, installs on restart (`updater.ts`, design in `docs/design-desktop-update.md`). Only a packaged app checks. `NATAMUX_UPDATE_AS_VERSION=0.0.0` makes it act as an older version, so a release can update to itself in a test. The log is `logs/update.log`.
- **Quitting** asks first when agents are working, because it stops them. Tasks, worktrees and changes stay. On macOS, closing the window keeps the app and the agents running.

The user data folder is `~/Library/Application Support/Natamux` on macOS and `%APPDATA%\Natamux` on Windows. The daemon log is in its `logs` folder (**File → Show Daemon Log**).

## Run it from the repository

```sh
pnpm install
pnpm -r build
pnpm --filter @natamux/desktop rebuild-native   # node-pty for Electron's runtime
pnpm --filter @natamux/desktop start
```

`rebuild-native` builds node-pty for Electron. A daemon run with plain Node.js afterwards needs `pnpm rebuild node-pty` again.

## Package it

```sh
scripts/package-desktop.sh
```

It builds the app for the machine it runs on, into `packages/desktop/release`: a `.dmg` on macOS, an installer on Windows, a folder on Linux. It needs Node.js 22.13 or newer, pnpm (`corepack enable`), and a C++ toolchain for node-pty (on macOS: `xcode-select --install`).

- The app is not signed with a certificate or notarized yet (build order step 11). On macOS it gets an ad hoc signature (`after-pack.cjs`), which is enough on the Mac that built it. A copy downloaded to another Mac needs right click → **Open** the first time, or `xattr -dr com.apple.quarantine /Applications/Natamux.app`.
- Files stay plain (no asar): node-pty's helper and the daemon's entry run from disk.
- The dashboard goes in as its built files only (`Resources/web`); the daemon and the CLI come with their production dependencies.

## Check it without a screen

`NATAMUX_DESKTOP_SMOKE=1` loads the dashboard and quits: exit code 0 when the page started signed in, 1 when not. `NATAMUX_DESKTOP_SMOKE_SHOT=<file.png>` also saves a picture of the window. CI runs it on macOS and Windows (the `os` job, on main). On Linux:

```sh
NATAMUX_DESKTOP_SMOKE=1 xvfb-run -a pnpm --filter @natamux/desktop start -- --no-sandbox
```

## Not here yet

Signing and notarization (build order step 11). After signing, the updater should also check that the new Mac app has the same Team ID.
