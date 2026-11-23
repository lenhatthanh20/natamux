# Security

## Report a vulnerability

Please do not open a public issue for a security problem.

Report it privately through GitHub: **Security → Report a vulnerability** on this repository ([private vulnerability reporting](https://github.com/lenhatthanh20/natamux/security/advisories/new)). Include the version, your platform, the steps to reproduce and what an attacker gains.

You get an answer within 7 days. A fix goes out in a new release, and the advisory is published after that release, with credit to you unless you ask otherwise.

## Supported versions

Natamux is young. Only the latest release gets security fixes.

## For a company's security review

[For a security review](https://natamux.github.io/docs/help/security-review/) lists every network connection, offline mode (`NATAMUX_OFFLINE=1`), the SBOM of each release, how the dependencies are controlled, how to check a download, and what is missing today.

## How Natamux is built to be safe

Natamux runs coding agents on your machine. An agent can read and change files and run commands with **your** user's rights. Natamux does not sandbox agents. Treat an agent like a person who types in your terminal, and use each agent's own permission settings.

### The daemon

- It listens on `127.0.0.1` (loopback) by default, so other machines cannot reach it. `NATAMUX_HOST` is not checked: any address works, including `0.0.0.0`, and you are then responsible for the network. The Docker image sets `0.0.0.0` inside its container, and `compose.prod.yaml` publishes the port on `${NATAMUX_BIND:-127.0.0.1}`, which is loopback unless you change it.
- Every API request needs a token in the `Authorization: Bearer` header. `GET /health` needs no token. A terminal WebSocket uses a one-time ticket instead. The daemon serves no static dashboard files. Tokens are compared in constant time.
- There are two kinds of token:
  - The daemon token is `NATAMUX_TOKEN` of the daemon. Without it, the daemon makes a random 192-bit token for each run and prints it once. It can call every route.
  - The agent token belongs to one task. It looks like `<task-id>.<secret>`, lives in the daemon's memory only, and is new after a daemon restart. The terminals of the task get it in `NATAMUX_TOKEN`.
- An agent token may call only the routes of its own task (not `DELETE`), reads of its direct subtasks, a short list of global reads, `POST /tasks` for its own subtasks (a subtask stays in its parent's project), and the hook and limits posts that agents send. Every other route answers 403. The list is in [the HTTP API reference](https://github.com/lenhatthanh20/natamux/blob/main/site/src/content/docs/docs/reference/http-api.md) and in `agent_may` in `crates/natamux-daemon/src/http/agent_rules.rs`.
- An agent cannot approve, skip or pass a stage, change project settings, or approve repo commands. Its events are recorded as the agent's. A command stage passes with a one-time ticket (`NATAMUX_STAGE_TICKET`) that only that stage's script gets: an agent cannot report such a stage done. An agent cannot fan out a task, and it cannot send the events that only the daemon sends (`stage.checked`, `stage.started`, `stage.reopened`, `pr.merged`). Every send-back by an agent counts for `maxSendBacks`.
- The `natamux` command in a task terminal uses only `NATAMUX_TOKEN`, so it runs with the agent token. The app ignores its saved token file when it starts inside a task terminal.
- The token travels in a header, never in a cookie, so another website you visit cannot use your session (no cross-site request forgery). A browser cannot add the header to a WebSocket, so terminals open with a one-time ticket that expires after 30 seconds. `/events` takes the header too, and also the token as `?token=` in the address; a reverse proxy can log that address.
- The daemon checks the `Host` header against an allow-list: `127.0.0.1`, `localhost` and `::1`, plus a non-wildcard `NATAMUX_HOST` and the names in `NATAMUX_ALLOWED_HOSTS` (comma list). A WebSocket handshake with an `Origin` header needs an allowed host name too. So a web page on another site cannot reach the daemon, not even by DNS rebinding.
- Files from a workspace that the Changes tab shows, such as images, get `sandbox` and `nosniff`.
- File paths from requests are checked so they cannot leave the workspace. The daemon writes into a worktree through a symlink-safe path, so a link inside a repo cannot send a write outside it.

### Repo trust

- A repo can carry commands: stage `check`, path `when.command`, stage `command` and `args` in `.natamux.json`, and the setup scripts `.natamux-setup` and `.natamux-setup.ps1`. These run with your rights. The agent config files are part of what you approve: they hold hooks, MCP servers and rules that an agent runs. One list counts for all agents: `.claude/settings.json`, `.claude/settings.local.json`, `.mcp.json`, `.codex/config.toml`, `.codex/hooks.json`, `.codex/rules/`, `.cursor/mcp.json`, `.cursor/hooks.json`, `.cursor/cli.json`, `.gemini/settings.json`, `.gemini/extensions/`, `.github/hooks/*.json`, `.qwen/settings.json`, `.qwen/extensions/`, `.factory/hooks.json`, `.factory/settings.json`, `.factory/mcp.json`, `opencode.json`, `opencode.jsonc`, `.opencode/`, `.aider.conf.yml`, `crush.json`, `.amp/settings.json` and `.claude/agents/`, `.claude/commands/`, `.claude/skills/`, `.claude/hooks/`. A script that a hook or an MCP server names, inside the worktree, counts too (best effort). A setup script that is a link never runs.
- The check runs in each task's worktree, on the branch that task works on, not only in the project folder. A branch with other hooks needs its own approval (`natamux repo trust <repo> --task <id> --approve <hash>`, or the app dialog, which lists the changed files). A worktree whose content is the same as an approved one needs no new step. Natamux keeps up to 32 approved worktree digests per project.
- A link in the agent config, a path that leaves the worktree, more than 500 files, a file over 1 MiB or more than 8 MiB in all make the folder unverifiable: it is never approved.
- A file that git tracks always counts, also at a path where Natamux writes its own hook file. An untracked file at such a path is Natamux's own only when its bytes are exactly what Natamux writes and `.git/info/exclude` lists it (outside a git repository, the exact bytes are enough). An untracked `.claude/settings.local.json` (your own Claude allow list) counts in Strict mode only.
- Natamux runs none of them until you approve them for that repo, in the app dialog or with `natamux repo trust <repo> --approve <hash>`. You approve the exact text you saw. Any change to a command, a setup script or an agent config file needs a new approval. A repo with no commands, no setup script and no agent config file needs none. After an update, a project that has an agent config file asks for one new approval.
- Repos that were already added when this check arrived were approved once with their commands at that time.
- An untrusted repo gets no repo commands, no setup script, no stage `args`, no auto mode and no workspace trust for agents. A blocked stage fails with a reason that says so.
- Saving the project settings in the app approves what you saved. **Overwrite** after a conflict asks first when the merged text has commands that came only from the changed file.
- In an untrusted repo, Draft commit message starts no agent and gives the simple draft. `natamux statusline` runs only the `statusLine` command of your user-level settings, never one from the project.
- Every agent start (new, resume, restore after a restart, reuse of a paused shell, reopen) passes one gate. It takes the digest before Natamux writes hook files, again after, and again after it writes the launch script. A change in between stops the start. When the agent ends and its config changed while it ran, the task's log gets a warning.
- Strict mode (Settings, coming with a later update) also refuses a worktree that is not approved: no hook file, no trust marker, no launch. The launch script then runs `natamux repo verify-launch` right before the agent, and the agent starts only when the daemon still sees the approved digest. Cursor's and Copilot's trust of the worktree is never written and is taken back when the content changes or the agent ends.
- Known limits: the digest covers the files above, not user-level agent config (`~/.claude`, `~/.codex`, …). An agent typed by hand in a Natamux terminal passes no gate.
- This covers Natamux's own commands and the trust it gives agents. It does not limit what an agent does inside its worktree after you approve (see Known limits).

### Secrets on disk

- The app folder and the daemon data folder are private to your user (mode `0700`), and Natamux tightens a wider folder when it starts. Modes `0700` and `0600` are set on macOS and Linux only. On Windows the folders keep their default permissions.
- The desktop app keeps its daemon token in its user data folder, readable only by your user (file mode `0600`).
- The scripts that start agents (`agent-launch-<task>.sh`) are written with mode `0600`, so the prompt and settings in them are not readable by other users.

### Agents and hooks

- Natamux learns what an agent does through the agent's hooks. For each agent it starts, it adds hook settings that call `natamux hook` or the daemon's local address. Those calls carry the agent token from the terminal's environment. The daemon takes the task from the token, not from the request.
- For most agents, the hook file goes into the task's worktree and is kept out of git through `.git/info/exclude`. A file of your own with the same name is left alone: Natamux rewrites a file only when git does not track it and it holds Natamux's hooks only, each exactly `natamux hook <agent>`, with no other top-level key (`env`, `apiKeyHelper`, `mcpServers`, `permissions`, …). The file of each agent is in the [hook file table](https://natamux.github.io/docs/concepts/agents-and-hooks/).
- For Codex, Natamux passes `--dangerously-bypass-hook-trust` so that the hooks it adds for that session run without a trust prompt. In Strict mode it does so only for an approved digest. Codex then also skips its trust review of the session's other hooks, including those of a project you already trust. See [Codex](https://natamux.github.io/docs/agents/codex/).
- In an approved repo, Natamux marks the task's worktree as trusted for the agent, so the repo's own agent config (`.codex`, `.cursor`, `.claude`, MCP servers) runs without a prompt. For Codex this is `trust_level="trusted"`. For Cursor and Copilot it writes a trust entry outside the worktree, which Natamux removes when it removes the worktree. For Gemini CLI it sets `GEMINI_CLI_TRUST_WORKSPACE=true`, and for Muse Code it passes `--trust-workspace`. In an untrusted repo none of this is set.
- In Strict mode each agent starts in its ask mode, and an agent without one does not start: `--permission-mode default` (Claude Code), `--ask-for-approval on-request --sandbox workspace-write` (Codex), `--approval-mode default` (Gemini CLI, Qwen Code), `GOOSE_MODE=approve` (Goose). The [agent adapters doc](docs/agent-adapters.md#ask-mode-in-strict-mode) has the table.
- In an approved repo, by default Natamux starts each built-in agent with a flag that skips most permission prompts, for example `--permission-mode auto` (Claude Code) and `--force` (Cursor CLI). [The configuration reference](https://natamux.github.io/docs/reference/configuration/#agents) lists the flag of every agent. Only Claude Code checks each action; the others run commands without asking. `"agents": { "permissionMode": "ask" }` in `.natamux.json` turns the flags off. An untrusted repo gets no auto flags.
- Title, body and comments of a GitHub issue that you import are marked as data, not as instructions, in the first prompt and in `context.md`. This helps, and it is not a guarantee.
- Each task works in its own git worktree. Natamux pushes, merges or deletes a branch only when you click the button or run the command for it, with these exceptions. For a task that has **Check on CI** ticked (off by default), each CI round runs `git push --force` of a snapshot commit to `natamux-ci/<branch>` on `origin`; the snapshot holds every file git does not ignore, also uncommitted ones, and Natamux deletes that remote branch when the task ends or is deleted. Natamux also runs `git fetch` when a workspace is made, `git ls-remote` when the New workspace dialog lists remote branches, `git pull --ff-only` before a stage that follows a merged subtask, and `gh issue create` when `tracker.createIssue` is `on-spec-approved` or `on-pr`. These commands use `gh auth git-credential` for github.com. An agent token can call the routes of its own task, which include the push and pull request routes.

### The desktop app

- The apps are **not signed** with an Apple Developer ID or a Windows certificate, and the macOS app is not notarized, so macOS warns on first open. Release macOS apps are signed with a Natamux self-signed certificate; only dry runs are ad hoc. Windows and Linux builds are not signed. Download them only from this repository's Releases page.
- The app is native. It has no web view and shows no web page. Links in the app open in your browser through one helper that allows only `http`, `https` and `mailto`. Links in a terminal follow the same rule.
- The terminal is display only. It ignores the escape code that sets the clipboard (OSC 52), so a program in a terminal cannot write to your clipboard.
- The Docs tab does not load images from the web or from files; only images inside the document itself are drawn.
- Before it sends its token to a daemon it started, the app checks a proof from the daemon's `/health` that the daemon knows the token. A program that took the port first cannot get the token.
- **Updates** come over HTTPS from this repository's latest GitHub release. The app starts the system `curl` for this (found through `PATH`; it reads `~/.curlrc` and proxy variables). `SHA256SUMS.txt` of each release is signed with [minisign](https://jedisct1.github.io/minisign/); the secret key stays offline with the maintainer and is never on GitHub. Before it downloads an update, the app checks that signature against the public key built into it, and that the signed trusted comment is `natamux <tag>` of that release, whose version must be newer than its own. Any failure, including a missing signature, stops the update. Then the app compares the SHA-256 of the download with `SHA256SUMS.txt`, and on macOS runs `codesign --verify --deep --strict` on the new app, which checks that the code signature is intact, not who signed it (the minisign signature proves who published), and checks its version before it swaps the app in. The Linux `.deb` update checks the hash again as root before it installs. Versions up to v0.1.23 do not check the signature and trust the GitHub release alone. The release public key:

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
