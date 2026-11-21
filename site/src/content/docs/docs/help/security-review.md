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
- Natamux has no telemetry, no crash reports and no analytics.

## Network connections

### Made by Natamux itself

| Connection | When | What is sent |
|---|---|---|
| The app, the daemon and the `natamux` command talk to each other on `127.0.0.1` | By default. `NATAMUX_HOST` is not checked: any address works, including `0.0.0.0`, and a non-wildcard value joins the Host allow-list. The Docker image sets `0.0.0.0` inside its container; `compose.prod.yaml` publishes on `${NATAMUX_BIND:-127.0.0.1}`. `NATAMUX_URL` must start with `http://` (`https://` is refused) and can name any host: the token then goes there in clear text | Local only, unless you change the address |
| Update check: `https://github.com/lenhatthanh20/natamux/releases/...`, through the system `curl` | Only when **Check for Updates Automatically** is on (off by default), or when the user picks **Check for Updates…** | The app version in the user agent. GitHub sees the IP address |
| Update download from the same release, with redirects to GitHub's file hosts | After an update check finds a signed, newer release | Same |

Natamux links no TLS library. The app starts the system `curl`, found through `PATH`, which does the HTTPS. It reads `~/.curlrc` and the proxy variables, uses the certificate store of the system, and follows redirects from github.com to GitHub's file hosts. A check sends a HEAD request; when a newer release exists, the app also fetches `SHA256SUMS.txt`, `SHA256SUMS.txt.minisig`, then the file. Timed checks run about 30 seconds after the start and every 6 hours, only when the switch is on. The switch is in the app menu (macOS), the **Help** menu (Linux and Windows) and **Settings → General → Updates**.

### Made by programs that Natamux starts on its own

| Program | When | Reaches |
|---|---|---|
| `gh` (GitHub CLI), with the user's own login | While a dashboard or the app is connected and a task has an open or draft pull request or a pushed branch: `gh api graphql` every 60 seconds while checks are pending or a pushed branch has no pull request, else every 5 minutes. `gh pr list` and `gh pr view` when an agent stops | GitHub, or your GitHub Enterprise host |
| Coding agents | When the daemon starts, it starts again the agents of tasks in running, idle, needs-input or review state | The agent's vendor |
| `git push --force` and `gh run list` for the CI gate | Only for a task with **Check on CI** ticked (off by default) on a stage with `"ci": true`. Each CI round force-pushes a snapshot commit to `natamux-ci/<branch>` on `origin`; the snapshot holds every file git does not ignore, also uncommitted ones. Natamux polls `gh run list` every 15 seconds for up to 30 minutes, and deletes that remote branch when the task ends or is deleted. No click per round | GitHub |
| `git fetch` | When a workspace is made, for the base branch | The git remote |
| `git ls-remote` | When the New workspace dialog lists remote branches | The git remote |
| `git pull --ff-only` | Before a stage that follows a merged subtask | The git remote |
| `gh issue create` | When `tracker.createIssue` is `on-spec-approved` or `on-pr` (default `manual`) | GitHub |
| `gh issue view` | For a task whose context source is `github-issue` | GitHub |

Git pushes and the git commands above use `gh auth git-credential` as the credential helper for github.com.

### Made when the user asks

`git push` from the **Push** button, `gh pr create`, issue import, links that open in the browser, and anything typed in a terminal.

## Offline mode

Offline mode is off by default: without it Natamux cannot open a pull request or read its status. It is on when `NATAMUX_OFFLINE` is exactly `1`, or when `"offline": true` is set in one of two settings files. It is read once at start: restart to change it. Turn it on in one of three ways:

| Where | Who sets it |
|---|---|
| The settings file of the machine, with `{ "offline": true }`: `/Library/Application Support/Natamux/settings.json` on macOS, `/etc/natamux/settings.json` on Linux, `%ProgramData%\Natamux\settings.json` on Windows | An administrator. A user cannot turn it off again: the app, the daemon and the `natamux` command all read this file |
| `"offline": true` in `settings.json` of the app's user data folder | The user |
| `NATAMUX_OFFLINE=1` in the environment of the app or the daemon | The user, or a start script |

Any one of them turns offline mode on. None of them can turn it off when another one is on. The user's own `settings.json` reaches the daemon only through the app that starts it; the machine file and the variable are read by every program. With offline mode on:

- no update check and no update download. The manual **Check for Updates…** is refused too;
- no `gh` call: no pull request status, no pull request watcher, no issue import, no pull request from the app, no issue context from GitHub;
- no `git fetch`, `pull`, `push`, `ls-remote` or `clone` from Natamux. Local git work (worktrees, commits, diffs) is unchanged;
- the CI gate is not resumed. A CI stage started offline ends with an error and waits for the user. A CI branch pushed before is not deleted.

Offline mode covers Natamux's own code and the programs it starts by itself. It does not limit coding agents or what a user types in a terminal. Limit those with the agent's own settings and with your network rules.

