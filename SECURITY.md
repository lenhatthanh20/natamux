# Security

## Report a vulnerability

Please do not open a public issue for a security problem.

Report it privately through GitHub: **Security → Report a vulnerability** on this repository ([private vulnerability reporting](https://github.com/lenhatthanh20/natamux/security/advisories/new)). Include the version, your platform, the steps to reproduce and what an attacker gains.

You get an answer within 7 days. A fix goes out in a new release, and the advisory is published after that release, with credit to you unless you ask otherwise.

## Supported versions

Natamux is young. Only the latest release gets security fixes.

## How Natamux is built to be safe

Natamux runs coding agents on your machine. An agent can read and change files and run commands with **your** user's rights. Natamux does not sandbox agents. Treat an agent like a person who types in your terminal, and use each agent's own permission settings.

### The daemon

- It listens on `127.0.0.1` (loopback) by default, so other machines cannot reach it. Only the Docker setup listens on all interfaces inside its container, and `compose.prod.yaml` publishes the port to loopback only.
- Every API request needs the daemon token in the `Authorization: Bearer` header. `/health` and the static dashboard files are the only exceptions. Tokens are compared in constant time.
- The token is `NATAMUX_TOKEN`. Without it, the daemon makes a random 192-bit token for each run and prints it once.
- The token travels in a header, never in a cookie, so another website you visit cannot use your session (no cross-site request forgery). A browser cannot add the header to a WebSocket, so terminals open with a one-time ticket that expires after 30 seconds. The token never appears in a URL.
- The dashboard is served with a strict Content Security Policy (`default-src 'self'`, `frame-ancestors 'none'`). Files from a workspace that the Changes tab shows, such as images, get `sandbox` and `nosniff`.
- File paths from requests are checked so they cannot leave the workspace or the dashboard folder.

### Secrets on disk

- The desktop app keeps its daemon token in its user data folder, readable only by your user (file mode `0600`).
- The scripts that start agents (`agent-launch-<task>.sh`) are written with mode `0600`, so the prompt and settings in them are not readable by other users.
- The dashboard keeps the token in the browser's `localStorage` for its own address only. Sign out in the account menu to remove it.

### Agents and hooks

- Natamux learns what an agent does through the agent's hooks. For each agent it starts, it adds hook settings that call `natamux hook` or the daemon's local address. Those calls carry the task id and the token from the terminal's environment, and nothing else.
- For Copilot and Cursor, the hook file goes into the task's worktree (`.github/hooks/natamux.json`, `.cursor/hooks.json`) and is kept out of git through `.git/info/exclude`. A file of your own with the same name is left alone.
- For Codex, Natamux passes `--dangerously-bypass-hook-trust` so that the hooks it adds for that session run without a trust prompt. It only affects the hooks Natamux passes on the command line.
- By default Natamux starts each agent with a flag that skips most permission prompts: `--permission-mode auto` (Claude Code), `--full-auto` (Codex), `--approval-mode yolo` (Gemini CLI), `--allow-all-tools` (Copilot CLI), `--force` (Cursor CLI). Only Claude Code checks each action; the others run commands without asking. `"agents": { "permissionMode": "ask" }` in `.natamux.json` turns the flags off.
- Each task works in its own git worktree. Natamux never pushes, merges or deletes a branch unless you click the button or run the command for it.

### The desktop app

- The first releases are **not signed** with an Apple or Windows certificate. The macOS app has an ad hoc signature only. Download it only from this repository's Releases page.
- The window shows only the local dashboard. Other links open in your browser.
- **Updates** come over HTTPS from this repository's latest GitHub release. The app compares the SHA-256 of the download with `SHA256SUMS.txt` of the same release, and on macOS checks the app's signature and version before it swaps the app in. The checksum file comes from the same release, so it catches a broken download, not a forged release. Until the app is signed with a certificate, the trust rests on the GitHub repository alone.

## What is out of scope

- What an agent does with the rights you give it.
- An attacker who already runs code as your user, or who can read your files.
- Running the daemon on a network address without TLS. Put a reverse proxy with TLS in front of it if you must.
