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
| The app, the daemon and the `natamux` command talk to each other on `127.0.0.1` | Always | Local only |
| Update check: `https://github.com/lenhatthanh20/natamux/releases/...`, through the system `curl` | Only when **Check for Updates Automatically** is on (off by default), or when the user picks **Check for Updates…** | The app version in the user agent. GitHub sees the IP address |
| Update download from the same release, with redirects to GitHub's file hosts | After an update check finds a signed, newer release | Same |

Natamux links no TLS library. `curl` does the HTTPS, with the proxy settings and the certificate store of the system.

### Made by programs that Natamux starts on its own

| Program | When | Reaches |
|---|---|---|
| `gh` (GitHub CLI), with the user's own login | While the app is open and a task has an open pull request or a pushed branch: every 1 to 5 minutes. Also when an agent stops | GitHub, or your GitHub Enterprise host |
| Coding agents | When the daemon starts, it starts again the agents of unfinished tasks | The agent's vendor |
| `gh` and `git push` for the CI gate | Only in a project that sets `"ci": true` on a stage | GitHub |
| `git pull --ff-only` | When a stage starts after a subtask was merged | The git remote |

### Made when the user asks

`git fetch`, `git push`, `git ls-remote`, `gh pr create`, `gh issue ...`, links that open in the browser, and anything typed in a terminal.

## Offline mode

Set `NATAMUX_OFFLINE=1` for the app or the daemon, or put `"offline": true` in `settings.json` of the app's user data folder. Then:

- no update check and no update download;
- no `gh` call: no pull request status, no issue import, no pull request from the app;
- no `git fetch`, `pull`, `push`, `ls-remote` or `clone` from Natamux. Local git work (worktrees, commits, diffs) is unchanged;
- the CI gate does not run.

Offline mode covers Natamux's own code and the programs it starts by itself. It does not limit coding agents or what a user types in a terminal. Limit those with the agent's own settings and with your network rules.

## What a release is built from

- Every release has one SBOM per platform, in CycloneDX 1.5 JSON: `Natamux-mac-arm64.cdx.json`, `Natamux-mac-x64.cdx.json`, `Natamux-win-x64.cdx.json` and `Natamux-linux-x64.cdx.json`. Each lists the third-party Rust crates in that build, with version, license, SHA-256 and source.
- The SBOM files are in `SHA256SUMS.txt`, so the release signature covers them.
- License texts are in `THIRD-PARTY-RUST.md` and `THIRD-PARTY-NOTICES.md`, in the repository and in every package.

## How the dependencies are controlled

- Versions are locked (`Cargo.lock`), and every build uses the lock.
- Crates may come only from crates.io and from forks that the maintainer owns, each pinned to one commit on a branch that cannot be changed or deleted.
- Every day, and on every dependency change, CI checks the lock against the RustSec advisory database (`cargo deny`, `cargo audit`) and against the OSV list of malicious packages.
- `cargo vet` records which crates have an audit from Mozilla, Google, the Bytecode Alliance, Embark, Fermyon, ISRG, Zcash, actix or Ariel OS. **Most crates have no such audit**; they are listed as exemptions in `supply-chain/config.toml`. An advisory that cannot be fixed yet is listed in `deny.toml` with its reason and a review date.
- A new version of a dependency is taken only after it has been public for 7 days.
- Release builds run on GitHub Actions, never on a developer machine. Each release file has a build provenance attestation.
- On Linux, the release build starts the app with every network call traced, and fails if the app or its daemon reaches past `127.0.0.1` or looks up a host name.

## How to check a download

1. The signature of `SHA256SUMS.txt` and the hash of the file: [Verify the download](/docs/getting-started/install/#verify-the-download). The signing key stays offline with the maintainer.
2. That GitHub Actions built the file from this repository:

   ```sh
   gh attestation verify Natamux-mac-arm64.dmg --repo lenhatthanh20/natamux
   ```

## What is missing today

- The macOS app is not signed with an Apple Developer ID and is not notarized. The Windows installer has no Authenticode signature. The Linux packages are not signed. See [The desktop app is not notarized](/docs/help/security-and-privacy/#the-desktop-app-is-not-notarized).
- One person maintains Natamux and its copy of the UI toolkit. Only the latest release gets security fixes.
- No independent security test has been done.
- Agents are not sandboxed, and an agent that runs as the user can read the daemon's token file. See [Known limits](/docs/help/security-and-privacy/#known-limits).
