# Privacy

Natamux runs on your machine. It has no account, no server of its own, and no telemetry.

## What Natamux collects

Nothing. Natamux sends no usage data, crash reports or analytics anywhere. The only request it makes on its own is the update check of the desktop app, below.

## What stays on your machine

- Tasks, their history and stage files: in `.natamux/` of each repository, as Markdown and JSON files, plus a local SQLite index.
- Worktrees: in `.natamux/worktrees/`, or the folder you set.
- The daemon token, the update setting, downloaded updates and the logs of the desktop app: in its user data folder (`~/Library/Application Support/Natamux` on macOS, `~/.config/Natamux` on Linux).
- Dashboard preferences (theme, layout, the token): in your browser's `localStorage`.

You can delete any of these folders at any time.

## When data leaves your machine

Only when you or an agent you start makes it happen:

- **Coding agents.** Claude Code, Codex, Gemini CLI, Copilot CLI and Cursor send your prompts and code to their own companies, under those companies' terms. Natamux starts them; it does not see or change what they send.
- **GitHub.** Pushing a branch, opening a pull request, importing issues, or a GitHub issue as task context uses your own `git` and `gh` login to talk to GitHub.
- **Links.** Links in the dashboard open in your browser.
- **Update check (desktop app).** About 30 seconds after the app starts, and every 6 hours, the app asks `github.com` for the latest Natamux release. The request has no data about you, your repositories or your tasks. GitHub sees your IP address and the app's version. When a new version exists, the app downloads it and its signature from GitHub. Untick **Check for Updates Automatically** in the app menu (macOS) or the **Help** menu (Linux) to turn the timed checks off.

## The website

The Natamux website on GitHub Pages is static. It sets no cookies and has no analytics. GitHub may keep its own server logs, under the [GitHub Privacy Statement](https://docs.github.com/site-policy/privacy-policies/github-general-privacy-statement).

## Questions

Open an issue, or for anything sensitive use the private report described in [SECURITY.md](SECURITY.md).
