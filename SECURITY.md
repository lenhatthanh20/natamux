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
- Every API request needs a token in the `Authorization: Bearer` header. `/health` and the static dashboard files are the only exceptions. Tokens are compared in constant time.
- There are two kinds of token:
  - The daemon token is `NATAMUX_TOKEN` of the daemon. Without it, the daemon makes a random 192-bit token for each run and prints it once. It can call every route.
  - The agent token belongs to one task. It looks like `<task-id>.<secret>`, lives in the daemon's memory only, and is new after a daemon restart. The terminals of the task get it in `NATAMUX_TOKEN`.
- An agent token may call only the routes of its own task (not `DELETE`), reads of its direct subtasks, a short list of global reads, `POST /tasks` for its own subtasks, and the hook and limits posts that agents send. Every other route answers 403. The list is in [the HTTP API reference](https://github.com/lenhatthanh20/natamux/blob/main/site/src/content/docs/docs/reference/http-api.md) and in `agent_may` in `crates/natamux-daemon/src/http/agent_rules.rs`.
- An agent cannot approve, skip or pass a stage, change project settings, or approve repo commands. Its events are recorded as the agent's. A command stage passes with a one-time ticket (`NATAMUX_STAGE_TICKET`) that only that stage's script gets.
- The `natamux` command in a task terminal uses only `NATAMUX_TOKEN`, so it runs with the agent token. The app ignores its saved token file when it starts inside a task terminal.
- The token travels in a header, never in a cookie, so another website you visit cannot use your session (no cross-site request forgery). A browser cannot add the header to a WebSocket, so terminals open with a one-time ticket that expires after 30 seconds. `/events` takes the header too. Only the legacy web dashboard still sends the token as `?token=` on `/events`, because a browser cannot set a header there. Do not use that dashboard on a shared machine or behind a proxy that logs URLs.
- The daemon checks the `Host` header against an allow-list: `127.0.0.1`, `localhost` and `::1`, plus the names in `NATAMUX_ALLOWED_HOSTS` (comma list). A WebSocket handshake with an `Origin` header needs an allowed host name too. So a web page on another site cannot reach the daemon, not even by DNS rebinding.
- The dashboard is served with a strict Content Security Policy (`default-src 'self'`, `connect-src 'self'`, `frame-ancestors 'none'`). Files from a workspace that the Changes tab shows, such as images, get `sandbox` and `nosniff`.
- File paths from requests are checked so they cannot leave the workspace or the dashboard folder. The daemon writes into a worktree through a symlink-safe path, so a link inside a repo cannot send a write outside it.

### Repo trust

- A repo can carry commands: stage `check`, path `when.command`, stage `command` and `args` in `.natamux.json`, and the setup scripts `.natamux-setup` and `.natamux-setup.ps1`. These run with your rights.
- Natamux runs none of them until you approve them for that repo, in the app dialog or with `natamux repo trust <repo> --approve <hash>`. You approve the exact text you saw. Any change to a command or a setup script needs a new approval. A repo with no commands needs none.
- Repos that were already added when this check arrived were approved once with their commands at that time.
- An untrusted repo gets no repo commands, no setup script, no stage `args`, no auto mode and no workspace trust for agents. A blocked stage fails with a reason that says so.
- Saving the project settings in the app approves what you saved.
- This covers Natamux's own commands and the trust it gives agents. It does not limit what an agent does inside its worktree after you approve (see Known limits).

### Secrets on disk

- The app folder and the daemon data folder are private to your user (mode `0700`), and Natamux tightens a wider folder when it starts.
- The desktop app keeps its daemon token in its user data folder, readable only by your user (file mode `0600`).
- The scripts that start agents (`agent-launch-<task>.sh`) are written with mode `0600`, so the prompt and settings in them are not readable by other users.
- The dashboard keeps the token in the browser's `localStorage` for its own address only. In a browser, Sign out in the account menu removes it. The desktop app is a native app: it has no page and no Sign out.

### Agents and hooks

- Natamux learns what an agent does through the agent's hooks. For each agent it starts, it adds hook settings that call `natamux hook` or the daemon's local address. Those calls carry the agent token from the terminal's environment. The daemon takes the task from the token, not from the request.
- For Copilot and Cursor, the hook file goes into the task's worktree (`.github/hooks/natamux.json`, `.cursor/hooks.json`) and is kept out of git through `.git/info/exclude`. A file of your own with the same name is left alone.
- For Codex, Natamux passes `--dangerously-bypass-hook-trust` so that the hooks it adds for that session run without a trust prompt. It only affects the hooks Natamux passes on the command line.
- In an approved repo, Natamux marks the task's worktree as trusted for the agent, so the repo's own agent config (`.codex`, `.cursor`, `.claude`, MCP servers) runs without a prompt. For Codex this is `trust_level="trusted"`. For Cursor and Copilot it writes a trust entry outside the worktree, which Natamux removes when it removes the worktree. In an untrusted repo none of this is set.
- In an approved repo, by default Natamux starts each agent with a flag that skips most permission prompts: `--permission-mode auto` (Claude Code), `--sandbox workspace-write --ask-for-approval never` (Codex), `--approval-mode yolo` (Gemini CLI), `--allow-all-tools` (Copilot CLI), `--force` (Cursor CLI). Only Claude Code checks each action; the others run commands without asking. `"agents": { "permissionMode": "ask" }` in `.natamux.json` turns the flags off. An untrusted repo gets no auto flags.
- Text from a GitHub issue that you import into a prompt is marked as data, not as instructions. This helps, and it is not a guarantee.
- Each task works in its own git worktree. Natamux never pushes, merges or deletes a branch unless you click the button or run the command for it. An agent token can call the routes of its own task, which include the push and pull request routes.

### The desktop app

- The first releases are **not signed** with an Apple or Windows certificate. The macOS app has an ad hoc signature only. Download it only from this repository's Releases page.
- The app is native. It has no web view and shows no web page. Links in the app open in your browser through one helper that allows only `http`, `https` and `mailto`. Links in a terminal follow the same rule.
- The terminal is display only. It ignores the escape code that sets the clipboard (OSC 52), so a program in a terminal cannot write to your clipboard.
- The Docs tab does not load images from the web or from files; only images inside the document itself are drawn.
- Before it sends its token to a daemon it started, the app checks a proof from the daemon's `/health` that the daemon knows the token. A program that took the port first cannot get the token.
- **Updates** come over HTTPS from this repository's latest GitHub release. `SHA256SUMS.txt` of each release is signed with [minisign](https://jedisct1.github.io/minisign/); the secret key stays offline with the maintainer and is never on GitHub. Before it downloads an update, the app checks that signature against the public key built into it, and that the signed trusted comment is `natamux <tag>` of that release, whose version must be newer than its own. Any failure, including a missing signature, stops the update. Then the app compares the SHA-256 of the download with `SHA256SUMS.txt`, and on macOS checks the app's signature and version before it swaps the app in. The Linux `.deb` update checks the hash again as root before it installs. Versions up to v0.1.23 do not check the signature and trust the GitHub release alone. The release public key:

  ```
  RWSgtwsXHppEP5n4kgg1Q3yNIYODQNGsMQBKF93tjSRwzL1BrjeGoBpo
  ```

### If the release key leaks

The app has one public key built in. If the secret key leaks or is lost:

1. Stop releasing. Publish a security advisory on this repository that names the last release you trust.
2. Make a new key pair on a clean machine.
3. Ship a release with the new public key built in. An installed app accepts it only if the old key signs it. If an attacker holds the old key, they can sign too, so do not rely on this path: tell users to reinstall by hand.
4. Users reinstall from the Releases page and check the download as in the install guide. Delete releases signed after the leak.

Plan to shorten this: ship a second public key in the app, with its secret key kept offline on a different medium than the first. The app will accept a signature from either key. Then a leak of one key is fixed by a release signed with the other, with no manual reinstall. Until that ships, a leak means a manual reinstall for every user.

## Known limits

- An agent runs as your user. It can read your files, including the daemon token file, and the daemon's environment. The agent token and the gate rules stop an agent that follows the API, for example one that follows bad instructions from a web page or an issue. They do not stop one that looks for the daemon token. Keeping the token in the system keychain, or running agents as another user, is future work.
- In `auto` mode (the default in an approved repo) agents run commands without asking you, apart from Claude Code's own checks. `ask` keeps each agent's prompts. Natamux does not sandbox either mode.
- Repo trust covers Natamux's own commands and the trust it gives agents. It does not control what an agent does inside its worktree: it can run any command there with your rights, also in `ask` mode if you allow it.
- A task terminal on Windows types the command stage ticket into that terminal, so the ticket shows there.

## What is out of scope

- What an agent does with the rights you give it.
- An attacker who already runs code as your user, or who can read your files.
- Running the daemon on a network address without TLS. Put a reverse proxy with TLS in front of it if you must.
