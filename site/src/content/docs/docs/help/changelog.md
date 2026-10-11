---
title: Changelog
description: What changed in each Natamux release.
sidebar:
  order: 4
---

All releases are on [GitHub releases](https://github.com/lenhatthanh20/natamux/releases).

## v0.1.8 (2026-10-11)

- Run a hard stage as a Claude Code workflow: more accurate, but many more tokens. In **Project settings**, an agent stage has **Tasks can run it as a workflow** (`"workflow": true`), on by default for **Plan** and **Implement**. On a task, the stage card in **Progress** has **Run with a workflow**, off by default. Only Claude Code runs workflows. The CLI has `natamux stage workflow <task> on|off --stage <id>`, and the API has `PUT /tasks/:id/workflow`. See [Run a stage as a workflow](/docs/concepts/stages/#run-a-stage-as-a-workflow).
- The **Start a task** dialog has two groups. **Context** holds **Title**, **Context from**, **Notes** or **Reference**, and **Attachments**, under a line that says what the agent reads for the source you pick. **Workspace and agent** holds **Project**, **Branch from** and **Agent**. **Notes** is taller and suggests what to write, and **Reference** has a hint under it. See [Add a task](/docs/getting-started/quick-start/#2-add-a-task).

## v0.1.7 (2026-10-11)

- **Project settings** has a **Stages** tab, so you change the pipeline without JSON. **Add stage** picks from a catalog of 27 common steps, such as Research, Security review or Manual QA, or makes a **Custom stage**. Turn a stage off with its switch (`"enabled": false` in the file), drag it or use `Alt+ArrowUp` and `Alt+ArrowDown`, and run it alongside the stage above. A flow chart shows the pipeline while you edit. See [Change the pipeline](/docs/concepts/stages/#change-the-pipeline) and [Project settings](/docs/guides/dashboard/#project-settings).
- Each stage has **Wait for my approval**, with its default shown, and a **Prompt** you can change. **Insert skill** puts `{skill:<name>}` in the prompt. Natamux fills it for the agent of the task: `/name` for Claude Code and Copilot CLI, `$name` for Codex, and the path of the `SKILL.md` for the others. See [Skills in a prompt](/docs/concepts/stages/#skills-in-a-prompt).
- The `skill` context source works with every agent. It used to send `/<skill> <ref>`, which only Claude Code understands.
- The **JSON** tab stays for the whole file. **Save** changes only `stages`, keeps the other fields, and leaves `stages` out when it equals the default. If the file changed on disk, **Reload** or **Overwrite**. The API has `GET /repos/:id/skills` and a version on the settings routes. See [HTTP API](/docs/reference/http-api/).
- The default `implement` stage in the configuration reference no longer shows `requireCommit` and `requireClean`. The built-in pipeline never had them. See [Stage catalog](/docs/reference/configuration/#stage-catalog).
- Fix: the bottom row of the terminal, such as the status line of Claude Code, was hidden under the edge of the pane, because the terminal took more rows and columns than the pane could show. The terminal now fits its pane.

## v0.1.6 (2026-10-11)

- **Delete task** in the menu of a task row, of a workspace row and of the details panel. It deletes the task, its notes and its history, and the worktree when there is one. It asks first, and deletes nothing when the worktree has changes that are not committed. **Remove workspace** still keeps the task. See [Sidebar](/docs/guides/dashboard/#sidebar).
- The dashboard opens faster: the Markdown view, the file editor and the terminal load their code only when you open them. A terminal hidden for 10 seconds gives back its GPU memory. Changed-file counts of many workspaces load faster. See [Font and drawing](/docs/guides/terminal/#font-and-drawing).
- Fix: when a stage ended and the next one started (for example **Testing** after you approved **Review**), Cursor showed the next prompt but did not send it, and you had to press Enter. Natamux now presses Enter a moment after the paste, so Cursor sends the prompt on its own. Review comments and failed-check messages are sent the same way.

## v0.1.5 (2026-10-11)

- Attach files to a new task: in the **Start a task** dialog, click **Add files**, drop files or paste a screenshot, or use `natamux task create --attach <path>`. Up to 20 files, 20 MB each and 50 MB together. They are copied to `.natamux-task/attachments/` and listed in `context.md`. See [Attachments](/docs/concepts/stages/#attachments).
- Fix: Codex tasks failed at start, because Codex CLI 0.159 no longer accepts `--full-auto`. Natamux now passes `--sandbox workspace-write --ask-for-approval never`. See [Codex](/docs/agents/codex/).
- Fix: the **Push** button counted the commits ahead of the base branch, so it still showed **Push ↑N** after a push. It now counts the commits that `origin/<branch>` does not have, shows **Publish branch** when origin has no such branch, and says to pull or rebase first when origin has commits the branch lacks.
- Fix: when a task started stages side by side (for example **Implement** and **Test spec**), the shell of the second stage showed only after you left the task and came back. It now shows as soon as the stage starts, and the shell you type in keeps the keyboard.
- The **Agents** grid keeps its panes in place. A task event no longer moves its pane to the front.
- The terminal scrollbar is thin, like the rest of the app, and shows search matches.
- Fix: opening the shell of a done or cancelled task started a shell that kept running after you left. It now ends a minute after no pane shows it, unless a program runs in it. See [Terminal](/docs/guides/terminal/).
- Fix: typing Vietnamese with an input method such as GoTiengViet, EVKey or OpenKey lost or mixed up letters in Claude Code. The input method erases a letter and types the accented one; Natamux now sends the two to the shell a moment apart, so Claude Code reads them apart.

## v0.1.4 (2026-10-11)

- The **Progress** view moved from a workspace tab into the details panel, next to the terminal. Key `4` now shows **Docs**. The details panel is open by default beside a workspace. See [Progress](/docs/concepts/stages/#progress).
- The **Docs** tab shows Markdown as formatted text. Switch to **Source** for the raw text. See [The Docs tab](/docs/concepts/stages/#the-docs-tab).
- Each terminal shows the state of its own stage, not the state of the whole task.
- **Remove finished workspaces** in the menu of a project, and on its "N finished" row. See [Sidebar](/docs/guides/dashboard/#sidebar).
- Any project can be removed. The app opens with no project: use **File → Add Project…** (no restart). The live socket tells open dashboards when the project list changes.
- A task whose agent waits for background work stays running and says what it waits for. It is not quiet. See [Waiting for background work](/docs/concepts/agents-and-hooks/#waiting-for-background-work).
- The **Needs you** section has an amber frame and a bell, with two-line rows. A task whose workspace was removed leaves it.
- Fix: after a resize or a reconnect, words of an agent broke apart at the right edge of the terminal. A terminal now shows the screen of the shell, drawn for its size. The pane you type in or click sets the size of the shell. See [Terminal](/docs/guides/terminal/).
- Terminals draw with the GPU, wait for the end of each screen update (no flicker with Claude Code), give emoji two columns and keep about 10 MB of history each, like Ghostty. **Terminal font…** in the Account menu; the desktop app uses your Ghostty font by default. See [Font and drawing](/docs/guides/terminal/#font-and-drawing).
- A task in review shows a banner with **Approve** and **Send back** in its workspace. With changed files and nothing committed, the header offers **Commit…**, and the commit and push toasts offer the next step.
- **Needs you** rows say what the agent asks or why it failed. Empty board columns fold. The table row menu has the task actions. **Start a task** has its own button next to **Add task**.
- `natamux task create --repo <id>` names the project. See [CLI](/docs/reference/cli/).
- Fix: on Windows, a task title with a typographic quote (’) could end the PowerShell launch line early.
- Fix: **No agent, a plain shell** started Claude Code anyway with the default stages.

## v0.1.3 (2026-10-11)

- **Remove project** in the menu of a repository's group in the sidebar. See [Repositories](/docs/guides/dashboard/#repositories).
- Agents start in auto mode, so they do not stop to ask before each command. Set `"agents": { "permissionMode": "ask" }` in `.natamux.json` to get the prompts back. See [Auto mode](/docs/agents/claude-code/#auto-mode).
- The Implement and Testing stages leave the changes uncommitted. You commit them in the **Changes** tab. See [Checks](/docs/concepts/stages/#checks).
- **New workspace** is now **Start a task**, a repository is a "project", and the dialog shows the **Notes** field at once.
- The task details panel starts closed. The terminal header shows the agent's icon and name.
- Fix: a terminal showed only its last 1,000 lines after you switched tasks or reloaded the dashboard. A shell now keeps all its output, and each pane keeps 10,000 lines of scrollback. See [Terminal](/docs/guides/terminal/).
- Removing a workspace works when its worktree folder is already gone.
- Fix: "command not found" when an agent starts. Task shells are login shells, and the app adds the Homebrew folders when it cannot read the login shell PATH.
- Fix: agent hooks failed behind a company proxy. Task terminals add `127.0.0.1`, `localhost` and `::1` to `NO_PROXY`.
- Only macOS on Apple silicon is built. The Intel Mac and Linux builds are paused; the last release with them is v0.1.2.

## v0.1.2 (2026-10-10)

- The desktop app updates itself. It checks GitHub for a new release, downloads it in the background, checks it against `SHA256SUMS.txt` and installs it on restart. See [Updates](/docs/guides/desktop-app/#updates).
- **Check for Updates…** and **Check for Updates Automatically** in the app menu (macOS) or the **Help** menu (Linux).

## v0.1.1 (2026-10-10)

- The macOS app is now `Natamux.app` (it was `natamux.app`).
- Download files no longer carry the version in their name, such as `Natamux-mac-arm64.dmg`. Links to the latest release always get the newest build.
- `natamux task show` without an id shows the task of the terminal you run it in.
- `natamux agent start` on a task with no workspace makes the workspace first (`--base` picks the branch). The dashboard and the CLI start the agent you picked, not the task's default one.
- The sign-in page says where the token comes from: `NATAMUX_TOKEN` of the daemon.

## v0.1.0 (2026-10-10)

The first release. Run many coding agents in parallel, each task in its own git worktree and terminal, with every task's state on one dashboard.

### What is in it

- **Tasks and a dashboard.** A table and a board of every task, with a "Needs you" strip. Keyboard first (`⌘K` or `Ctrl+K`).
- **A worktree per task.** Each task gets its own branch and folder.
- **Five agents.** Claude Code, Codex, Gemini CLI, GitHub Copilot CLI and Cursor. Natamux adds hooks for each session.
- **Stages.** Context, Spec, Plan, Test spec and Implement side by side, Review, Testing, Release. Change them per project in `.natamux.json`. Each stage checks its result before it moves on.
- **Changes and history.** A file tree, side by side or inline diffs, commit and push, open a pull request, and a branch graph.
- **The `natamux` command**, short name `nmx`, for you and for agents.
- **Browser or desktop app.** No account and no telemetry.

### Downloads

| System | File |
|---|---|
| macOS, Apple silicon | `Natamux-0.1.0-mac-arm64.dmg` |
| macOS, Intel | `Natamux-0.1.0-mac-x64.dmg` |
| Linux, any distribution | `Natamux-0.1.0-linux-x86_64.AppImage` |
| Debian, Ubuntu | `Natamux-0.1.0-linux-amd64.deb` |
| Linux, no install | `Natamux-0.1.0-linux-x64.tar.gz` |

`SHA256SUMS.txt` lists the checksum of every file.

### Before you open it

The release is not signed with an Apple certificate.

- **macOS:** drag Natamux to Applications and open it once. macOS blocks it. Open **System Settings → Privacy & Security** and click **Open Anyway**. If there is no such button, run:

  ```sh
  xattr -dr com.apple.quarantine /Applications/Natamux.app
  ```

- **Ubuntu 24.04 and later, AppImage:** the system can block Chromium's sandbox. Install the `.deb`, or start the AppImage with `--no-sandbox`.

You need at least one agent installed and logged in, and `git`. See [Install](/docs/getting-started/install/).

### Known limits

- No Windows build.
- No automatic updates. Watch the repository's releases for new versions.
