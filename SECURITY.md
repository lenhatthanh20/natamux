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
- An agent token may call only the routes that have a name for what they do (deny by default), on its own task (not `DELETE`), reads of its direct subtasks (`GET /tasks/<id>` gives a short status view), `POST /tasks` for its own subtasks (a subtask stays in its parent's project, and `dependsOn` stays inside its own task and subtasks), and the hook and limits posts that agents send. Lists and dashboard reads (`/tasks`, `/jobs`, `/usage`, `/activity`, `/diffstats`, `/repos`, `/events`) give only its own task's rows and its own project, without folders. Every other route answers 403 and logs `agent-api: denied <action> <route> task=<id>`, without body, query or token. The token ends 60 seconds after its task is Done or Cancelled, and at once when the task is deleted. The list is in [the HTTP API reference](https://github.com/lenhatthanh20/natamux/blob/main/site/src/content/docs/docs/reference/http-api.md) and in `crates/natamux-daemon/src/http/agent_actions.rs`.
- In Strict mode (`agentApi.mode: scoped`) the agent token gets a fixed minimum of actions, plus the grants listed in `agentApi.grants`. Push, pull request, issue, agent start, rollback, subtask files, the task list, the event stream and project reads need a grant; `requireHumanForPush` (the default) never gives the push grant. `/events` takes the agent token only in the header.
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
- Strict mode (Settings > Security) also refuses a worktree that is not approved: no hook file, no trust marker, no launch. The launch script then runs `natamux repo verify-launch` right before the agent, and the agent starts only when the daemon still sees the approved digest. Cursor's and Copilot's trust of the worktree is never written and is taken back when the content changes or the agent ends.
- Known limits: the digest covers the files above, not user-level agent config (`~/.claude`, `~/.codex`, …). An agent typed by hand in a Natamux terminal passes no gate.
- This covers Natamux's own commands and the trust it gives agents. It does not limit what an agent does inside its worktree after you approve (see Known limits).

### Secrets on disk

- The app folder and the daemon data folder are private to your user (mode `0700`), and Natamux tightens a wider folder when it starts. Modes `0700` and `0600` are set on macOS and Linux only. On Windows the folders keep their default permissions.
- The desktop app keeps its daemon token in its user data folder, readable only by your user (file mode `0600`).
- The scripts that start agents (`agent-launch-<task>.sh`) are written with mode `0600`, so the prompt and settings in them are not readable by other users.
- Task files, event logs, saved terminal screens, shell lists, usage files, attachments, the SQLite index (with its `-wal` and `-shm` files) and the app, daemon and update logs are written with mode `0600`, in folders with mode `0700`. At start the daemon tightens wider files and folders an older version left in the top two levels of the data folder. Worktrees keep their modes: they are your code.
- Deleting a task removes every file Natamux kept for it: the task file, events, usage, shell lists, saved screens, archive, attachments, and its launch and stage-command scripts. The SQLite index deletes with `secure_delete` on and empties its WAL file after the delete. A delete writes `tasks/<task>.deleting` first; if Natamux stops in the middle, the next start finishes the delete. The next start also removes leftover files of a task that has neither a task file nor an index row. Deleting a task never deletes a worktree with uncommitted changes unless you choose to, and never deletes a folder that is not a worktree.
- Log lines and git error messages pass a redaction step: the user name and password of a URL, the values of `token`, `access_token`, `key`, `password`, `secret` and `sig` in a URL query, and words that start with `ghp_`, `github_pat_`, `gho_` or `sk-` are replaced with `***`. Other secrets in free text are not found.

### Agents and hooks

- Natamux learns what an agent does through the agent's hooks. For each agent it starts, it adds hook settings that call `natamux hook` or the daemon's local address. Those calls carry the agent token from the terminal's environment. The daemon takes the task from the token, not from the request.
- For most agents, the hook file goes into the task's worktree and is kept out of git through `.git/info/exclude`. A file of your own with the same name is left alone: Natamux rewrites a file only when git does not track it and it holds Natamux's hooks only, each exactly `natamux hook <agent>`, with no other top-level key (`env`, `apiKeyHelper`, `mcpServers`, `permissions`, …). The file of each agent is in the [hook file table](https://natamux.github.io/docs/concepts/agents-and-hooks/).
- For Codex, Natamux passes `--dangerously-bypass-hook-trust` so that the hooks it adds for that session run without a trust prompt. In Strict mode it does so only for an approved digest. Codex then also skips its trust review of the session's other hooks, including those of a project you already trust. See [Codex](https://natamux.github.io/docs/agents/codex/).
- In an approved repo, Natamux marks the task's worktree as trusted for the agent, so the repo's own agent config (`.codex`, `.cursor`, `.claude`, MCP servers) runs without a prompt. For Codex this is `trust_level="trusted"`. For Cursor and Copilot it writes a trust entry outside the worktree, which Natamux removes when it removes the worktree. For Gemini CLI it sets `GEMINI_CLI_TRUST_WORKSPACE=true`, and for Muse Code it passes `--trust-workspace`. In an untrusted repo none of this is set.
- In Strict mode each agent starts in its ask mode, and an agent without one does not start: `--permission-mode default` (Claude Code), `--ask-for-approval on-request --sandbox workspace-write` (Codex), `--approval-mode default` (Gemini CLI, Qwen Code), `GOOSE_MODE=approve` (Goose). The [agent adapters doc](docs/agent-adapters.md#ask-mode-in-strict-mode) has the table.
- In an approved repo, by default Natamux starts each built-in agent with a flag that skips most permission prompts, for example `--permission-mode auto` (Claude Code) and `--force` (Cursor CLI). [The configuration reference](https://natamux.github.io/docs/reference/configuration/#agents) lists the flag of every agent. Only Claude Code checks each action; the others run commands without asking. `"agents": { "permissionMode": "ask" }` in `.natamux.json` turns the flags off. An untrusted repo gets no auto flags. In `ask` mode, a word that would turn approvals back on (for example `--yolo` in `NATAMUX_CODEX_ARGS`, or `--permission-mode bypassPermissions` in a stage's `args`) fails the start with the event `agent.policy-blocked` `{ word, source, reason }`; Natamux never drops the word and starts anyway. A custom agent on base `other` still starts, with a notice that Natamux cannot check its flags. In Strict mode the auto mode is never used, whatever the project says (a save of `permissionMode: auto` is refused), the word check always runs, and an agent on base `other` does not start.
- Title, body and comments of a GitHub issue that you import are marked as data, not as instructions, in the first prompt and in `context.md`. This helps, and it is not a guarantee.
- Each task works in its own git worktree. Natamux pushes, merges or deletes a branch only when you click the button or run the command for it, with these exceptions. For a task that has **Check on CI** ticked (off by default), each CI round runs `git push --force` of a snapshot commit to `natamux-ci/<branch>` on `origin`; the snapshot is the last commit plus changes to tracked files, never untracked files, and Natamux deletes that remote branch when the task ends or is deleted. Before the round, Natamux records what it sends (`stage.ci-approved`: mode, remote without user name or password, branch, commit, tree, files) and pins the commit with a local ref. Right before each network call it checks again that the mode, the remote URL and the tree are unchanged and that the branch starts with `natamux-ci/`; a retry or a restart pushes the same commit. Natamux deletes no remote branch outside `natamux-ci/`. Strict `ciPush` can limit this further: `approved-commit` sends only the last commit, `approved-snapshot` waits until a person approves the exact tree (`POST /tasks/:id/stages/:stage/ci-approve`, desktop token only), and `forbid` makes no CI push and no CI remote call. Natamux also runs `git fetch` when a workspace is made, `git ls-remote` when the New workspace dialog lists remote branches, `git pull --ff-only` before a stage that follows a merged subtask, and `gh issue create` when `tracker.createIssue` is `on-spec-approved` or `on-pr`. These commands use `gh auth git-credential` for github.com. An agent token can call the routes of its own task, which include the push and pull request routes, unless Strict mode holds them back.

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

### Strict mode

Strict mode is for companies with strict security rules. Turn it on in **Settings > Security**. With it off, Natamux works as before.

- The setting lives in `security.json` in the daemon's data folder. Only the daemon writes it, through `PUT /security`, and only with the daemon token: an agent token or a terminal ticket gets 403. `GET /security` shows the state, each key with `{ value, blocked, reason }`, and any warning; task tokens may read it (it holds no secrets). `natamux security [--json]` prints the same.
- `"strict": true` sets these defaults: `ciPush: approved-commit`, `agentAutoMode: forbid`, `terminalInputTrace: off`, repo trust per worktree with `bypassHookTrust: false`, agent permission mode `ask` with `requireAskGuarantee`, and `agentApi: scoped` with `requireHumanForPush`. A key can only be stricter than its Strict default: a looser value in the file is ignored and listed in `ignored`, and `PUT /security` refuses it. A repo's `.natamux.json` can tighten (a `security` object), never loosen. Environment variables never loosen it.
- What each key does: agents start in their ask mode and an agent without one does not start; agent config is approved per worktree and checked again right before the agent runs; agent tokens reach only their own task plus the grants you list; CI gets only the last commit (`approved-commit`), only a tree you approved (`approved-snapshot`, with a dialog in the app and `natamux stage ci-approve`), or nothing (`forbid`); typed terminal input is never traced.
- A missing file means Strict is off. A file that cannot be read turns Strict on with its defaults and shows a warning: once you opted in, a broken file must not turn it off.
- Tamper check: the daemon keeps a copy of the last file it wrote. When `security.json` changes another way (while the daemon runs, or found at start), the daemon keeps the stricter of the old and the new settings, sends `security.changed` and shows a warning in the app (Settings > Security and the status bar's **Strict** item) until you save the settings again.
- The daemon reads the file again at most once per second and at each sensitive decision, so a change applies to the next decision. A running agent keeps the mode it started with until its next start.
- The app shows a **Strict** item in the status bar (its tooltip lists the rules), a note on the Agents page, and turns the **Check on CI** box off with the reason under `ciPush: forbid`.
- Limits: without isolation, an agent runs as your user. It can read the daemon token and call `PUT /security`, or edit the file (the tamper check then keeps the stricter state, but it cannot stop a determined agent that has the token). With isolation (below) the data folder, and so the token and `security.json`, are out of the agent's reach. Strict mode stops mistakes and repo content, not an agent that attacks Natamux on purpose. A person with `sudo`, or with your password, can turn it off. Isolation, network rules and an admin policy that IT sets are not part of Strict mode.

### Isolation (Linux, Claude Code)

Isolation is its own setting, not part of Strict mode: on macOS and Windows no backend exists yet, so Strict mode must not depend on it. Set it in `security.json` and restart Natamux:

```json
{ "isolation": { "mode": "required", "extraReadOnly": [], "extraReadWrite": [], "envAllow": [] } }
```

- `required` works on Linux (x86_64, aarch64) with Landlock (kernel 5.13 or later, `landlock` in `/sys/kernel/security/lsm`), and only for Claude Code. On another OS, without Landlock, or for another agent (also a custom agent based on Claude Code), the shell or agent does not start, and the reason is shown. Nothing ever runs unconfined while isolation is required. A `mode` that is not `off`, or a file that is not JSON, counts as `required`.
- What runs isolated: every task terminal (also a stage's terminal, a resumed agent and the setup script, which run in one), every stage `check` command, and the commit-message agent. Each runs under Landlock with `NO_NEW_PRIVS`. The rules are built in the daemon before the fork; any error (no Landlock, a path that cannot be opened) stops the start before the shell runs.
- Read only: `/usr`, `/bin`, `/sbin`, `/lib*`, `/etc`, `/proc`, `/dev`, the toolchain folders of the login `PATH` (a `.../bin` folder gives its parent, unless that parent is the home folder or holds the data folder), the Natamux bin folder, Claude Code's hook settings, the task's own `<data>/sandbox/<task>` folder (launch scripts, git config) and `extraReadOnly`.
- Read and write: the task's worktree, `<data>/sandbox/<task>/tmp` (`TMPDIR`), `/dev/null`, `/dev/tty`, `/dev/pts`, `/dev/ptmx`, `~/.claude` and `~/.claude.json`, and `extraReadWrite`. `~/.claude*` is where Claude Code keeps its sign-in: that credential stays in the agent's reach.
- Out of reach: the rest of your home folder (SSH keys, cloud credentials, `~/.gitconfig`), the data folder (the daemon token, `security.json`, other tasks' files), other worktrees, `/tmp` and `/run`. Landlock also stops a sandboxed process from reading the environment or memory of a process outside its sandbox, such as the daemon.
- A linked worktree writes its own git folder, the shared `objects`, `refs/heads` and `logs/refs/heads`. The rest of the shared git folder (`config`, `hooks`, `info/exclude`, other worktrees' folders) is read only. Branch refs are shared, so a task can move another task's branch ref; it cannot change its files.
- Environment: an isolated shell keeps only `PATH`, `LANG`, `LC_*`, `TERM`, `SHELL`, `USER`, `LOGNAME`, `HOME`, `TMPDIR`, the task's `NATAMUX_*` variables, `PORT`, the proxy variables, `CLAUDE_*`, `ANTHROPIC_*` and `envAllow`. `GIT_CONFIG_GLOBAL` points to a file Natamux writes with your git name and email and an empty `credential.helper`. A command or the commit agent never gets a task token.
- The shell is interactive (`-i`), not a login shell: profiles in the home folder are out of reach.
- Cleanup: a closed terminal gets SIGHUP, then SIGKILL for its process tree, then every process that still carries the shell's own mark in its environment is killed too (a program that left the tree with `setsid`). Once the last terminal of a task ends, its token stops working; the next shell gets a new one.
- The daemon's own git hardens itself while isolation is on: no hooks (`core.hooksPath` set to an empty folder), `core.fsmonitor=false`, `protocol.ext.allow=never` and `GIT_CONFIG_NOSYSTEM=1`. An agent cannot get a command run outside its sandbox through a git setting.
- `GET /security` shows `isolation { mode, backend, abi, ok, reason }` for this daemon run.

### Network and credentials

The `network` key of `security.json` decides where Natamux's own calls go (git remotes, `gh`, the updater) and what credentials the programs it starts keep. Egress is its own setting, not set by Strict mode; `credentials` is. A save in Settings > Security keeps the file's `network` key.

```json
{ "network": {
    "egress": "open | allowlist | loopback",
    "git": [{ "host": "github.com", "owner": "acme", "repo": "*", "actions": ["fetch", "push"] }],
    "gh": [{ "host": "github.com", "actions": ["read", "pr-create"] }],
    "updater": true,
    "proxy": { "url": "", "noProxy": [] },
    "credentials": { "agentEnv": "strip | inherit", "stripExtra": [] } } }
```

- Egress: offline mode (`NATAMUX_OFFLINE=1` or the machine `offline` key) is always `loopback`. Otherwise the file's value. A repo's `.natamux.json` (`security.network.egress`) can only move it from `open` to `allowlist` to `loopback`. An unknown value counts as `loopback`.
- `open`: the calls of today.
- `loopback`: no `git fetch`, `pull`, `push`, `ls-remote`, no `gh`, no update check, no pull request watcher, no resumed CI gate.
- `allowlist`: before a remote call, the daemon asks git for the URL it would really use (`git remote get-url`, so `insteadOf` and `pushInsteadOf` rules are applied), refuses more than one push URL or a URL that git would rewrite again, checks it against `network.git`, and runs git with that URL, not the remote name. A change of the remote after the check cannot move the call. The git of that call takes only `https`, `ssh` and `file` (`GIT_ALLOW_PROTOCOL`), runs no hook (`core.hooksPath=/dev/null`), and asks no credential helper but `gh auth git-credential`. Any other network git call is refused. Actions: `fetch` (also `ls-remote` and `pull`), `push`, `push-force` (`--force`, `+refspec`; the CI push needs it) and `delete`. A host is exact or `*.name`; owner and repo are exact or `*`. A folder or `file://` remote is local and always allowed. `ext::` and other helper forms, `http://`, `git://` and a URL with a password are refused. An empty list, or a list with one bad entry, refuses everything.
- `gh` under `allowlist`: only calls Natamux knows (`read`: `pr view/list/checks`, `run list/view`, GET `gh api`, GraphQL queries; `pr-create`; `issue-create`), on a host of `network.gh`. gh gets `GH_HOST` and `GH_REPO` pinned to the checked host and origin.
- Updater: `updater: false` turns update checks off in every mode. Under `allowlist` the host must be listed (`updater: true` lists GitHub's release hosts; a list of hosts replaces them), curl follows no redirect by itself, and the host of each redirect is checked before the next request. curl always gets `--disable` (no `~/.curlrc`), and `--proxy` when a proxy is set.
- Credentials: with `agentEnv: strip` (the Strict default) task terminals, stage `check` commands and the commit-message agent lose `GH_TOKEN`, `GITHUB_TOKEN`, `GH_ENTERPRISE_TOKEN`, `GITHUB_ENTERPRISE_TOKEN`, `GITLAB_TOKEN`, `GIT_ASKPASS`, `SSH_ASKPASS`, `SSH_AUTH_SOCK`, `GIT_SSH`, `GIT_SSH_COMMAND`, `GIT_CREDENTIAL_*` and `stripExtra`. They get an empty `GH_CONFIG_DIR`, `GIT_TERMINAL_PROMPT=0`, `GCM_INTERACTIVE=never` and an empty `credential.helper`. Model keys (`ANTHROPIC_*` and the like) stay. Under Strict, `inherit` is ignored. `git push` or `gh` typed in such a terminal then has no login: the daemon's own push still works.
- Proxy: with `proxy.url` set, those programs get `HTTP_PROXY`, `HTTPS_PROXY` and `ALL_PROXY` (both cases), and `NO_PROXY` becomes loopback plus `noProxy`.
- With isolation on, egress not `open` and Landlock ABI 4 or later, an isolated process can open TCP only to the daemon port and the proxy port. Landlock checks ports, not addresses. Otherwise `GET /security` says `child processes not confined` in `network.note`.
- A refusal starts with `egress policy:` and is logged as one line with the host and the action, never a URL with a password.
- Changes apply to the next call or the next terminal. `GET /security` shows `network { egress, updater, credentials, proxy, confined, note }`.

## Known limits

- Isolation (above) does not cover macOS or Windows, or agents other than Claude Code. Without it an agent runs with all your rights.
- Network rules hold Natamux's own git, gh and update calls. Other traffic of an agent is held only with isolation, egress `allowlist` or `loopback` and Landlock ABI 4 (TCP ports, see above); UDP is not held, and with egress `open` an isolated agent can still send what it can read (the worktree, `~/.claude`) anywhere. Blocking it for every program needs a firewall or a proxy. Landlock before ABI 6 also does not stop a connection to a unix socket by path (for example a Docker or ssh-agent socket whose path it knows); the socket's own permissions still apply.
- `NO_NEW_PRIVS` means `sudo` and other setuid programs do not gain rights in an isolated shell.
- The cleanup sweep is best effort: a process that clears its own environment is not found. Killing every process of a task for sure needs a PID namespace or a cgroup, which need root.
- Isolation follows the paths as they are at the shell's start. A folder made later in a granted place is covered; a new toolchain folder on the `PATH` needs a new shell.
- Claude Code writes `~/.claude.json` by replacing it through a new file in your home folder, which an isolated shell cannot make. Sign in to Claude Code once outside Natamux first.
- An agent runs as your user. It can read your files, including the daemon token file, and the daemon's environment. The agent token and the gate rules stop an agent that follows the API, for example one that follows bad instructions from a web page or an issue. They do not stop one that looks for the daemon token. Keeping the token in the system keychain, or running agents as another user, is future work.
- In `auto` mode (the default in an approved repo) agents run commands without asking you, apart from Claude Code's own checks. `ask` keeps each agent's prompts. Without isolation Natamux does not sandbox either mode.
- Repo trust covers Natamux's own commands and the trust it gives agents. It does not control what an agent does inside its worktree: it can run any command there with your rights, also in `ask` mode if you allow it.
- A task terminal on Windows types the command stage ticket into that terminal, so the ticket shows there.

## What is out of scope

- What an agent does with the rights you give it.
- An attacker who already runs code as your user, or who can read your files.
- Running the daemon on a network address without TLS. Put a reverse proxy with TLS in front of it if you must.
