---
title: Changelog
description: What changed in each Natamux release.
sidebar:
  order: 4
---

All releases are on [GitHub releases](https://github.com/lenhatthanh20/natamux/releases).

## v1.0.12 (2026-10-31)

- Install or update the desktop app with one command: `curl -fsSL https://natamux.github.io/install.sh | sh` on macOS, Debian and Ubuntu, or `irm https://natamux.github.io/install.ps1 | iex` on Windows. The script checks the checksum, and the signature when minisign is installed. See [Install with one command](/docs/getting-started/install/#install-with-one-command).
- The homepage download button goes to the install command instead of downloading a file.
- The macOS app is signed with a Natamux certificate, so Full Disk Access stays after an update. The first update to v1.0.12 needs the grant once more. See [Full Disk Access on macOS](/docs/guides/desktop-app/#full-disk-access-on-macos).
- Programs in a Natamux terminal get a macOS prompt for privacy permissions instead of being denied.

## v1.0.11 (2026-10-30)

- In the desktop app, the sidebar hides the task id by default, so the title gets the whole row. **Settings > Appearance > Task ids in the sidebar** shows it again. See [Task ids in the sidebar](/docs/guides/desktop-app/#task-ids-in-the-sidebar).
- In the desktop app, the docked details panel has its own width again, from 280 to 560 px, 320 by default. Dragging its edge resizes only the panel. See [In the desktop app](/docs/guides/dashboard/#in-the-desktop-app).

## v1.0.10 (2026-10-30)

- Name the branch of a task: the **Branch** field in **Start a task**, the project pattern `git.branchName` with `{id}` and `{slug}`, `--branch` in the CLI and `branch` in the API. See [Branch name](/docs/concepts/workspaces/#branch-name).
- Natamux follows the branch the task really uses. When an agent switches the worktree to another branch, the pull request lookup, the CI check, push and the branch delete use it.
- In the desktop app, each row of the sidebar shows the task id before the title.
- In the desktop app, the sidebar and the docked details share one width, so dragging either edge resizes both. See [In the desktop app](/docs/guides/dashboard/#in-the-desktop-app).
- Fix: in the desktop app, the line between the terminals and the docked details panel was 2 px wide. Now it is 1 px.

## v1.0.9 (2026-10-30)

- In the desktop app, text boxes take the keys of a text field in a web page, such as moving by word, and `Page Up` and `Page Down` in boxes of many lines. Long lines wrap, also in **Notes** of **Start a task**. See [Text boxes](/docs/reference/shortcuts/#text-boxes).
- In the desktop app, the rows of **History** fit the side panel, and **Show in main** shows the graph over the terminals at full width with author, hash and age. See [History tab](/docs/guides/changes-and-history/#history-tab).
- Fix: in the desktop app, the side-by-side diff could not scroll to the end of a file when one side had many changed lines at the end. Now it does.
- Fix: in the desktop app details panel, the **Create issue** button stretched over the whole row. Now it is compact.

## v1.0.8 (2026-10-29)

- Fix: in the v1.0.7 desktop app, **Start** in **Start a task** closed the app at once when the project's commands needed no approval. Now the task starts.

## v1.0.7 (2026-10-29)

- A project's commands (a stage `check` or `command`, a path `when.command`, stage `args` and the setup script) run only after you approve them, and again after each change. The desktop app asks after you add a project and in **Start a task**, and the project settings show **Review commands** until you approve; without the app, use `natamux repo trust`. Projects added before were approved at the first start. See [Approve the project's commands](/docs/guides/project-settings/#approve-the-projects-commands).
- Each task's terminals get their own `NATAMUX_TOKEN`, which works only for that task. Agent commands such as `natamux done` work as before; a gate command such as `stage approve` typed in a task terminal is refused. The daemon also checks the `Host` and `Origin` of requests, and terminal links open only `http`, `https` and `mailto`. See [Security and privacy](/docs/help/security-and-privacy/).
- In the desktop app, each file you open gets its own tab above the editor. Tabs wrap, a dot marks an edit that is not saved, and `Mod+W` closes the shown file. See [Files of the worktree](/docs/guides/dashboard/#files-of-the-worktree).
- In the desktop app, the Projects dialog has **Browse…** buttons next to **Folder** and **Worktree folder**. See [Add a project](/docs/getting-started/quick-start/#add-a-project).
- On macOS, the Dock icon shows how many tasks need you once **Notify me** is on and notifications are allowed. See [Notifications and the Dock](/docs/guides/desktop-app/#notifications-and-the-dock).
- The desktop app starts, switches tasks and opens diffs faster. The Done and Cancelled columns show their 50 newest cards with **Show 50 more**, and a terminal that fell far behind comes back with its last 1,000 lines of history.
- Token usage counts the turn that runs `natamux done` or sends a stage back for that stage, and no longer loses the last model call of a turn or the usage of a terminal that ends.

## v1.0.6 (2026-10-28)

- The desktop app shows what the updater does in the status bar (**Downloading 1.0.6 · 42 MB**, **Restart to update**, **Update failed**) and in a new **Version** row under **Settings → General → Updates**. A timed check never opens a dialog: a ready update shows one message with **Restart Now** and **What's New**, and a failed timed check shows nothing. See [Updates](/docs/guides/desktop-app/#updates).
- In the desktop app, **Remove finished workspaces** and **Delete all done tasks** / **Delete all cancelled tasks** no longer skip a workspace with files that are not committed. The confirm names it and reads **Remove anyway** or **Delete anyway**: those files are lost. See [Sidebar](/docs/guides/dashboard/#sidebar).
- In the desktop app, with no task the main area shows **Welcome to Natamux** with three steps: choose a project, sign in to your agents, set up your workflow. See [Add a project](/docs/getting-started/quick-start/#add-a-project).
- In the desktop app, the **Agent** and **Kind** selects and the number fields on the **Stages** tab of a project's settings sit on the row of their label, so the page is shorter.

## v1.0.5 (2026-10-28)

- In the desktop app, a task whose worktree has files that are not committed can now be cleaned up. **Remove workspace** and **Delete task** list those files first and offer **Remove anyway** or **Delete anyway**, which discard them. The finished banner no longer disables **Remove workspace** for them. The web dashboard, the CLI and **Remove finished workspaces** still refuse. For scripts, add `force=true` to `DELETE /tasks/:id` or `DELETE /tasks/:id/workspace`. See [When a task is finished](/docs/guides/dashboard/#when-a-task-is-finished).
- Fix: in the v1.0.4 desktop app for Apple silicon on macOS 26, the green traffic light touched the first letter of the title bar. Now there is a gap.
- Fix: in the desktop app sidebar, the hover of the "N finished" row stopped before its **Remove finished workspaces** button. Now it covers the whole row.

## v1.0.4 (2026-10-27)

- In the **Changes** tab, comment on a line of a task's diff, or on a whole file, in the desktop app and the web dashboard. Comments are saved as drafts and stay on their code when the agent edits the file. **Send N comments** (`Mod+Alt+Enter`) sends every open comment to the agent in one prompt. Click **Resolve** when a comment is fixed. See [Comment for the agent](/docs/guides/changes-and-history/#comment-for-the-agent).
- An agent that ended its turn and waited 30 minutes, with no window that shows its terminal, pauses: Natamux ends its terminal and keeps its conversation. Show it again and the agent starts again on the same conversation. Change the minutes, or turn it off with `0`, with **Pause idle agents after (min)** on the **Stages** tab of a project's settings, or `agents.pauseIdleAfterMinutes` in `.natamux.json`. See [An idle agent pauses](/docs/guides/terminal/#an-idle-agent-pauses).
- In the desktop app, the status bar shows the agent nearest its plan limit, such as **Claude 84% · 5h**, and its menu lists each agent's windows. A toast says when a window passes 80%, and **Start a task** shows the share next to each agent. Only Claude Code and Codex tell their limits. See [Plan limits](/docs/guides/dashboard/#plan-limits).
- The desktop app has nine more theme families: Ayu, Catppuccin, Dracula, GitHub, Gruvbox, Nord, Rosé Pine, Solarized and Tokyo Night. Pick them under **Light theme** and **Dark theme** in **Settings → Appearance**. See [Settings](/docs/guides/dashboard/#settings).
- Deleting a task now also deletes its branch, also when the branch has commits that are not merged. A task whose worktree has changes that are not committed is still not deleted. For a task that used **Check on CI**, its `natamux-ci/` branch on GitHub goes too; before, only a done or cancelled task lost it. Only the branch of the pull request stays on GitHub, with the pull request.
- Fix: since v1.0.0 the desktop app showed no macOS notifications, played no sound and showed no count on the Dock icon. Now they work again, and a click on a notification opens the task. See [Notifications and the Dock](/docs/guides/desktop-app/#notifications-and-the-dock).
- Fix: in a narrow desktop app window, a click on a task in the sidebar drawer only closed the drawer. Now it opens the task.

## v1.0.3 (2026-10-26)

- An agent may now send a task back from one stage any number of times; before, it stopped after 2. To set a limit, type a number in **Agent send backs** on the **Stages** tab of a project's settings, or set `agents.maxSendBacks` in `.natamux.json`. Past the limit the task waits for you, as before. The limit counts CI failures too. See [Limit the send backs](/docs/guides/project-settings/#limit-the-send-backs).
- In the desktop app, a project with **Blur names** on shows its names, titles and labels as soft blurred bars, as the web dashboard blurs them, and not as dots. Tooltips still show dots.
- Fix: with a send-back limit, after a person sent a task back or approved it, the next agent send back from that stage could be refused at once. Now the limit counts only the agent send backs since the last action of a person on that stage.

## v1.0.2 (2026-10-26)

- In **Start a task** in the desktop app, **Branch from** has a folded **Remote branches** group with the branches of `origin` that have no local branch, also ones you never fetched. Pick `origin/<branch>` to start the task from it: Natamux fetches only that branch and makes no local branch for it. See [Base branch](/docs/concepts/workspaces/#base-branch).
- In the desktop app, the header of a project's Settings page reads "Projects / <name>". **Stages**, **Issues** and **JSON** are tabs under the header that stay on screen while the page scrolls, and the path of `.natamux.json` is in the bottom bar.
- Fix: in the **Progress** view, a long line under **Show evidence** ran past the right edge of the stage card. Now it wraps inside the card.
- Fix: when a stage that ends its terminal ran again in the same terminal within 20 minutes, the terminal could show "The terminal ended." while the new agent was working. Now it shows the new terminal.
- Fix: with git-lfs installed by Homebrew, making a task's worktree could fail with `git-lfs: command not found`. Natamux's own `git` and `gh` commands now use the `PATH` of your login shell.

## v1.0.1 (2026-10-26)

- In the desktop app, an open task has no tabs. Its terminals fill the main area. **Progress**, **Changes**, **History**, **Docs**, **Files** and **Details** are icons on a rail at the right edge of the window, and the rail stays when the details panel is closed. Click the picked view again to close the panel. See [In the desktop app](/docs/guides/dashboard/#in-the-desktop-app).
- In the desktop app, **Changes** lists the files and the commit box in the panel and shows the picked file's diff over the terminals. **Docs** lists the stage files in the panel and shows the picked one over the terminals. A file from **Files** opens over the terminals too. **Terminals**, or `Esc`, shows the terminals again, and `Mod+W` closes the open file.
- In the desktop app, to add a terminal, click the split button in a terminal's header. On a finished task, **Back to wrap-up** in a terminal's header goes back to the wrap-up.
- In **Start a task** in the desktop app, the selects (**Context from**, **Skill**, **Project**, **Branch from** and **Agent**) sit on the row of their label and are as wide as their text. A long name is cut short.
- In the desktop app, `Enter` starts a new line in **Notes**, in the notes of the edit form, in the send-back reason, in the commit message and in the **Ask agent** comment.
- Fix: in the sidebar, the buttons of a project's header followed its task count, and the rows under a task were not indented. Now the buttons sit on the right and the rows are indented again.
- Fix: after a task was finished, **Reopen agent** could show the ended Terminal 1. Now it shows the new terminal of the agent.

## v1.0.0 (2026-10-26)

- Natamux is now a native app: the desktop app, the daemon and the `natamux` and `nmx` commands are Rust programs. The app draws its own window; the daemon still serves the dashboard to your browser. See [Desktop app](/docs/guides/desktop-app/).
- The app uses the same data folder as the version before, so it opens with your tasks, worktrees, settings and daemon token.
- The daemon starts on port 4317, or on another free port when 4317 is taken. The `natamux` command in a task terminal finds it.
- The app checks the signature of a release with the same release key as before, and installs nothing that fails the check. See [Updates](/docs/guides/desktop-app/#updates).
- The Docs tab uses the app's text sizes for markdown and for its file list.
- The Docker image `natamux-daemon` runs the Rust daemon. See [Self-host with Docker](/docs/guides/self-host-docker/).
- On Debian and Ubuntu the package is still `natamux`, so `apt install` replaces the old version. The app is `/opt/Natamux/natamux-app`; the `natamux-desktop` command is gone. If the AppImage does not start, start it with `--appimage-extract-and-run`. See [The AppImage does not start](/docs/help/troubleshooting/#the-appimage-does-not-start).
- Each task shows how many tokens its agents used and an estimated cost in USD: a **Usage** section in the task's **Details** tab, a line such as `$0.42 est. · 610K tokens` under each stage card, and a **Cost** column in the task table. The cost is an estimate at API list prices. Cursor does not report usage. See [Usage](/docs/guides/dashboard/#usage).
- Output in one terminal draws only that terminal. The sidebar, the task list, the details panel and the other panes draw again only when their own data changes.
- A window without focus stops the age clock of the sidebar and the 60-second checks of the open task. When the window has focus again, it reads once what it skipped. Changes the daemon sends are still read in the background.
- The open task reads its git state, its events, its stage files and its settings at most once a second. Many changes in a row make one read, and the screen draws again only when the result differs.
- The sidebar lays out only the rows on screen. The time labels of the sidebar, the task list and the details panel draw again only when one of them changes.
- In the daemon, a shell that prints a lot no longer holds up the other shells.
- Fix: a click on the sidebar or details toggle in the status bar could do nothing. Now every click shows or hides the panel.
- Fix: while Claude Code worked in Terminal 1, the sidebar row and the pane header showed a plain `zsh` terminal until the agent's turn ended. Now they show the agent while it prints.
- Fix: when the agent of a stage with **End the agent and close its terminal** ended the stage itself (`natamux done` or `natamux stage back`), its terminal closed before the agent saw the answer. Now the terminal closes when the agent ends its turn, at most 60 seconds later. See [End a stage's terminal](/docs/concepts/stages/#end-a-stages-terminal).
- Fix: in the dashboard, a terminal you ended with `exit` could leave the screen at once. Now it keeps its last screen and **Open a new terminal**.

## v0.1.43 (2026-10-21)

- The stage settings in **Project settings → Stages** have a **Run in a terminal of its own** checkbox (`ownShell`). Checked, the stage runs in Terminal 5 with a new session. Unchecked, it runs in Terminal 1 with the task's agent. Unchecking also clears the stage's **Agent** and **Arguments** when it can no longer have them. See [The Stages tab](/docs/guides/project-settings/#the-stages-tab) and [Stages in their own terminal](/docs/concepts/stages/#stages-in-their-own-terminal).
- Fix: a stage that started in Terminal 5 kept it after `ownShell` was turned off. Now an `ownShell` change moves the stage to the terminal the settings give at its next start. A lane of a group keeps its terminal. See [Choose a model](/docs/concepts/stages/#choose-a-model).

## v0.1.42 (2026-10-21)

- A new task answers at once. Natamux makes the worktree and starts the first agent in the background, and the task shows **Preparing workspace** with the current step. A toast says when the workspace is ready or failed. A failure keeps the task, so **Create workspace** can try again. See [Start a task](/docs/getting-started/quick-start/#start-a-task).
- The status bar shows background jobs, with a menu of each job, its task and how long it has run. `GET /jobs` and `jobs.changed` give the same list. See [Background jobs](/docs/guides/dashboard/#background-jobs).
- When Spec or Plan runs in its own terminal, an empty Terminal 1 names the terminal that works now, such as **Terminal 5 is working**. See [Terminals and agents](/docs/concepts/stages/#terminals-and-agents).
- Fix: a settings change could move a running stage to another terminal, so `natamux done`, turn ends and send backs reached the wrong stage. Now a stage keeps the terminal it started in until it starts again. See [Choose a model](/docs/concepts/stages/#choose-a-model).

## v0.1.41 (2026-10-20)

- In **Project settings → Stages**, a stage that starts its own agent shows that agent's icon and its model (or the agent's name) under its name in the flow chart. See [The Stages tab](/docs/guides/project-settings/#the-stages-tab).

## v0.1.40 (2026-10-20)

- **Reopen agent** in the wrap-up of a done or cancelled task starts the agent of Terminal 1 again on its last conversation. The task keeps its state. See [When a task is finished](/docs/guides/dashboard/#when-a-task-is-finished).
- The Done and Cancelled columns of the board have a trash button that deletes every task in the column. Natamux asks first and skips a task with changes that are not committed. See [Task views](/docs/guides/dashboard/#task-views).
- The Overview card of the Progress view is one row of short figures, with the full meaning in a tooltip.
- On macOS 26, the desktop app's menu items have icons, so their titles line up with the system items. See [Menu](/docs/guides/desktop-app/#menu).
- Fix: Copilot CLI printed a warning at start and skipped Claude Code's hook file in the worktree. Now Copilot shows no warning and reports each event once. See [Copilot](/docs/agents/copilot/#how-hooks-are-added).

## v0.1.39 (2026-10-20)

- **Rollback…** in the Changes tab puts files that are not committed back as they are in the last commit. It is also on a file or folder row, and `Mod+Alt+Z` rolls back the file in the diff. The toast has **Undo**. See [Roll back changes](/docs/guides/changes-and-history/#roll-back-changes).
- The panes of a task are called terminals: **Terminal 1**, **New terminal**, and the sidebar tag T1. See [Terminals](/docs/guides/terminal/#terminals).
- Fix: when the file list of the Changes tab was narrow, **Commit all** covered the diff. Now it moves to its own line.

## v0.1.38 (2026-10-19)

- A subtask is part of its parent's work: it starts from the parent's branch, its pull request goes into that branch, and the parent's next stage waits for its open subtasks, then pulls their work. See [Subtasks](/docs/concepts/tasks/#subtasks).
- Add a subtask with **Add subtask…** in the row menu, or with `natamux task create <title> --parent <id>`. See [task](/docs/reference/cli/#task).
- A new **Settings** dialog gathers the app-wide settings. Open it with **Settings…** in the Account menu or the desktop app menu, or `Mod+,`. See [Settings](/docs/guides/dashboard/#settings).
- The desktop app shows a spinner while it opens, instead of a blank window.

## v0.1.37 (2026-10-19)

- Click a notification to open its task. The desktop app icon shows how many tasks need you, and **File → Keep Computer Awake While Agents Run** keeps the computer awake while an agent runs. See [Notifications](/docs/guides/dashboard/#notifications) and [Menu](/docs/guides/desktop-app/#menu).
- In the command palette, `Tab` cycles the scope through **Needs you** and each project. See [Command palette](/docs/guides/dashboard/#command-palette).
- After a restart, opening a task brings back every agent of the task, also one that crashed or that you started by hand. An agent you quit stays quit. See [A shell after a daemon restart](/docs/guides/terminal/#a-terminal-after-a-daemon-restart).
- A terminal hidden for 20 minutes is closed in the dashboard to save memory. Its shell keeps running.
- Fix: an agent's `git add` or `git commit` could fail on `index.lock` while the dashboard was open. The dashboard now reads git status without the lock.
- Fix: typed Vietnamese was garbled when the daemon had no locale. Task shells now get `LANG=C.UTF-8`. See [Task terminals](/docs/reference/environment-variables/#task-terminals).
- Fix: menus in dialogs, double clicks in the task table, **Start agent** on finished tasks, subtask projects, fan out try numbers, **Save as template** with a long title, the **Find** count and the dashboard fonts.

## v0.1.36 (2026-10-18)

- The desktop app no longer shows **Sign out**, because it signs in by itself. In a browser, Sign out says it removes the daemon token from this browser. See [Account menu](/docs/guides/dashboard/#account-menu).

## v0.1.35 (2026-10-18)

- Select lines in a diff and click **Ask agent** or press `Mod+Shift+A`: the agent gets the file, the line range and the code with your comment. See [Ask about selected lines](/docs/guides/changes-and-history/#ask-about-selected-lines).
- The new **Files** view of the details panel lists the files of the task's worktree. Drag one onto a terminal to paste its path, or open it in an editor tab to change and save it. See [Files of the worktree](/docs/guides/dashboard/#files-of-the-worktree).

## v0.1.34 (2026-10-17)

- The notice of a task that needs input, failed or waits for review, with its actions, is now in the current stage card of **Progress**, not in the workspace header. Click the state in the header to open the details at that stage. See [Control a stage by hand](/docs/concepts/stages/#control-a-stage-by-hand).
- **Account → Report an issue** opens a new GitHub issue in your browser. See [Account menu](/docs/guides/dashboard/#account-menu).

## v0.1.33 (2026-10-17)

- `natamux done` from a stage that does not run now is refused, so a Review agent that sent a task back no longer passes Implement. A send back also closes the shell of a stage with **End the agent and close its shell when the stage is done**. See [What `natamux done` does](/docs/concepts/stages/#what-natamux-done-does).
- Wait states survive a daemon restart, two done reports run the checks once, and removing a workspace keeps the latest stage files.
- Copilot asks for input only when it shows a prompt. A Cursor or Copilot hook file with a hook of your own is kept. See [Agents and hooks](/docs/concepts/agents-and-hooks/).
- The task notice is in the workspace header, so the shells no longer blink when the task changes state. A hidden cursor stays hidden when a terminal opens again, and quitting waits for every shell to end.
- A hand edit of a task file is no longer lost to a change right after it, and PR and issue links take only `http` and `https` URLs.
- Time limits on `gh` and update requests, one pull request lookup per project, and a stricter token check.
- A new homepage and a regrouped guide.

## v0.1.32 (2026-10-16)

- **Progress** folds each path a task did not take into one row marked **Not taken**. Click it to see its stages. The branch says which path it took and why. See [Paths with a condition](/docs/concepts/stages/#paths-with-a-condition).
- The **Progress** overview shows the times sent back only when there are any, and the current stage scrolls to the middle.
- New guide pages: [Build a workflow](/docs/guides/workflows/) and [Mix agents in one task](/docs/guides/mix-agents/).

## v0.1.31 (2026-10-16)

- A stage in Shell 1 no longer closes it with **End the agent and close its shell when the stage is done**. Shell 1 keeps the task's agent and its session; the stage settings say so. See [End a stage's shell](/docs/concepts/stages/#end-a-stages-terminal).
- The button that opens a new pull request says **Create PR** at every width. See [Push and open a pull request](/docs/guides/changes-and-history/#push-and-open-a-pull-request).
- A task title may have up to 400 characters. See [Task fields](/docs/concepts/tasks/#task-fields).
- Fix: a refused field gave only "The daemon answered with status 400." The message now names the field and the reason.
- Fix: a shell that a stage closed while another task was shown came back as an empty plain shell. It now stays closed.
- Fix: a new shell, a closed shell, or a banner or panel that changed a terminal's size showed the shell redrawing. The pane is now hidden while its size changes, and fades in after the shell has redrawn.

## v0.1.30 (2026-10-16)

- The line at the end of every stage prompt now reads "When you are done, run `natamux done`, unless the steps above say not to." A prompt of your own can say when the agent must not run it, for example after it sends the task back. See [Configuration](/docs/reference/configuration/).

## v0.1.29 (2026-10-16)

- A pipeline can split into 2 to 4 paths by a condition: a file has some text, or a command succeeds. The task runs only the stages of the path it takes. See [Paths with a condition](/docs/concepts/stages/#paths-with-a-condition).
- **Blur names** on a sidebar project blurs its names, and the names of its tasks, while you share your screen. See [Sidebar](/docs/guides/dashboard/#sidebar).
- Timed update checks are off by default. Tick **Check for Updates Automatically** to check every 6 hours. See [Updates](/docs/guides/desktop-app/#updates).

## v0.1.28 (2026-10-15)

- A task's shells come back after a restart, with the same names. An agent goes on with its last conversation. See [A shell after a daemon restart](/docs/guides/terminal/#a-terminal-after-a-daemon-restart).
- Fix: a shell that a stage closed came back as a new plain shell. Its pane now leaves the **Terminal** tab.
- Fix: after an API error, a Claude Code stage stayed failed when you typed to the agent or clicked **Start agent**. Now the stage runs again. See [Events and states](/docs/agents/claude-code/#events-and-states).

## v0.1.27 (2026-10-15)

- Fix: when an agent ended its turn and still ran, **Resume** left it at its prompt. Resume now asks it to go on with the stage. See [Type to the agent](/docs/guides/terminal/#type-to-the-agent).

## v0.1.26 (2026-10-15)

- A context, agent or testing stage can end its agent and close its shell when it is done (`"endShell": true`). A next stage in that shell starts a new agent session. See [End a stage's shell](/docs/concepts/stages/#end-a-stages-terminal).
- Fix: a testing stage whose command was running when the daemon stopped stayed running. The daemon now runs the command again when it starts.
- Codex and Gemini run inline, so their output goes into the terminal history. See [Codex](/docs/agents/codex/#what-natamux-runs) and [Gemini CLI](/docs/agents/gemini/#what-natamux-runs).
- The waiting state is now called **Needs input**, so its chip fits the Table view. See [Status icons](/docs/guides/status-icons/).

## v0.1.25 (2026-10-15)

- Task shells get an agent token. An agent cannot approve, skip or pass a stage, or change project settings, and what it does is recorded as the agent's. See [Agents run with your rights](/docs/help/security-and-privacy/#agents-run-with-your-rights).
- Fix: on Windows, control bytes in a prompt, such as from an imported issue, could run a command in PowerShell. Natamux removes them from every prompt.
- The **Stages** tab shows every check command, and an import lists the commands in the file before it applies them. See [Import and export](/docs/guides/project-settings/#import-and-export).
- A finished task says what is left, shows a wrap-up on the **Terminal** tab, and removes its workspace in one dialog. See [When a task is finished](/docs/guides/dashboard/#when-a-task-is-finished).
- Switching tasks no longer makes the terminals jump, and **Check for Updates…** shows a popup while it works. See [Updates](/docs/guides/desktop-app/#updates).

## v0.1.24 (2026-10-14)

- The desktop app installs an update only when `SHA256SUMS.txt` of the release is signed with the Natamux release key. A missing or wrong signature stops the update. v0.1.23 and earlier do not check it. See [Updates](/docs/help/security-and-privacy/#updates).
- Every task state has a status icon of its own, everywhere. A running task blinks once every 2 seconds instead of spinning. See [Status icons](/docs/guides/status-icons/).
- Fix: Copilot CLI 1.0.97 refused to start, because Natamux added `--no-alt-screen`, which it no longer has. See [What Natamux runs](/docs/agents/copilot/#what-natamux-runs).

## v0.1.23 (2026-10-14)

- When you add a project, **Worktree folder** sets where the worktrees of its tasks go. See [Repositories](/docs/guides/dashboard/#repositories).
- A stage with its own agent or arguments now runs in a shell of its own, from Shell 6. Natamux never closes an agent while the task runs, so Shell 1 keeps the task's agent and its session. See [Choose a model](/docs/concepts/stages/#choose-a-model).
- Natamux starts Copilot CLI with `--no-alt-screen`, so its output goes into the terminal history and the light line at the right edge is gone. See [What Natamux runs](/docs/agents/copilot/#what-natamux-runs).

## v0.1.22 (2026-10-14)

- In the desktop app, **Browse…** in the Projects panel picks the project folder in a folder dialog. See [Repositories](/docs/guides/dashboard/#repositories).
- An agent or testing stage of your own can pick its **Agent** and **Arguments**, in Shell 1 too. There it closes another agent while it works, and the next stage in Shell 1 gets the task's agent back with a new session. See [Choose a model](/docs/concepts/stages/#choose-a-model).

## v0.1.21 (2026-10-14)

- Each project in the sidebar is a card in its own color, from the project id. A gear next to **+** opens its **Settings**. See [Sidebar](/docs/guides/dashboard/#sidebar).
- **Project settings** can export the settings as `<project>.natamux.json` and import a `.json` file. Import does not save until you click **Save**. See [Import and export](/docs/guides/project-settings/#import-and-export).

## v0.1.20 (2026-10-14)

- A pull request that the agent or you open another way, such as with `gh pr create`, is now linked to the task. A closed one is not. See [Pull requests](/docs/guides/github/#pull-requests).
- A new task starts from the default branch (`origin/HEAD`, then `main`, then `master`), not from the branch of your main checkout. Any base branch is fetched first, and the branch pickers start on the default branch. See [Base branch](/docs/concepts/workspaces/#base-branch).
- Fix: a stage could say that a skill was not found while **Insert skill** offered it, when only your main checkout had the skill, such as a plugin turned on in `.claude/settings.local.json`. Now Natamux also looks in your main checkout. See [Skills](/docs/agents/claude-code/#skills).
- Natamux has a new icon: an N made of three lanes, with a cursor.

## v0.1.19 (2026-10-14)

- Mark a stage **Skip** in **Progress** before the task gets there, and the task skips it when it reaches it. See [Skip a stage](/docs/concepts/stages/#skip-a-stage).
- Fix: when **Check on CI** found a failed run, the fix prompt could stay in the agent's input without being sent. Now it is sent at once, and the card says **Sent back by CI N×**. See [Check a stage on CI](/docs/concepts/stages/#check-a-stage-on-ci).
- Natamux no longer opens a GitHub issue for each task by itself; click **Create issue** in **Progress**. Choose another way in the new **Issues** tab of **Project settings**. Projects that set `tracker.createIssue` keep their choice. See [The Issues tab](/docs/guides/project-settings/#the-issues-tab).
- The desktop window opens at once, agent hooks and the task list answer faster, and the dashboard gets changes as they happen. Pull requests are checked only while a dashboard is open.
- Fix: every terminal pane showed a thin white line on the left of the scrollbar. It is gone.
- New guide pages: [The review loop](/docs/concepts/review-loop/), [Project settings](/docs/guides/project-settings/) and [Use the command line](/docs/guides/command-line/).

## v0.1.18 (2026-10-14)

- Natamux is available again for macOS on Intel and for Linux (AppImage, `.deb` and `.tar.gz`), and now for Windows 10 and 11 on x64. See [Install Natamux](/docs/getting-started/install/).
- Intel Mac and Linux installs of v0.1.2 update themselves again. On Windows the app tells you about a new version and opens its release page; run the new installer. See [Updates](/docs/guides/desktop-app/#updates).
- On Windows, task terminals run PowerShell, and closing the window quits the app, as on Linux.
- Start a task's title with an id, such as `ABC-12: Fix the login bug`, to make task `ABC-12`. See [Tasks](/docs/concepts/tasks/).
- The card with the figures at the top of the **Progress** tab no longer has a bar of stages, which redrew every second while a stage ran.

## v0.1.17 (2026-10-13)

- The app no longer waits for your login shell before it starts. The dashboard shows your tasks at once; changes, branches, terminals and agents start when the shell's `PATH` is read.
- A terminal hidden for 10 s gives back its GPU memory. The daemon starts far fewer programs: it shares one process list and keeps git answers for 5 s. **Refresh** still reads at once, and `Shift+R` now works in History too.
- The dashboard polls less and redraws less, also with many tasks. The Agents view loads its terminal code ahead.
- **Start a task** can take its context from any agent skill (**Context from** → **An agent skill**).
- After a restart, the agent of a stage in review starts again too.
- Terminal panes without the focus are no longer dimmed. The focused pane keeps its outline.

## v0.1.16 (2026-10-13)

- Spinners, stage rings, stripes and breathing icons move 8 times a second, all at once, not on every frame. With the window in front and tasks running, the app uses much less CPU.

## v0.1.15 (2026-10-13)

- A stage that was sent back shows only **Sent back N×** in its card. The long reason no longer fills the details panel; the agent still gets it in its prompt.

## v0.1.14 (2026-10-13)

- The app no longer keeps the CPU busy while agents work. In a window without focus, every spinner, ring and stripe stands still, and the text in the title bar turns grey. With the focus, the moving stripes and dashes cost much less to draw.
- The **Needs you** strip at the top of the sidebar is gone; a task that waits on you is already a card with its question. The **N need you** pill still lists every task that waits. A folded row shows its subtasks that need you, failed or are blocked. See [Sidebar](/docs/guides/dashboard/#sidebar).
- The second line of a task row shows its agent's icon. Each project has a header band with its name and the number of workspaces.
- **Generate** in the commit box (`Mod+Shift+G`) asks the task's agent for a commit message. **Commit…** drafts one too when the box is empty. See [Commit](/docs/guides/changes-and-history/#commit).
- Natamux marks each task's worktree as trusted for the agent it starts, so the agent does not ask. Cursor CLI and Copilot CLI get a file written outside the worktree. See [Folder trust](/docs/concepts/agents-and-hooks/#folder-trust).
- When Natamux starts again, the agents of running and waiting tasks start again on their own. See [A shell after a daemon restart](/docs/guides/terminal/#a-terminal-after-a-daemon-restart).
- A failed CI check shows one short line per failed run in the stage card. When CI fails on a testing stage, the task goes back to the code stage before it, such as Implement, at most twice. See [Check a stage on CI](/docs/concepts/stages/#check-a-stage-on-ci).

## v0.1.13 (2026-10-13)

- The sidebar gives each task the room it needs. A task that waits on you, failed or is blocked is a tinted card with the agent's question or the reason; **Answer** opens the terminal of the agent that asked. A task that works has two lines: id, step and stage, subtasks, what the agent does, the pull request and the agent. A finished task is one line. Each state has its own icon shape. See [Sidebar](/docs/guides/dashboard/#sidebar).
- Only the open task shows its shells and subtasks. Click a task, or press `Right` on it, to open it. The chevron and the comet are gone, and rows no longer open by themselves.

## v0.1.12 (2026-10-13)

- The default pipeline is Context, Spec, Plan, Test spec and Implement side by side, **Automated tests**, **Review** and Release. **Review** is an agent stage in its own shell: it writes `review.md` and sends the task back to Implement with the problems, at most twice, then waits for you. The review by a person is **Manual review** in the catalog, and **Manual QA** is now **Manual testing**. See [Review by an agent](/docs/concepts/stages/#review-by-an-agent).
- **Check on CI**: a checkbox on Implement and Automated tests pushes the work to `natamux-ci/<branch>`, waits for your CI and gives a failure back to the agent. See [Check a stage on CI](/docs/concepts/stages/#check-a-stage-on-ci).
- A stage beside Shell 1 or in its own shell can use another agent and arguments, such as a cheaper model. The Spec stage writes a **Code map**. See [Choose a model](/docs/concepts/stages/#choose-a-model).
- A new workspace starts from the branch of your main checkout as its remote has it now. See [Base branch](/docs/concepts/workspaces/#base-branch).
- The sidebar shows one row per shell under each task, a comet for the progress of the task, and a task icon that spins while any of its agents works. The diff size is no longer on the second line. See [Sidebar](/docs/guides/dashboard/#sidebar).
- An agent you start yourself in a shell shows in the pane header with its own name and, when its hooks reach Natamux, its own state. Natamux runs at most 30 shells in all. See [Shells](/docs/guides/terminal/#terminals).
- Fix: a shell where you started Cursor with `agent` showed nothing in its header or in the sidebar. Now it shows Cursor and its state.

## v0.1.11 (2026-10-13)

- Open subtasks sit under their parent task in the sidebar. A line such as "2 open subtasks · 2 parallel stages" shows or hides the task's agent shells and subtasks; click it or press `Right` and `Left`. Click a shell to open the task's terminal on it. The line opens by itself while something under it works, waits or failed, and Natamux remembers what you fold. The line under a workspace title shows the agent, the task id (hover it for the branch), the stage and its step, the pull request, how many subtasks are done, and the size of the diff. A blue dot marks a change you have not seen yet. See [Sidebar](/docs/guides/dashboard/#sidebar).
- A task that waits for you says so once in **Needs you** and once in the banner of its workspace. The workspace row, the workspace header and the terminal pane no longer repeat it, and terminal panes have no amber bar or tint. With more than one agent pane, the waiting one says **Needs your input** in its header. The state is called **Needs your input** everywhere. When the agent ends its turn without `natamux done`, the banner says **The agent ended its turn. Reply in the terminal, or resume it.** See [Needs you](/docs/guides/dashboard/#needs-you).
- Fix: when you opened a task with two shells side by side, the keyboard went to Shell 2 first and then jumped to Shell 1. Now Shell 1 gets the keyboard at once. A shell you pick with **Answer in terminal** or **Jump to a workspace** gets it with no stop in Shell 1.

## v0.1.10 (2026-10-12)

- When a task is done or cancelled, each pane keeps its last screen, dimmed, with no cursor, and takes no keys. You can still scroll, select, copy and find in it. A bar at the bottom says **Task done.** or **Task cancelled.**, and **Open a new shell** opens a plain shell in the workspace without the agent. After a daemon restart, the pane shows **The shell ended.** with the same link. See [Shells](/docs/guides/terminal/#terminals).
- When you switch to another task's workspace, the keyboard goes to its first agent shell, so you can type to the agent at once. See [Open the terminal](/docs/guides/terminal/#open-the-terminal).
- The sidebar and the details panel start at the same width, 320 px. A width you set by dragging stays.
- Fix: after you quit and opened the app again, **Resume** on a task that waits for you moved it to running, but no agent ran in its terminal. Now **Resume** starts the agent again. Claude Code goes on with the same conversation; other agents start again with the stage prompt. See [Type to the agent](/docs/guides/terminal/#type-to-the-agent).
- Fix: in the desktop app, the first click on a terminal from another app only brought the window to the front, and a second click was needed to type. Now one click does both.

## v0.1.9 (2026-10-12)

- The terminal looks like a native terminal: **JetBrains Mono** at 13px with rows as tall as the font and bold at 700, bundled Nerd Font icons, the **Night** palette, dimmed panes without the focus, an outline cursor and a dimmer selection when a terminal has no focus, and `Alt` + wheel to scroll 5 times faster. See [Font and drawing](/docs/guides/terminal/#font-and-drawing).
- The diff in **Changes** and the file editor use JetBrains Mono, the font of the terminal, so code looks the same in a diff and in a shell.
- The text of the dashboard is in **Inter** at 13px, the UI font of JetBrains IDEs. Small labels such as ids and branches stay in Geist Mono.
- Faster terminals: up to 8 hidden terminals stay open, so going back to a terminal or a task shows it at once. A shell that prints a lot no longer slows down the page or the other shells, and `Ctrl+C` shows the prompt quickly. A dropped connection shows **Reconnecting.** and resumes the same shell, with the output you missed. After a daemon restart the pane shows **The shell ended.** with **Start a new shell**. See [Shells](/docs/guides/terminal/#terminals).
- Fix: an agent typed into a new shell, such as `codex`, showed the agent of the task ("Claude Code") in the pane header. Shell panes and the **Agents** grid now show the agent that runs in each shell, and no agent for another program. Codex has its own icon.

## v0.1.8 (2026-10-11)

- Run a hard stage as a Claude Code workflow: more accurate, but many more tokens. In **Project settings**, an agent stage has **Tasks can run it as a workflow** (`"workflow": true`), on by default for **Plan** and **Implement**. On a task, the stage card in **Progress** has **Run with a workflow**, off by default. Only Claude Code runs workflows. The CLI has `natamux stage workflow <task> on|off --stage <id>`, and the API has `PUT /tasks/:id/workflow`. See [Run a stage as a workflow](/docs/concepts/stages/#run-a-stage-as-a-workflow).
- The **Start a task** dialog has two groups. **Context** holds **Title**, **Context from**, **Notes** or **Reference**, and **Attachments**, under a line that says what the agent reads for the source you pick. **Workspace and agent** holds **Project**, **Branch from** and **Agent**. **Notes** is taller and suggests what to write, and **Reference** has a hint under it. See [Start a task](/docs/getting-started/quick-start/#start-a-task).

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
