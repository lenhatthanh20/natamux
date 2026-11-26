---
title: For a security review
description: What a company needs to know before it allows Natamux. What it connects to, how to turn that off, what it is built from, and how to check a download.
sidebar:
  order: 4
---

This page is for the person who reviews Natamux before a company installs it. It lists facts and how to check them. [Security and privacy](/docs/help/security-and-privacy/) explains the protections in more detail.

## What Natamux is

- A desktop app, a local daemon and a `natamux` command. All three run as the user, on the user's machine. There is no Natamux server and no account.
- It starts coding agents (Claude Code, Codex, and others) in terminals. **Natamux does not sandbox agents.** An agent can read the user's files and sends prompts and code to its own vendor. What an agent may do is set in the agent, not in Natamux. Review each agent you allow as its own product.
- Natamux has no telemetry, no crash reports and no analytics. It installs no crash handler, so a crash dump follows the policy of the operating system (for example Windows Error Reporting or the macOS crash reporter), which your IT team sets.
- In Strict mode, programs Natamux starts for a task get `DO_NOT_TRACK=1` and `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1`, unless the user already set them. Nothing else is set. What an agent sends is still the agent's and the network's job. See [PRIVACY.md](https://github.com/lenhatthanh20/natamux/blob/main/PRIVACY.md#agent-opt-out-in-strict-mode).

## Network connections

### Made by Natamux itself

| Connection | When | What is sent |
|---|---|---|
| The app, the daemon and the `natamux` command talk to each other on `127.0.0.1` | By default. The daemon refuses to start when `NATAMUX_HOST` is not loopback (`127.0.0.1`, `localhost`, `::1`) unless `NATAMUX_ALLOW_REMOTE_BIND` is exactly `1`; a non-wildcard value joins the Host allow-list. The Docker image sets both inside its container; `compose.prod.yaml` publishes on `${NATAMUX_BIND:-127.0.0.1}`. `NATAMUX_URL` must start with `http://` (`https://` is refused) and can name any host: the token then goes there in clear text | Local only, unless you change the address |
| Update check: `https://github.com/lenhatthanh20/natamux/releases/...`, through the system `curl` | Only when **Check for Updates Automatically** is on (off by default), or when the user picks **Check for Updates…** | The app version in the user agent. GitHub sees the IP address |
| Update download from the same release, with redirects to GitHub's file hosts | After an update check finds a signed, newer release | Same |

Natamux links no TLS library. The app starts the system `curl`, found through `PATH`, which does the HTTPS. It gets `--disable`, so it does not read `~/.curlrc`; it reads the proxy variables (or `--proxy` from the network settings), uses the certificate store of the system, and follows redirects from github.com to GitHub's file hosts. Under egress `allowlist` it follows no redirect by itself: the app checks each redirect's host first. A check sends a HEAD request; when a newer release exists, the app also fetches `SHA256SUMS.txt`, `SHA256SUMS.txt.minisig`, then the file. Timed checks run about 30 seconds after the start and every 6 hours, only when the switch is on. The switch is in the app menu (macOS), the **Help** menu (Linux and Windows) and **Settings → General → Updates**.

### Made by programs that Natamux starts on its own

| Program | When | Reaches |
|---|---|---|
| `gh` (GitHub CLI), with the user's own login | While a dashboard or the app is connected and a task has an open or draft pull request or a pushed branch: `gh api graphql` every 60 seconds while checks are pending or a pushed branch has no pull request, else every 5 minutes. `gh pr list` and `gh pr view` when an agent stops | GitHub, or your GitHub Enterprise host |
| Coding agents | When the daemon starts, it starts again the agents of tasks in running, idle, needs-input or review state | The agent's vendor |
| `git push --force` and `gh run list` for the CI gate | Only for a task with **Check on CI** ticked (off by default) on a stage with `"ci": true`. Each CI round force-pushes a snapshot commit to `natamux-ci/<branch>` on `origin`: the last commit plus changes to tracked files, never untracked files. The commit, its tree and the remote are recorded (`stage.ci-approved`) before the round and checked again before each network call. The push goes to that one URL and is refused when `origin` has more than one push URL. Natamux polls `gh run list` every 15 seconds for up to 30 minutes, and when the task ends or is deleted it deletes the `natamux-ci/` branch of every branch a round pushed. No click per round | GitHub |
| `git fetch` | When a workspace is made, for the base branch | The git remote |
| `git ls-remote` | When the New workspace dialog lists remote branches | The git remote |
| `git pull --ff-only` | Before a stage that follows a merged subtask | The git remote |
| `gh issue create` | When `tracker.createIssue` is `on-spec-approved` or `on-pr` (default `manual`) | GitHub |
| `gh issue view` | For a task whose context source is `github-issue` | GitHub |

Git pushes and the git commands above use `gh auth git-credential` as the credential helper for github.com.

### Made by the install scripts

| Script | Connects to | When |
|---|---|---|
| `install.sh` (macOS, Debian, Ubuntu) | `github.com` (the newest tag, then the release files), redirected to GitHub's file hosts | Once per install |
| `install.ps1` (Windows) | `api.github.com` for the newest tag (not when `NATAMUX_VERSION` is set), then `github.com` for the release files, redirected to GitHub's file hosts | Once per install |

They send no data about the user or the machine beyond what any download sends. `NATAMUX_VERSION=v1.1.0` installs that release and not the newest one (see [Install](/docs/getting-started/install/#install-a-chosen-version)).

### Made when the user asks

`git push` from the **Push** button, `gh pr create`, issue import, links that open in the browser, and anything typed in a terminal.

**Draft commit message** (a click, never automatic) runs the task's agent once with the task title, the list of changed files and up to 60,000 characters of the uncommitted diff. That text goes to the agent's vendor. It never runs in a repo that is not trusted.

## Offline mode

Offline mode is off by default: without it Natamux cannot open a pull request or read its status. It is on when `NATAMUX_OFFLINE` is exactly `1`, or when `"offline": true` is set in one of two settings files. It is read once at start: restart to change it. The settings file of the machine also holds two more keys, listed [below](#settings-file-of-the-machine). Turn offline mode on in one of three ways:

| Where | Who sets it |
|---|---|
| The settings file of the machine, with `{ "offline": true }`: `/Library/Application Support/Natamux/settings.json` on macOS, `/etc/natamux/settings.json` on Linux, `%ProgramData%\Natamux\settings.json` on Windows | An administrator. A user cannot turn it off again: the app, the daemon and the `natamux` command all read this file. On Windows the file is also read at the fixed path `C:\ProgramData\Natamux\settings.json`, so changing the `ProgramData` variable does not help |
| `"offline": true` in `settings.json` of the app's user data folder | The user |
| `NATAMUX_OFFLINE=1` in the environment of the app or the daemon | The user, or a start script |

Any one of them turns offline mode on. None of them can turn it off when another one is on. The user's own `settings.json` reaches the daemon only through the app that starts it; the machine file and the variable are read by every program. With offline mode on:

- no update check and no update download. The manual **Check for Updates…** is refused too;
- no `gh` call: no pull request status, no pull request watcher, no issue import, no pull request from the app, no issue context from GitHub;
- no `git fetch`, `pull`, `push`, `ls-remote` or `clone` from Natamux. Local git work (worktrees, commits, diffs) is unchanged;
- the CI gate is not resumed. A CI stage started offline ends with an error and waits for the user. A CI branch pushed before is not deleted.

Offline mode covers Natamux's own code and the programs it starts by itself. It does not limit coding agents or what a user types in a terminal. Limit those with the agent's own settings and with your network rules.

### Settings file of the machine

An administrator sets these keys for every user in the settings file of the machine. Only the exact JSON value counts: any other value, or a file that does not parse, means "not set".

| Key | Value | Effect |
|---|---|---|
| `offline` | `true` | Offline mode (above). Read once at start |
| `strict` | `true` | Strict mode for every user. `security.json` cannot turn it off, and `PUT /security` with Strict off is refused with "Strict mode is set by your administrator". `GET /security` shows `machineStrict: true`. Read about once per second: no restart |
| `updates` | `false` | No update check and no update download, timed or manual. `git` and `gh` still work. The app shows "Updates are turned off by your administrator". Read at each check |

| System | Path |
|---|---|
| macOS | `/Library/Application Support/Natamux/settings.json` |
| Linux | `/etc/natamux/settings.json` |
| Windows | `C:\ProgramData\Natamux\settings.json`, and `%ProgramData%\Natamux\settings.json` when that variable names another folder. A key is on when either file turns it on |

`strict` stops a user, or an agent that runs as the user, from rewriting `security.json`. It does not stop a person with administrator rights, who can edit the machine file.

## Network settings

The `network` key of `security.json` (the daemon's data folder) gives finer control than offline mode: egress `open`, `allowlist` (only the git remotes and `gh` hosts you list, each URL resolved and pinned before git runs) or `loopback` (as offline mode, read at each call; under both, and offline, git fetches no missing object of a partial clone and LFS files stay pointer files), a proxy, updates off, and `credentials.agentEnv: strip`, which takes `GH_TOKEN`, `SSH_AUTH_SOCK`, `GIT_ASKPASS` and the like out of task terminals, checks and the commit-message agent. Strict mode turns `strip` on. Other traffic of agents is held only with isolation on Linux and Landlock ABI 4 (TCP to the daemon and proxy ports); otherwise use a firewall or proxy. `GET /security` shows the state in `network`. The full rules are in [SECURITY.md](https://github.com/lenhatthanh20/natamux/blob/main/SECURITY.md#network-and-credentials).

## What a release is built from

- From v1.0.31, every release has one SBOM per platform, in CycloneDX 1.5 JSON: `Natamux-mac-arm64.cdx.json`, `Natamux-mac-x64.cdx.json`, `Natamux-win-x64.cdx.json` and `Natamux-linux-x64.cdx.json`. Each lists the third-party Rust crates that the app, the daemon and the CLI build with for that platform, without dev dependencies: name, version, license when the crate gives one, the SHA-256 for crates.io crates, and for a git crate the fork and commit in its package URL. Each SBOM file has a build provenance attestation.
- The bundled SQLite (compiled in through `rusqlite` "bundled") is its own component, `pkg:generic/sqlite@<version>`, so a scanner sees the C code too. `scripts/sbom.py` takes the version from a table keyed by the `libsqlite3-sys` version and fails on a version it does not know. The SBOM metadata names the `rustc` version.
- The SBOM files are in `SHA256SUMS.txt`, so the release signature covers them.
- Releases before v1.0.31 have no SBOM.
- Each platform build also ships `Natamux-<platform>.buildenv.txt` (`Natamux-mac-arm64`, `Natamux-mac-x64`, `Natamux-win-x64`, `Natamux-linux-x64`): `rustc -vV`, `cargo -V`, the runner image name and version, and on Linux the `dpkg-query` list of the build packages installed. It is in `SHA256SUMS.txt`, so it is signed, and it has a build provenance attestation.
- The licence of every group that ships is on the [Licensing](/docs/help/licensing/) page.
- License texts are in `THIRD-PARTY-RUST.md` and `THIRD-PARTY-NOTICES.md`, in the repository and in every package. `THIRD-PARTY-RUST.md` is made by `cargo about` without dev dependencies and without the Zed crates, which `THIRD-PARTY-NOTICES.md` lists by hand. Together they list every third-party crate that a shipped build uses, not every package of `Cargo.lock`. The `supply-chain` job makes `THIRD-PARTY-RUST.md` again with `cargo about` (pinned, a CI tool and not a dependency of the app) and fails when the file differs. `deny.toml` and `about.toml` hold the same 13 allowed licence ids; a test checks it.

## How the dependencies are controlled

- Versions are locked (`Cargo.lock`), and every build uses the lock.
- Crates may come only from crates.io and from forks under `github.com/lenhatthanh20` (`deny.toml`), each pinned to one commit on a branch that cannot be changed or deleted. The Zed fork is frozen: there is no monthly upgrade.
- When the lock, `deny.toml`, `supply-chain/`, `.cargo/audit.toml` or `scripts/audit.sh` change, the `supply-chain` job of `rust.yml` runs `cargo deny`, `scripts/audit.sh` (`cargo audit`, and again for the git crates) and `cargo vet`. The OSV check for malicious packages and the Zed advisory check run only in the daily `advisories.yml`.
- `.cargo/audit.toml` holds only the two vulnerability ids of the nine ignores in `deny.toml`; the other seven are unmaintained notices, which `cargo audit` only warns about. `scripts/zed-lock-check.py` requires `Cargo.lock` to hold exactly the crates of `scripts/zed-lock-crates.txt`.
- `cargo vet` imports nine organizations and records which crates have an audit from Mozilla, Google, the Bytecode Alliance, Embark, Fermyon, ISRG, Zcash, actix or Ariel OS. **Most crates have no such audit**: `supply-chain/config.toml` lists 362 exemptions, and `Cargo.lock` holds 647 packages. A new crate with no audit and no exemption fails `cargo vet`. An advisory that cannot be fixed yet is listed in `deny.toml` with its reason and a review date, at most 90 days away (`scripts/advisory-dates.py` fails past that). The target for a fix of a reachable vulnerability is 14 days, best effort (see [SECURITY.md](https://github.com/lenhatthanh20/natamux/blob/main/SECURITY.md#dependency-advisories)).
- Dependabot waits 7 days before it opens a pull request for a new version. The maintainer can take a version by hand inside that time; `notify` 9.0.0-rc.9 was taken so (`Cargo.toml`).
- The `main` ruleset blocks delete and force-push and requires the status check `rust-ok` to merge. No review is required: one maintainer. The tag ruleset "Release tags" covers `v*`: an existing tag cannot be deleted or moved, and a new tag can be created. Secret scanning and its push protection are on.
- A release build is clean: a run that publishes restores no compiled `target` folder from the cache, only the cargo download folders that `Cargo.lock` checksums. The runners have fixed labels (`ubuntu-24.04`, `windows-2025`, and fixed macOS labels), not `-latest`. Before the build, the release workflow runs `scripts/advisory-dates.py` and `scripts/audit.sh`; a passed review date or a new advisory that is not ignored stops the release.
- Release builds run on GitHub Actions, never on a developer machine. `release.yml` runs only for `v*` tags and by hand, never on pull requests. Each of the six download files and the SBOM files has a build provenance attestation; `SHA256SUMS.txt` has the minisign signature instead. CI holds the macOS signing certificate as secrets (`MACOS_SIGN_P12`, `MACOS_SIGN_P12_PASSWORD`); the minisign key and the tag key are not in CI.
- The Linux release build starts the app and its daemon for about 12 seconds under `strace` on `connect`, `sendto`, `sendmsg` and `sendmmsg`, and fails if they reach past `127.0.0.1` or look up a host name. It runs twice: with default settings, and with the update check turned on in the user's settings plus `NATAMUX_OFFLINE=1` (it must still show no connection past loopback). The same check runs on a pull request that changes the app, daemon or update code. It covers the start only, not `gh` and `git` calls.
- A source test fails when the app or UI code draws an image from a URL that starts with `http`, or sets an HTTP client.
- `NATAMUX_VERSION` makes `install.sh` and `install.ps1` install the release the company reviewed.

## Where data is kept

- The app folder holds the token, prompts, saved screens, logs and worktrees. On Windows a new install keeps it in `%LOCALAPPDATA%\Natamux`, which a roaming profile does not copy. An older install that has `%APPDATA%\Natamux` and no `%LOCALAPPDATA%\Natamux\data` keeps using the old folder. To move it, see [Where things are](/docs/guides/desktop-app/#where-things-are).
- The first prompt of a task is also on the agent's command line. On a shared Linux host other local accounts can read it in `/proc` unless `/proc` is mounted with `hidepid=2`. File mode `0600` on the launch script does not hide it.
- Retention is off by default (`data.retentionDays` is `0`): finished tasks stay until the user deletes them. Set it in `security.json`.
- Which agent programs a user may run is set by the company's software and network rules. A Natamux terminal is a plain shell, so a list of agents inside Natamux would stop nothing.
- Every path, how private it is and how it goes away: [data inventory](https://github.com/lenhatthanh20/natamux/blob/main/docs/data-inventory.md).

## How to check a download

1. The signature of `SHA256SUMS.txt` and the hash of the file: [Verify the download](/docs/getting-started/install/#verify-the-download). The signing key stays offline with the maintainer.
2. That GitHub Actions built the file from this repository:

   ```sh
   gh attestation verify Natamux-mac-arm64.dmg --repo lenhatthanh20/natamux
   ```

## What is missing today

- The macOS app is signed with a self-made certificate, not an Apple Developer ID, and is not notarized. The Windows installer has no Authenticode signature. The Linux packages are not signed. See [The desktop app is not notarized](/docs/help/security-and-privacy/#the-desktop-app-is-not-notarized).
- A signature proves that the files equal what CI built, not that the build is clean. The build is not bit-for-bit reproducible: the same commit can give files with other hashes. The `buildenv.txt` file records the tools that were used instead. The apt packages in CI are not pinned to versions.
- There is one release signing key and no second key. See [SECURITY.md](https://github.com/lenhatthanh20/natamux/blob/main/SECURITY.md) for the plan if it leaks.
- The install script removes the macOS quarantine flag, and checks the signature of `SHA256SUMS.txt` only when `minisign` is installed; otherwise it warns and checks the hash alone.
- No second person reviews a change.
- One person maintains Natamux and its copy of the UI toolkit. Only the latest release gets security fixes.
- No independent security test has been done.
- Agents are not sandboxed, and an agent that runs as the user can read the daemon's token file. See [Known limits](/docs/help/security-and-privacy/#known-limits).
