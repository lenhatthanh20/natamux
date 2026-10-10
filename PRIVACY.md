# Privacy

Natamux runs on your machine. It has no account, no server of its own, and no telemetry.

## What Natamux collects

Nothing. Natamux sends no usage data, crash reports or analytics anywhere, and has no code that phones home.

## What stays on your machine

- Tasks, their history and stage files: in `.natamux/` of each repository, as Markdown and JSON files, plus a local SQLite index.
- Worktrees: in `.natamux/worktrees/`, or the folder you set.
- The daemon token, the chosen repository and the logs of the desktop app: in its user data folder (`~/Library/Application Support/Natamux` on macOS, `~/.config/Natamux` on Linux).
- Dashboard preferences (theme, layout, the token): in your browser's `localStorage`.

You can delete any of these folders at any time.

## When data leaves your machine

Only when you or an agent you start makes it happen:

- **Coding agents.** Claude Code, Codex, Gemini CLI, Copilot CLI and Cursor send your prompts and code to their own companies, under those companies' terms. Natamux starts them; it does not see or change what they send.
- **GitHub.** Pushing a branch, opening a pull request, importing issues, or a GitHub issue as task context uses your own `git` and `gh` login to talk to GitHub.
- **Links.** Links in the dashboard open in your browser.

## The website

The Natamux website on GitHub Pages is static. It sets no cookies and has no analytics. GitHub may keep its own server logs, under the [GitHub Privacy Statement](https://docs.github.com/site-policy/privacy-policies/github-general-privacy-statement).

## Questions

Open an issue, or for anything sensitive use the private report described in [SECURITY.md](SECURITY.md).
