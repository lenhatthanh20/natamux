---
title: Changelog
description: What changed in each Natamux release.
sidebar:
  order: 4
---

All releases are on [GitHub releases](https://github.com/lenhatthanh20/natamux/releases).

## v0.1.38 (2026-10-19)

- A subtask is part of its parent's work: it starts from the parent's branch, its pull request goes into that branch, and the parent's next stage waits for its open subtasks, then pulls their work. See [Subtasks](/docs/concepts/tasks/#subtasks).
- Add a subtask with **Add subtask…** in the row menu, or with `natamux task create <title> --parent <id>`. See [task](/docs/reference/cli/#task).
- A new **Settings** dialog gathers the app-wide settings. Open it with **Settings…** in the Account menu or the desktop app menu, or `Mod+,`. See [Settings](/docs/guides/dashboard/#settings).
- The desktop app shows a spinner while it opens, instead of a blank window.

## v0.1.37 (2026-10-19)

- Click a notification to open its task. The desktop app icon shows how many tasks need you, and **File → Keep Computer Awake While Agents Run** keeps the computer awake while an agent runs. See [Notifications](/docs/guides/dashboard/#notifications) and [Menu](/docs/guides/desktop-app/#menu).
- In the command palette, `Tab` cycles the scope through **Needs you** and each project. See [Command palette](/docs/guides/dashboard/#command-palette).
- After a restart, opening a task brings back every agent of the task, also one that crashed or that you started by hand. An agent you quit stays quit. See [A shell after a daemon restart](/docs/guides/terminal/#a-shell-after-a-daemon-restart).
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

- A stage in Shell 1 no longer closes it with **End the agent and close its shell when the stage is done**. Shell 1 keeps the task's agent and its session; the stage settings say so. See [End a stage's shell](/docs/concepts/stages/#end-a-stages-shell).
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

- A task's shells come back after a restart, with the same names. An agent goes on with its last conversation. See [A shell after a daemon restart](/docs/guides/terminal/#a-shell-after-a-daemon-restart).
- Fix: a shell that a stage closed came back as a new plain shell. Its pane now leaves the **Terminal** tab.
- Fix: after an API error, a Claude Code stage stayed failed when you typed to the agent or clicked **Start agent**. Now the stage runs again. See [Events and states](/docs/agents/claude-code/#events-and-states).

## v0.1.27 (2026-10-15)

- Fix: when an agent ended its turn and still ran, **Resume** left it at its prompt. Resume now asks it to go on with the stage. See [Type to the agent](/docs/guides/terminal/#type-to-the-agent).

## v0.1.26 (2026-10-15)

- A context, agent or testing stage can end its agent and close its shell when it is done (`"endShell": true`). A next stage in that shell starts a new agent session. See [End a stage's shell](/docs/concepts/stages/#end-a-stages-shell).
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
- When Natamux starts again, the agents of running and waiting tasks start again on their own. See [A shell after a daemon restart](/docs/guides/terminal/#a-shell-after-a-daemon-restart).
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
- An agent you start yourself in a shell shows in the pane header with its own name and, when its hooks reach Natamux, its own state. Natamux runs at most 30 shells in all. See [Shells](/docs/guides/terminal/#shells).
- Fix: a shell where you started Cursor with `agent` showed nothing in its header or in the sidebar. Now it shows Cursor and its state.

## v0.1.11 (2026-10-13)

- Open subtasks sit under their parent task in the sidebar. A line such as "2 open subtasks · 2 parallel stages" shows or hides the task's agent shells and subtasks; click it or press `Right` and `Left`. Click a shell to open the task's terminal on it. The line opens by itself while something under it works, waits or failed, and Natamux remembers what you fold. The line under a workspace title shows the agent, the task id (hover it for the branch), the stage and its step, the pull request, how many subtasks are done, and the size of the diff. A blue dot marks a change you have not seen yet. See [Sidebar](/docs/guides/dashboard/#sidebar).
- A task that waits for you says so once in **Needs you** and once in the banner of its workspace. The workspace row, the workspace header and the terminal pane no longer repeat it, and terminal panes have no amber bar or tint. With more than one agent pane, the waiting one says **Needs your input** in its header. The state is called **Needs your input** everywhere. When the agent ends its turn without `natamux done`, the banner says **The agent ended its turn. Reply in the terminal, or resume it.** See [Needs you](/docs/guides/dashboard/#needs-you).
- Fix: when you opened a task with two shells side by side, the keyboard went to Shell 2 first and then jumped to Shell 1. Now Shell 1 gets the keyboard at once. A shell you pick with **Answer in terminal** or **Jump to a workspace** gets it with no stop in Shell 1.

## v0.1.10 (2026-10-12)

- When a task is done or cancelled, each pane keeps its last screen, dimmed, with no cursor, and takes no keys. You can still scroll, select, copy and find in it. A bar at the bottom says **Task done.** or **Task cancelled.**, and **Open a new shell** opens a plain shell in the workspace without the agent. After a daemon restart, the pane shows **The shell ended.** with the same link. See [Shells](/docs/guides/terminal/#shells).
- When you switch to another task's workspace, the keyboard goes to its first agent shell, so you can type to the agent at once. See [Open the terminal](/docs/guides/terminal/#open-the-terminal).
- The sidebar and the details panel start at the same width, 320 px. A width you set by dragging stays.
- Fix: after you quit and opened the app again, **Resume** on a task that waits for you moved it to running, but no agent ran in its terminal. Now **Resume** starts the agent again. Claude Code goes on with the same conversation; other agents start again with the stage prompt. See [Type to the agent](/docs/guides/terminal/#type-to-the-agent).
- Fix: in the desktop app, the first click on a terminal from another app only brought the window to the front, and a second click was needed to type. Now one click does both.

## v0.1.9 (2026-10-12)

- The terminal looks like a native terminal: **JetBrains Mono** at 13px with rows as tall as the font and bold at 700, bundled Nerd Font icons, the **Night** palette, dimmed panes without the focus, an outline cursor and a dimmer selection when a terminal has no focus, and `Alt` + wheel to scroll 5 times faster. See [Font and drawing](/docs/guides/terminal/#font-and-drawing).
- The diff in **Changes** and the file editor use JetBrains Mono, the font of the terminal, so code looks the same in a diff and in a shell.
- The text of the dashboard is in **Inter** at 13px, the UI font of JetBrains IDEs. Small labels such as ids and branches stay in Geist Mono.
- Faster terminals: up to 8 hidden terminals stay open, so going back to a terminal or a task shows it at once. A shell that prints a lot no longer slows down the page or the other shells, and `Ctrl+C` shows the prompt quickly. A dropped connection shows **Reconnecting.** and resumes the same shell, with the output you missed. After a daemon restart the pane shows **The shell ended.** with **Start a new shell**. See [Shells](/docs/guides/terminal/#shells).
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