## What a release is built from

- From the next release, every release has one SBOM per platform, in CycloneDX 1.5 JSON: `Natamux-mac-arm64.cdx.json`, `Natamux-mac-x64.cdx.json`, `Natamux-win-x64.cdx.json` and `Natamux-linux-x64.cdx.json`. Each lists the third-party Rust crates that the app, the daemon and the CLI build with for that platform, without dev dependencies: name, version, license when the crate gives one, the SHA-256 for crates.io crates, and for a git crate the fork and commit in its package URL. Each SBOM file has a build provenance attestation.
- The SBOM files are in `SHA256SUMS.txt`, so the release signature covers them.
- The latest published release (v1.0.30) has no SBOM yet. SBOM files start with the first release after this change.
- License texts are in `THIRD-PARTY-RUST.md` and `THIRD-PARTY-NOTICES.md`, in the repository and in every package. `THIRD-PARTY-RUST.md` is made by `cargo about` without dev dependencies and without the Zed crates, which `THIRD-PARTY-NOTICES.md` lists by hand. Together they list every third-party crate that a shipped build uses, not every package of `Cargo.lock`.

## How the dependencies are controlled

- Versions are locked (`Cargo.lock`), and every build uses the lock.
- Crates may come only from crates.io and from forks under `github.com/lenhatthanh20` (`deny.toml`), each pinned to one commit on a branch that cannot be changed or deleted. The Zed fork is frozen: there is no monthly upgrade.
- When the lock, `deny.toml`, `supply-chain/`, `.cargo/audit.toml` or `scripts/audit.sh` change, the `supply-chain` job of `rust.yml` runs `cargo deny`, `scripts/audit.sh` (`cargo audit`, and again for the git crates) and `cargo vet`. The OSV check for malicious packages and the Zed advisory check run only in the daily `advisories.yml`.
- `.cargo/audit.toml` holds only the two vulnerability ids of the nine ignores in `deny.toml`; the other seven are unmaintained notices, which `cargo audit` only warns about. `scripts/zed-lock-check.py` requires `Cargo.lock` to hold exactly the crates of `scripts/zed-lock-crates.txt`.
- `cargo vet` imports nine organizations and records which crates have an audit from Mozilla, Google, the Bytecode Alliance, Embark, Fermyon, ISRG, Zcash, actix or Ariel OS. **Most crates have no such audit**; they are listed as exemptions in `supply-chain/config.toml`. An advisory that cannot be fixed yet is listed in `deny.toml` with its reason and a review date.
- Dependabot waits 7 days before it opens a pull request for a new version. The maintainer can take a version by hand inside that time; `notify` 9.0.0-rc.9 was taken so (`Cargo.toml`).
- The `main` ruleset blocks delete and force-push and requires the status check `rust-ok` to merge. No review is required: one maintainer. The tag ruleset "Release tags" covers `v*`: an existing tag cannot be deleted or moved, and a new tag can be created. Secret scanning and its push protection are on.
- Release builds run on GitHub Actions, never on a developer machine. `release.yml` runs only for `v*` tags and by hand, never on pull requests. Each of the six download files and the SBOM files has a build provenance attestation; `SHA256SUMS.txt` has the minisign signature instead. CI holds the macOS signing certificate as secrets (`MACOS_SIGN_P12`, `MACOS_SIGN_P12_PASSWORD`); the minisign key and the tag key are not in CI.
- The Linux release build starts the app and its daemon for about 12 seconds under `strace` on `connect`, `sendto` and `sendmsg`, and fails if they reach past `127.0.0.1` or look up a host name. It checks the start only, not updates or `gh` and `git` calls.

## How to check a download

1. The signature of `SHA256SUMS.txt` and the hash of the file: [Verify the download](/docs/getting-started/install/#verify-the-download). The signing key stays offline with the maintainer.
2. That GitHub Actions built the file from this repository:

   ```sh
   gh attestation verify Natamux-mac-arm64.dmg --repo lenhatthanh20/natamux
   ```

## What is missing today

- The macOS app is signed with a self-made certificate, not an Apple Developer ID, and is not notarized. The Windows installer has no Authenticode signature. The Linux packages are not signed. See [The desktop app is not notarized](/docs/help/security-and-privacy/#the-desktop-app-is-not-notarized).
- A signature proves that the files equal what CI built, not that the build is clean.
- There is one release signing key and no second key. See [SECURITY.md](https://github.com/lenhatthanh20/natamux/blob/main/SECURITY.md) for the plan if it leaks.
- The install script removes the macOS quarantine flag, and checks the signature of `SHA256SUMS.txt` only when `minisign` is installed; otherwise it warns and checks the hash alone.
- No second person reviews a change.
- One person maintains Natamux and its copy of the UI toolkit. Only the latest release gets security fixes.
- No independent security test has been done.
- Agents are not sandboxed, and an agent that runs as the user can read the daemon's token file. See [Known limits](/docs/help/security-and-privacy/#known-limits).
