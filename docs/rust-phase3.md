# Rust phase 3: the desktop app

Phase 3 of `plan-rust-migration.md`: the native app (`crates/natamux-app`) gets the screens of the React dashboard one by one, on today's Node daemon. This page covers the shared base and screen 1 (T-0146): the app shell, the sidebar, the task table and the task board; screen 2 (T-0148): the workspace with its terminals, and the Agents grid; screen 3 (T-0149): the details panel with the stage timeline and the gate actions; screen 4 (T-0150): the Changes and History tabs of the workspace; screen 5 (T-0151): the Progress layout of All tasks and the pipeline editor of Project settings; screen 6 (T-0152): the Files tab of the details panel, the editor tab and the Docs tab of the workspace; and screen 7 (T-0154), the last of phase 3: the command palette and the jump palette, the shortcut sheet, the account menu with the terminal font dialog, the Settings dialog, the Projects dialog, the Issues and JSON tabs of Project settings, Import issues, and Sign in. It says how to get the beta, what CI checks, where the app differs from the React screens, and the gate results.

## Decisions (owner, 2026-10-22)

- This task builds the foundation and screen 1 only.
- The beta is a GitHub pre-release made from `dev`, with the attested CI build. It is unsigned until the Apple account exists. `scripts/release.sh` and the releases of `main` do not change.
- Inter, JetBrains Mono and the Lucide SVGs that screen 1 uses go in `crates/natamux-app/assets`, with their license files. No new crate.
- Earlier rules hold: no new crates without asking, `cargo` runs in CI only, Rust PRs target `dev`.
- Baselines are shot in CI and kept in the repo after the owner signs a screen off. Nothing is shot on the owner's Mac.

## Crates

| Crate | Layer | Job in phase 3 |
|---|---|---|
| `natamux-proto` | L0 | Screen 6 adds the file types of the worktree (`workspaces/files.rs`: tree entries, a file with its version), the stage file text and the attachments. Screen 5 adds the pipeline rules of core the editor runs (`stages/pipeline_rules.rs`: `pipelineSchema` and `pipelineIssues`), the catalog texts and prompts, `prompt.rs`, `terminal_lane.rs` and the settings rules outside `stages`. The types screen 1 reads and sends: repos with `worktreesDir`, templates, jobs, activity, diff stats, usage, the terminals list, task events, branches, changes, the graph, the new workspace input. `tests/core_values.rs` checks names and limits against `packages/core` |
| `natamux-client` | L3 | Screen 6 adds the tree, the file read and the file save with `If-Match`, the stage file read and save, and the attachments (`files_methods.rs`). Screen 5 adds `save_settings` (`PUT /repos/:id/settings` with `If-Match`). One function per route, in files that mirror the daemon's route files. The live feed reads the whole dashboard (tasks, repos, jobs, activity, usage, diff stats, terminals, the preparing step) |
| `natamux-app` | L4 | Screen 4 adds Zed's `buffer_diff`, `language` and `text` (already in `Cargo.lock` through `editor`) and `grammars` with its tree-sitter grammars (new; owner, 2026-10-23) as direct dependencies, at the pinned rev. The Natamux theme, fonts, icons, the command registry, shared parts (dialog, menus, toasts, skeleton), the view state, the shell and screen 1 |

## Get the beta

`.github/workflows/native-beta.yml` makes the pre-release `native-beta.N` by hand (Actions, **Native beta**, run on `dev`). Inputs: the id of a finished, green **Rust** run of `dev`, and N. The workflow checks the run, downloads `natamux-app-macos-arm64` and `natamux-cli-macos-arm64` from it, runs `gh attestation verify` on each (signed by `rust.yml` on `dev`, for the run's commit), and makes the release on the run's commit with `--prerelease --latest=false`.

GitHub lists a `workflow_dispatch` workflow only when its file is on the default branch, so `native-beta.yml` is on `main` too, from the small PR into `main` that adds the README line. Its job runs only when started on `dev` (`if: github.ref == 'refs/heads/dev'`).

The pre-release does not touch the Electron app:

- the updater reads `/releases/latest`, which skips pre-releases;
- `release.yml` runs on `v*` tags only, and `native-beta.N` does not match.

All releases of the beta channel: <https://github.com/lenhatthanh20/natamux/releases?q=native-beta>. macOS Apple silicon only. Windows stays on Electron until parity.

```sh
gh release download native-beta.N -R lenhatthanh20/natamux -p natamux-app-macos-arm64.zip
gh attestation verify natamux-app-macos-arm64.zip -R lenhatthanh20/natamux
ditto -x -k natamux-app-macos-arm64.zip .
xattr -dr com.apple.quarantine "Natamux Native.app"
open "Natamux Native.app"
```

The app is not signed yet. If MDM blocks it, ask IT first. How it finds the daemon: `rust-phase1.md`, "Point it at a daemon".

## Get the CI artifact

Every Rust run on a PR into `dev` uploads the build, attested, and the shots:

```sh
gh run download <run-id> -R lenhatthanh20/natamux -n natamux-app-macos-arm64
gh run download <run-id> -R lenhatthanh20/natamux -n shots-macos
```

`shots-macos` holds every shot (`<state>-<theme>-<size>.png`) and, for each shot that has a baseline, `diff-<state>-<theme>-<size>.png`.

## Flags

The flags of `rust-phase1.md` stay. New:

| Flag | Does |
|---|---|
| `--shot STATE` | Opens the window in one named state of the fake daemon (it implies `--fake-daemon`). Use it with `--theme` and `--size`. The clock stops at the fake's time and the view state is fixed and never saved, so each run looks the same. The states are below |

## What CI checks

| Job | Checks |
|---|---|
| `check` | `cargo xtask ci`: fmt, clippy, all tests, the layers, and the colour-literal check (below). `scripts/compare-shots.test.py`. A push to a branch whose name ends in `-check` runs only this job |
| `app-macos` | Release build, `--smoke`, the GPUI-drew-it check, the screen-1 shots and their comparison, the size of the zipped app (at most 60 MiB), then attest and upload |
| `app-windows` | Release build and `--smoke` still pass. Runs on PRs into `dev` and on `dev`; snapshot pushes to `natamux-ci/**` skip it, as it is the slowest job |
| `live-daemon` | `natamux-client` against the daemon image of main, including the screen-1 flow (create a task, rename it, add a subtask, start a workspace, see `workspace.preparing` end, remove the workspace, delete the task) the screen-2 flow (create a task and its workspace, open Terminal 2 by socket, close it, read the ended terminals, push, remove the workspace, delete the task) the screen-3 flow (read the settings and the stage files, edit the notes and priority, set and clear a skip mark, skip a stage, send the task back, save a template, run one more try, then delete the try and the task) the screen-4 flow (`tests/live_daemon_git.rs`: a new workspace has no changes and a graph with commits; commit, the commit message and undo answer 409, a file or an image that did not change 404, a message to the agent is taken (or refused with 409 before the agent runs); the daemon keeps its worktrees in its own volume, so the test cannot change a file there), and the screen-6 flow (`tests/live_daemon_files.rs`: read the tree and a file, save it with its version, save with the old version (409), a path outside the worktree (409), write and read a stage file, list the attachments), and the screen-7 flow (`tests/live_daemon_projects.rs`: a wrong token answers 401 and the right one the task list, the projects, adding a relative path or a project that is there already (400), the GitHub issues and their import (409 with no signed-in `gh` in the image), an Issues-tab setting saved and put back). Each flow cleans up after a failed step too |
| `supply-chain` | `cargo deny`, `cargo audit`, `cargo vet` |

Rust tasks push their CI snapshots to two shared branches (owner, 2026-10-23): `natamux-ci/rust` runs every job (the shots), `natamux-ci/rust-check` only `check`, for the compile loop. Their Actions caches stay warm from task to task, so they are kept at cleanup; a task deletes only the temp branches it made.

The tests in `cargo xtask ci` that guard the look and the keys:

- **Contrast.** Both Natamux themes: every text pair at least 4.5:1, every glyph, focus ring and border pair at least 3:1 (`ui-spec.md` section 6). A test with a bad pair proves it fails.
- **Colour literals.** `cargo xtask colours` fails on `rgb(`, `rgba(`, `hsla(`, a hex integer such as `0x1b2a4aff` and a quoted `"#1B2A4A"` in `crates/natamux-app/src`, except in `components/colors.rs` and the hex data of the contrast test (`components/theme_contrast.rs`), by exact path. Views read the theme.
- **Keys.** Every key and menu item comes from the command registry. Two commands with the same key in the same context fail a test.

## Screenshot baselines

`app-macos` shoots each state in both themes (`light`, `dark`) at 1000x680 and 800x560: 73 states, 292 shots, and the 4 shots of the window check, 296 in all. Both sizes are under 1024, so the sidebar is a drawer there, and the details panel a sheet over main with a scrim. The runner's screen is 1024x768, so `tasks-table` and the states of screens 2 to 7 are shot at 1024x720 too, where the sidebar docks and the details panel is a sheet without a scrim: 116 more shots, 412 in all. The details panel docks only from 1280, so no shot shows it docked: run 39863431903 asked for 1280x720 and every one came out 1024x684 with the sheet, so that size was dropped (see "Screen 3: where it differs"). macOS keeps the window below the menu bar, so these come out 1024x684; the width, which docks the sidebar, holds. If a runner gives the window less than 1024 wide, the comparison says the sizes differ. A shot is the app's window itself (`screencapture -l` with the window id, found by the app's process id), so the menu bar is not in it and the status bar is. Baselines are the PNGs in `crates/natamux-app/tests/shots/`, named `<state>-<theme>-<size>.png`.

`scripts/compare-shots.py` compares a shot with its baseline. It fails when more than 0.5% of the pixels differ by more than 16 in a channel, or when the sizes differ, and it writes a diff image (differing pixels in red). The job summary lists each shot as matches, differs, or no baseline yet.

Until the owner signs screen 1 off, the compare step only reports: the job summary says `matches` or `DIFFERS` for each shot, and the step does not fail. New baselines come from the artifact of a CI run, after sign-off. Turning the step to fail (empty `REPORT_ONLY` in `rust.yml`) is the last commit before the PR is ready.

### Screen-1 states

| State | Shows |
|---|---|
| `tasks-table` | All tasks as a table, one row selected |
| `tasks-board` | The same tasks as a board |
| `board-hover-card` | A board card under the pointer |
| `add-task-typed` | Quick add with a typed backlog title |
| `start-task` | The Start a task dialog |
| `remove-workspace` | The Remove workspace dialog: the terminals that end and the branch choice |
| `clear-finished` | The Remove finished workspaces dialog |
| `add-subtask` | The Add subtask dialog |
| `task-context-menu` | A task's row menu |
| `skeleton` | The sidebar and table skeletons |
| `toast` | A toast over the list |
| `empty` | No tasks |
| `filter-miss` | A filter with no match |
| `no-projects` | No projects |
| `load-error` | "Could not load tasks" with Retry and the raw message |
| `drawer` | The sidebar as a drawer (under 1024 wide) |

### Screen-2 states

The fake daemon's tasks stand in for the storefront tasks of `docs/ui-reference/shots/`.

| State | Shows |
|---|---|
| `workspace-terminal-running` | T-0146: header, tab row, a side by side group (Terminal 1, 10, 12) and a plain terminal |
| `terminal-find` | T-0143: the find bar on Terminal 1 with "src", match 1 of 5 |
| `workspace-terminal-needs-input` | T-0141: one agent pane, its question in the terminal |
| `workspace-terminal-review` | T-0143 after review: five agent panes, Push ↑2 and Open pull request |
| `no-workspace` | T-0135: Branch from, Create workspace, Start agent |
| `workspace-preparing` | T-0135 while the daemon fetches the base branch |
| `finished-wrap-up` | T-0139, done: the finished banner, the wrap-up rows, a running terminal and five ended ones |
| `agents-grid` | The Agents grid with two panes (T-0141, T-0146), Add a pane and the columns |

### Screen-3 states

The fake daemon's pipeline stands in for the storefront one: Context, Spec, the branch "Bug or feature" (Bug fix: Reproduce the bug; Feature: Plan), the group "Test spec and Implement", Review, Security review, Manual review, Release.

| State | Shows |
|---|---|
| `task-details-review` | The table with T-0143 picked, its panel on Details: Approve, Request changes, Restart agent, the sections |
| `task-details-needs-input` | The table with T-0141 picked, its panel on Details: Resume |
| `answer-banner` | T-0141's workspace, the panel on Progress: the question, Approve anyway, Answer in terminal, Resume, the agent picker, Start agent, Mark as done, Skip, Send back |
| `workspace-terminal-plan-gate` | T-0150's workspace, Progress: Bug fix not taken, Plan in review with plan.md, its checks and Approve |
| `workspace-progress` | T-0143's workspace, Progress brought to Manual review, Context and issue below |
| `stage-failed` | T-0151's workspace, Progress: the reason box, the agent picker, Start agent, Skip, Send back |
| `send-back` | The Send back dialog over T-0143, a reason typed |
| `details-details` | T-0143's workspace with its panel on Details |
| `compare-tries` | T-0152 (a try of T-0135), Details brought to Tries |
| `details-more-menu` | The "..." menu of T-0143's panel |
| `fan-out` | The Run more tries dialog over T-0135, which has no workspace |

### Screen-4 states

| State | Shows |
|---|---|
| `workspace-changes-empty` | T-0144's workspace on Changes: No changes |
| `workspace-changes-list` | T-0143 on Changes: the two changed files, `src/checkout.ts` side by side, the commit box and the comment box |
| `workspace-changes-diff` | The same, the cursor in the second change ("2 of 2") |
| `workspace-changes-ask` | The same file inline, lines 5 to 6 picked and Ask agent pressed: the "Lines 5–6" chip (T-0117; no reference shot) |
| `rollback` | The Roll back changes dialog over T-0143: both files checked, "1 new file will be deleted." |
| `image-diff` | T-0147 on Changes: the new `public/banner.png`, After only, on the checkerboard, with its size |
| `workspace-history` | T-0143 on History: the summary, the lanes, the ref chips with "+1" |

### Screen-5 states

The fake's `natamux` project has a `.natamux.json` with the reference pipeline (Reproduce the bug on Cursor with a prompt of its own, Test spec on Gemini CLI, Review on Codex); `natamux-site` has none.

| State | Shows |
|---|---|
| `tasks-progress` | All tasks as Progress: one section per project, the stage columns with their counts and waits, the markers, Next. The fake's screen-1 tasks carry no stages, so they make the section "natamux, earlier pipeline" with no stage column, as the web does for such tasks |
| `project-settings-tab-stages` | Project settings of natamux as it opens: the preview with the branch and the group, the stage list, Context's details |
| `project-settings-agent-stage` | The same, Reproduce the bug picked: its agent, arguments and the prompt of its own |
| `project-settings-stage-picker` | Add stage: the catalog stages by phase and the Custom stage form |
| `project-settings-branch` | Bug or feature picked: the branch details with its two paths |
| `project-settings-problems` | Lint and types added with no command: the problem under its row, the preview's danger ring, "1 problem", Save off |
| `project-settings-conflict` | Save after another client changed the file: Reload and Overwrite |

### Screen-6 states

The fake's T-0143 has a worktree like the reference's storefront (`app/fake_files.json`): folders, `README.md`, `src/checkout.ts` with the change of screen 4, a binary and a too large file, six stage files and two attachments.

| State | Shows |
|---|---|
| `details-files` | T-0143's workspace, the details on Files: the hint and Refresh, the root rows, `src` open with `checkout.ts` (M), the modified dot on `src`, the file icons |
| `workspace-docs` | T-0143 on Docs: the stage files and the attachments in the list, `context.md`, Rendered \| Source and Edit |
| `workspace-docs-edit` | The same, `spec.md` in edit with a change: `edited` in the list, Cancel and Save |
| `workspace-editor` | `README.md` in the editor tab, the details on Progress |
| `workspace-editor-source` | `src/checkout.ts` in the editor tab (TypeScript colours), the details on Details |
| `workspace-editor-draft` | `src/checkout.ts` with a draft: Modified, Revert and Save on; the details on Files with `edited` on its row |
| `workspace-editor-conflict` | The draft after the agent wrote the file: the banner with Reload and Keep mine, Save off |
| `terminal-drop-target` | T-0146's terminals, Terminal 1 with the drop outline, the details on Files. CI cannot drag, so the shot sets the pane's drop state |

### Screen-7 states

The fake's `natamux` project has three open GitHub issues (one imported already); the Import issues shot turns on the fake's "no `gh`" switch, as the reference's demo has no `gh`.

| State | Shows |
|---|---|
| `palette` | All tasks, the palette empty: Needs you, Recent, Suggested, the footer |
| `palette-query` | The palette with "stage": the matching commands, the matched letters in bold |
| `palette-no-match` | A query with no match: Add task and Start task with the query |
| `palette-jump` | The jump palette as the needs-you pill opens it: the Needs you scope chip, the waiting workspaces and their terminals |
| `shortcuts` | The shortcut sheet: the groups of the keyboard map |
| `account-menu` | The account menu above the status bar's Account item |
| `terminal-font` | The terminal font dialog over T-0146's terminals |
| `settings-dialog` | The Settings dialog: Appearance and Projects |
| `projects` | The Projects dialog: the projects and the form with the daemon's worktree folder |
| `project-settings` | Project settings of `natamux` as it opens: the three tabs, Stages |
| `project-settings-tab-issues` | The Issues tab: the four choices, Manual with Default |
| `project-settings-tab-json` | The JSON tab with the file |
| `project-settings-json-error` | The JSON tab with text that is not JSON: the alert, "1 problem", Save off |
| `import-issues` | Import issues with no `gh`: the error, Retry and Show details |
| `sign-in` | Sign in with an empty field: Connect off |
| `sign-in-error` | Sign in after the daemon refused the token: the message and the danger border |

## Where it differs from the React screens

- **Hidden controls.** None since screen 7. Controls that opened a screen not built yet were hidden until it shipped: the details toggle, **Show details** in the table's row menu and the status bar's notification item came back with screen 3; View changes, Commit…, and the Changes and History tabs with screen 4; the Progress layout and project Settings (the project menu and the card's gear) with screen 5; the Files tab, the Docs tab and key 4, **Show the open file**, **Show the files of the worktree** and the output chip of a stage card with screen 6; the command center button, Add a project, the status bar's Projects, Import issues, Keyboard and Account items, and the jump palette of the needs-you pill with screen 7. Two parts of the web stay out on purpose: Sign out (the Electron app has none either) and the Desktop app part of Settings (see "Screen 7: where it differs").
- **View state in a file.** The React app keeps `natamux.*` in the browser's localStorage. The app keeps one `view.json` in its data folder (`~/Library/Application Support/Natamux Native/view.json`, `%APPDATA%` on Windows), written atomically: sidebar folds (at most 300), blurred projects, pinned tasks, layout, sidebar open and width, show finished, seen states (at most 1000), the last base branch per project, and since screen 7 the theme, the UI size, the terminal font and the workspaces opened last (at most 20; the palette's Recent). A broken file is ignored and replaced. A token typed into Sign in is kept next to it in `daemon-access`.
- **Pinned and folds are per app.** They are not shared with the browser or the Electron app, which keep their own.
- **Opening a task.** Enter or a double click opens the task's workspace (screen 2, below). The terminal keeps Esc, so an agent can be stopped with it (owner, 2026-10-22). Cmd+[ on macOS, **View → All tasks** or the sidebar go back.
- **Looks.** The widgets are Zed's `ui` kit with the Natamux theme, so spacing, focus rings and menus follow Zed, not the CSS of the dashboard.
- **No phone layout.** It stays in the web app. The needs-you notifications and the screen-reader announcement came with screen 3 (below).
- **Tools menu.** The perf overlay, the bench and the IME check are commands of the **Tools** menu.
- **`--gate2`.** It scrolls the sidebar tree and a bench terminal while the bench runs, then shows the task table in main while the bench's 10 shells keep printing out of sight, and scrolls the table for 20 s. Then it opens the task's workspace with four of the bench's shells as panes, still printing, and scrolls the first pane for 20 s (screen 2). Then it closes the bench and types into a quiet shell. It saves four perf reports: bench, table, panes and typing. Every report has `start.first_frame_ms` and `start.first_data_ms`, the times from the start of the app to the first frame and to the first task list.
- **Smaller differences.**
  - Ages of a week or more show the date in UTC: the app has no local time zone helper yet.
  - The needs-you pill and the status bar's "N need you" open the jump palette on what needs you, as the web (since screen 7; before, they filtered the list).
  - The status bar has a fourth link word, "Not connected", while the daemon cannot be used. A token the daemon refuses opens Sign in, as the web (since screen 7). It does not measure overflow; labels drop by width only. The jobs menu does not update while open, and its spinner is a still icon.
  - The drawer has no Tab focus trap. Esc and a click on the scrim close it.
  - Single-key commands are not in the menu bar, because a macOS menu fires a key without a modifier while the user types. Off macOS, Ctrl chords without Shift go to the shell inside a terminal.
  - New command ids: `app.quit`, `list.first` and `list.last` (the web's `list.ends`), `tree.expand` (from `tree.collapse`), and `tools.*`. The menu items that the web writes in its components are commands too, with no key: `task.add-subtask`, `row.pin`, `row.unpin`, `row.copy-branch`, `row.copy-path`, `task.copy-id`, `agent.start`, `agent.restart`, `task.cancel`, `task.delete`, `project.blur`, `project.show-names`, `project.remove-finished` and `project.remove`. A menu item shows the key of its command: F2 for Rename, Delete for Remove workspace, Enter for Open workspace.
  - Blurred projects show bullets in place of the text: GPUI has no text blur. Rows take a 2 px focus border. Esc in an empty filter moves the focus to the tree, so the single keys work at once.
  - In the sidebar tree `j` and `k` move the selection, so they are not type-ahead letters.
  - Board: a folded column's name is in its tooltip (no vertical text); no edge fades or scroll arrows. Table header cells are mouse-only. A disabled Start agent item has no "Waits for …" hint.
  - Start a task: the branch and skill pickers are plain menus with no search; pasting a screenshot is not supported. The files-not-committed dialog of Remove workspace offers Close, View changes and Commit… (since screen 4).
  - Agent icons (T-0148): the web's icons, `claude`, `gemini-ai`, `copilot`, `cursor` from material-icon-theme 5.39.0 (MIT) and the app's own `codex` (shape from lobe-icons, MIT), in `assets/icons/agents/` with both licences. In the light theme a `_light` icon is used when there is one. An unknown agent shows the generic bot icon.
  - The skeleton does not pulse, the toast timer pauses on hover only, the resize handle collapses during the drag, and context menus have no type-ahead.

## Screen 2: where it differs from the React screens

- **Tabs.** Terminal, Changes, History and Docs (keys 1 to 4) since screen 6, and the editor tab while a file is open.
- **Add to Agents** is Shift+Enter (or Shift and a click) on a task in the palettes, as in the web (screen 7). Commit… and View changes came back with screen 4.
- **Pickers.** "Branch from" and "Add a pane" are plain menus with no search, as the pickers of screen 1.
- **Times.** "ended 7:05" of the wrap-up and "Finished" of the summary are in UTC and say so: the app has no local time zone helper yet.
- **Spinners.** The connection strip, the restore cover and the preparing step show a still loader icon, as the jobs menu does: an endless animation would draw the window every frame. As in React, "Connecting…" and "Reconnecting…" show only after 200 ms, so a quick connect shows no strip; a terminal that could not start says so at once.
- **Close a terminal.** No confirm, as in the React screen (`ShellPanes.tsx`), although `ui-spec.md` 4.23 asks for one when a program other than the shell runs. The wrap-up's Stop asks first, as React does.
- **Grid.** The panes sit in flex rows; the last pane of the last row takes the rest of it, as `paneSpan` does in the CSS grid.
- **Hidden terminals.** As `terminal-cache.ts`: at most 8 terminals stay connected off screen, each at most 20 minutes; the terminals of a task that is gone, finished or has no workspace go once hidden. The 20-minute limit is checked when the app draws, not by a timer.
- **No phone layout**: no pill switcher of terminals and no "Git actions" menu.
- **`--gate2`** shows the panes grid with four bench shells printing and saves the `panes` report (screen-1 section, `--gate2`). The bench shells are plain terminals, so their panes show the terminal mark, not an agent's.

## Screen 3: where it differs from the React screens

- **Details panel modes.** Docked at 1280 and wider while main keeps 640, a sheet from 768 to 1279 (with a scrim under 1024), as `decideLayout`. The window is at least 800 wide, so the page mode of the web (under 768) never shows. The panel is 320 to 560 wide, kept in `view.json` (`detailsWidth`). As the web, the open state of each task's panel is not kept across runs; the picked tab is (`detailsView`).
- **Tabs.** Progress, Files (since screen 6, only for a task with a workspace) and Details; a remembered Files tab shows Progress for a task with no workspace.
- **Output chip.** Since screen 6 the output file chip of a stage card opens the file in the Docs tab. A chip of a file that is not written yet opens nothing.
- **Markers do not move.** The marker of a running stage and the line into it are still, as the screen-2 spinners: an endless animation would draw the window every frame.
- **Stage commands.** Skip the stage, Mark the stage as done, Approve the stage anyway, Send back and Show the task progress run from the palette, as in the web (since screen 7). The menu bar's **Task** menu keeps them too, with no key.
- **Keys.** `a` and `r` work from the details panel too (not in its text boxes, nor in a terminal); `e` edits, `i` shows or hides the selected task's panel, Mod+Alt+B shows or hides the panel. The phase-1 bench had Mod+Alt+B: it is Mod+Alt+T now. F6 does not cycle the parts of the window.
- **Show details** is in the table's row menu, as in the web; the sidebar's row menu has none, as in the web.
- **Notifications.** GPUI has no system notification API, and the app is not signed (macOS Notification Center needs a signed bundle). So a task that starts to need the user, or becomes done, while the app window is not active shows a small window of the app at the top right of the screen for 10 s, as Zed's agent notification does; a click brings the app forward and opens the task. The status bar's item is the app's own switch (**Notify me** / **Notifications on**, `notify` in `view.json`), off by default as the web starts without the browser's permission.
- **Screen-reader announcement.** The app works out the text of `NeedsYouAnnouncer` ("<title> needs you"), but GPUI has no live region to speak it yet, so it is not announced.
- **Times** in the panel ("2m ago", "Created", "Updated") are in UTC, as on the earlier screens.
- **Pickers.** "Start from branch", "Back to", the agent picker, the priority and the number of tries are plain menus.
- **Docked panel shots.** The runner's screen is 1024x768, and macOS keeps a window on the screen: the 1280x720 shots of run 39863431903 all came out 1024x684 and showed the sheet. So the screen-3 states are shot at the three sizes of the other screens, and the owner checks the docked panel on the CI build.

## Screen 4: where it differs from the React screens

- **The diff is Zed's editor**, not Monaco: read-only editors over the working-tree text with `buffer_diff` against the last commit. Inline is one editor with the deleted lines between the new ones. Side by side is two editors kept level by empty rows (Zed's own split view needs a project and a workspace, which the app has not); a scroll of one side scrolls the other. With Wrap long lines on, the two sides can drift apart where a long line wraps on one side only.
- **Syntax colours** come from Zed's built-in grammars (`grammars` with `load-grammars`: Bash, C, C++, CSS, Diff, Go, JavaScript, JSON, Markdown, Python, Rust, TypeScript, TSX, YAML and a few more), picked by the file's extension, with no language servers. A file of another language is plain text; Monaco knows more languages.
- **No diff options menu.** `buffer_diff` cannot ignore leading and trailing white space or show moved code, so "Ignore leading and trailing whitespace" and "Show moved code" are not offered. "Only whitespace or line endings changed." shows when the diff has no change; there is no [Show whitespace].
- **Collapse unchanged lines** keeps 3 lines around each change, as Monaco's context; Zed's expand buttons reveal more, in place of Monaco's 20 lines.
- **Side by side** off for want of room: the toggle shows Inline and its tooltip says "Needs a wider window"; a click still stores the choice for a wider window.
- **Ask agent** is Mod+Shift+A with lines picked in either side (and the comment box's label shows the quote). There is no floating button at the cursor nor an item in the editor's right-click menu.
- **The file tree** keeps its folded folders per task while the app runs, as the web keeps them while the page is open. The narrow picker ("Changed file") is a plain menu.
- **Icons.** The History tab, Collapse all, the fold, wrap and image toggles and the file rows use Zed's icons, as the app's Lucide set lacks them.
- **History's loading state** is the skeleton with no "Reading the history" text: GPUI has no screen-reader label yet.
- **Ages** in History are in UTC, as on the earlier screens.
- **Shot sizes.** At 1024x720 the sidebar docks and main is under 720 wide, so the Changes states show the narrow layout (the "Changed file" picker, Inline); at 1000x680 the sidebar is a drawer and they show the file panel and Side by side, as the reference does at 1440.

## Screen 5: where it differs from the React screens

- **Tabs.** Stages, Issues and JSON over one draft since screen 7. A file the Stages tab cannot read opens on the JSON tab, as the web, also when it comes from Import (screen 5 refused it there).
- **A file the app cannot read** (one the daemon reads, with a field the app's types cannot hold) opens with its text on the JSON tab, which says why (screen 5 started from the defaults).
- **Tab** moves through the stage rows, Add stage, the text fields on screen (or the picker's), Cancel and Save, in the order they are drawn. The checkboxes and the pick menus are not on it yet.
- **Dialogs over dialogs.** The questions of the editor (Discard your changes?, Reset to default, Remove, Import, a broken file) open over Project settings, which keeps its state under them.
- **Stage keys** are written in the order of the schema, not of the file, and a key the schema does not know is dropped from a stage when the steps are written back. A field of the wrong JSON type cannot be read: the file is refused at Import (or the screen starts from the defaults), where the web shows the problem next to its stage.
- **Messages.** The pipeline problems copy zod's texts; "This is not JSON: …" gives the JSON reader's own message.
- **Export** asks where to save `<project>.natamux.json` and says "Exported <file>."; the web downloads it.
- **Settings read again.** The daemon sends no event when a project's settings change. As the web reads them again when its tab gets the focus, the app reads them again when its window becomes active, when the project list changes, and after its own save. The open dialog keeps its draft; a change by another client shows as Reload and Overwrite on Save.
- **Pickers.** The agent, kind, condition, "Add a stage…" and Insert skill pickers are plain menus with no search, as the pickers of screen 1.
- **Icons.** The app's Lucide set lacks BookOpen, FlaskConical, Rocket, Hand, UserRound, GitFork and GripVertical: Zed's Book, ToolHammer, PullRequest, Person, GitBranch and Menu icons stand in (`stages/kind_icon.rs`), and the Progress segment uses ListTodo, until the SVGs come.
- **Progress.** The marker's tooltip names the status without the stage's name; the stage columns scroll sideways between the fixed Task and Next columns; a row's title has no underline on hover, the row is tinted.
- **Keys.** `stage.move` of the web is two commands, `stage.move-up` (Alt+Up) and `stage.move-down` (Alt+Down), in the stage list. Mod+Enter saves through the dialog's own key.
- **Shot sizes.** The editor states scroll the dialog to the stage list. At 1024x720 the list and the details sit side by side; at 1000x680 and 800x560 they are one column, so those shots show the list and the details are under it.
- **Window sizes.** The window is at least 800 wide, so the web's phone rules of the editor and the preview never apply; the list and the details sit side by side from a 1024-wide window.

## Screen 6: where it differs from the React screens

- **The editor is Zed's editor**, not Monaco: a writable buffer with no project, line numbers, the syntax colours of the grammars of screen 4 (a file of another language is plain text), soft wrap for prose files. A new text on disk goes into the same buffer, so the cursor and the scroll stay, and that change is in the undo history. Save is Mod+S through the app's command `file.save`.
- **Multi-line editors** (the editor tab, the Docs edit) take Enter, Tab, Shift+Tab, the arrows, Page Up and Page Down, and the moves to the start and end of the text. Single-line boxes leave the arrows and Enter to the lists and forms around them.
- **Markdown.** Until the owner OKs Zed's `markdown` crate, the Rendered view shows the text as it is, wrapped. With the crate: Zed's renderer with no HTML, no Mermaid, no image loading and only web and mail links, in Zed's spacing.
- **Drop on a terminal.** A path drags only inside the app, onto a terminal pane of the same task. The paste is bracketed when the program asked for it, as xterm does, and the pane takes the focus. A drag to another app is not offered.
- **Downloads.** An attachment that is not an image asks where to save it and says "Downloaded <name>."; the web downloads it.
- **Quitting** with edits that are not saved asks first, also for Docs edits; the web asks only for files of the editor, through the browser.
- **Reads.** As the web reads only the queries on screen, `workspace.dirty` and the 60 s fallback poll read again only the Files tree (while the panel shows Files), the Docs list or the open file (while its tab shows); a hidden one reads again when it shows (`dirty_reads`). The git state of the open workspace reads as before.
- **A path outside the worktree** is answered 409 by the daemon (`OutsideWorkspaceError`), not 400.
- **Icons.** The app's Lucide set lacks RefreshCw, FileCode and FileImage, and the file icons of material-icon-theme are not in the app yet: Zed's RotateCw, FileCode, Image and `File*` icons stand in (`files/file_icon_view.rs`), until the owner answers. The Rendered \| Source switch shows icons with its labels.
- **Sizes.** The `edited` and `Modified` marks use Zed's smallest label size (the web's 2xs is smaller). The editor tab and the Docs edit are mono 12; the source view of Docs uses the buffer font size of the theme.
- **Shots.** At the three shot sizes the details panel is a sheet over main, so it hides the right part of the editor and Docs headers (Revert, Save, the view switch, Edit); the owner checks them docked on the CI build.
- **Window sizes.** The Docs list is always a column on the left: the window is at least 800 wide.

## Screen 7: where it differs from the React screens

- **The palettes** are the app's own view, not Zed's picker (that crate is not in the lock): the web's ranking (`fuzzy.ts`) is ported as it is, so the same query gives the same rows. The terminal rows of the jump palette come from the terminal lists the app already reads (the sidebar's, the open workspace's and the Agents grid's); the web reads them for the jump palette itself when there are at most 20 workspaces. Alt+1 to 9 match the key, not the typed character (Alt+digit types a symbol on macOS). The footer does not wrap.
- **Sign in.** The token typed in is kept in `daemon-access` in the app's data folder (mode 0600), not in the browser; the line says "It stays on this computer." Connect asks the daemon for the task list first: a refused token keeps Sign in with the web's message, and another error (the daemon is down) shows its message there too, where the web would open the dashboard and show the load error. A 401 later deletes only the app's own file and opens Sign in; the Electron app's `daemon-token` and `NATAMUX_TOKEN` are never changed. The macOS Keychain would need a new crate. There is no Sign out item, as in the Electron app.
- **Settings** has no Desktop app part (keep the computer awake, check for updates): those are settings of the Electron app. Theme and UI size use a text-only segmented control.
- **UI size** sets Zed's UI and buffer font sizes (16 and 15 times 0.92, 1 or 1.077): parts drawn in rems follow, parts with a fixed size in pixels (row heights, the title and status bars) do not, where the web's CSS zoom scales everything. The terminal font size is not scaled.
- **Terminal font.** With no font picked, the terminals use Ghostty's font, else JetBrains Mono 13 (the web's size; before screen 7 the app used Zed's terminal size). Ghostty's config is read once when the app starts.
- **The account menu** opens where it was clicked, kept inside the window. "Terminal font…" and "Report an issue" show Zed's FontSize and the Lucide triangle-alert icons until the Lucide `type` and `bug` SVGs come.
- **Projects** has no Browse buttons: the web shows them only in the Electron app, and the daemon may run on another machine. The text fields use the buffer font.
- **Import issues** shows the raw message behind the empty state's own "Show details".
- **Status bar layout rules.** The bar uses only the width rules (`forced_by_width`), as before; the measured condensing of the web (`condense`, the `measure` cases) is ported with its tests but not used yet, so those items keep item-level `allow(dead_code)` in `app/status_layout.rs`.
- **The JSON tab** is a Zed editor with JSON colours and no line numbers, not a textarea; Tab types a tab there, so Tab does not leave it (Esc closes the dialog as before).
- **Seen in the CI shots** (run 39979437375):
  - Palette: the first nine rows show their Alt key (⌥1 to ⌥9) at the right, where the web only names it for screen readers; the footer says "Tab needs you" where the web draws ⇥. Suggested lists the commands in the app's registry order (View: Agents before Task: Start a task); the web starts with Jump to a workspace and Show the keyboard shortcuts. At the shot sizes the list ends inside Suggested and scrolls.
  - Account menu: the section titles are "Theme" and "UI size" as written, not in caps; "Terminal font…" has the Zed FontSize icon.
  - Project settings: the tabs are Zed's tab bar (boxed tabs on a strip), not the web's underlined tabs. A Save, Import or Add project that is off keeps its colour and only stops the click, as on screen 5; the web greys it. On the JSON tab the editor is 22 rows high, so at the shot sizes the parse error and the problem list sit under it, below the fold (the footer shows "1 problem").
  - Import issues: Retry is a plain button with no border.
  - Projects: the fake daemon's worktree folder is `/work/.natamux/worktrees`, the reference's demo `/data/worktrees`.
  - Sign in: `NATAMUX_TOKEN` in the help line is not in the mono font; the window keeps the traffic lights and no title bar.
- **Keys.** F6 and Shift+F6 (move between the parts of the window) and the accessible diff view are not in the app, so the shortcut sheet does not list them. The sheet lists "First or last row", "Collapse or expand a group" and "Move a stage" as one row each, as the web, although the app has a command per key.

## Ported tests

Each TypeScript case of spec Part D item 11 has a Rust test of the same name, except:

- `it.each` cases whose name has `%s` placeholders take a plain Rust name: "at %s is %s" (`background-jobs.test.ts`) is `formats_elapsed_time`, and "shows %s as %s (short) and %s (long)" (`tasks.test.ts`) is `shows_each_time_as_short_and_long`.
- `formatAge` "is short by default": Rust has no default arguments, so every caller of `format_age` passes its `AgeStyle`. The short case is in `shows_each_time_as_short_and_long`.
- `actionsFor` "only offers events the daemon accepts for that state": it runs core's `applyTaskEvent`, which has no Rust port. The event names are copied from the TypeScript table.
- "the last used branch is kept for each repository" (`lastBase`, `rememberBase`): it tests localStorage. `app/view_state.rs` has its own tests for the last base per project.
- React, DOM and store tests (`WorkspaceList.test.tsx`, `sidebar-static.test.ts`, the `useViewStore` route and persistence cases): the pure rules they cover have new tests in `sidebar/tree_model.rs` and `sidebar/sidebar_text.rs`.
- Cases that check object identity or one-at-a-time async deletes check equality and order instead: Rust has no object identity, and the delete loops are sequential.

Screen 2 (T-0148): every case of `lib/__tests__/workspace.test.ts` (`needsCommit`, `pushButton`, `pullRequestButton`, `nextGitStep`, `latestQuestion`, `reviewStages`), `finished.test.ts` (`isFinishedState`, `finishedBanner`, `wrapUpRows`, `finishedAt`), `panes.test.ts`, `terminal-tabs.test.ts`, `terminal-find.test.ts` and `notify.test.ts` (`finishedText`) has a Rust test of the same name, except:

- `terminalState` (`panes.test.ts`): the rule was ported with screen 1 (`sidebar/shell_rows.rs`), with its own test.
- `waitReason`, `bannerText`, `rowText` (`workspace.test.ts`): ported with screen 1 (`natamux-proto` and `sidebar/sidebar_text.rs`).
- The cases of `panes.test.ts` that build the task list with `upsertTask` and `sortByRecent` use a small local helper with the same order.
- React component tests (`ShellPanes.test.tsx`, `AgentPanes.test.tsx`, `WrapUp.test.tsx`, `FinishedBanner.test.tsx`, `NoWorkspace.test.tsx`, `WorkspaceHeader.test.tsx`, `TerminalFind.test.tsx`): their pure parts have new tests (`pane_facts`, `agent_lanes`, `restoring_label`, `agent_to_reopen`, `ended_meta`, `no_workspace_start`, `pane_candidates`, `with_pane`, the terminal cache's `evictions`); the rest is covered by the shots.
- `find_pattern` has no TypeScript twin: xterm's search takes the options, alacritty's takes a regex, so the app writes `(?i)` or `(?-i)` and escapes a plain term.

Screen 3 (T-0149): every case of `features/stages/__tests__/stage-view.test.ts`, `features/tasks/__tests__/tries.test.ts`, `usage-format.test.ts`, `task-events.test.ts`, `TaskEditForm.test.ts`, `lib/__tests__/notify.test.ts`, and of the core tests `events/__tests__/progress.test.ts` and the `resolveSteps` cases of `stages/__tests__/default-pipeline.test.ts` (in `natamux-proto`), has a Rust test of the same name, except:

- `stepsKey` (`stage-view.ts`): it only keys a React memo.
- `StageMarker.test.tsx`: `markerLabel`, `linkKind` and the even-pixel rule have Rust tests; the cases that check the SVG and the CSS classes ('draws the node of a branch as a diamond', 'keeps the label for screen readers', 'draws the link into a running stage as one moving fill', the shapes of 'shows … stage') are covered by the shots.
- `Timeline.test.tsx`: 'lists 1,000 events newest first, in order' is a test of `timeline_lines`; 'works the lines out once per read, not on every render' tests a React memo; 'says so when there is no activity yet' is covered by the shots.
- `ProgressTab.test.tsx`, `TaskDetail.test.tsx`, `UsageSection.test.tsx`, `NeedsYouAnnouncer.test.tsx`, `useTaskNotifications.test.tsx`: component tests. Their pure parts have tests of the same name where a rule covers them: the chip, buttons and notices of a card (`stages/card_rules.rs`, `card_facts`, with the names of 'the actions of the current stage' and the `ReasonText` cases), the overview figures, the time bars, the send back stages, `labelText`, the usage rows, the announcement text. The rest is covered by the shots.
- The details cases of `view-store.test.ts` have twins of the same name in `app/view_state_tests.rs` ('opens the details of a task on its progress' tests `ViewState::show_progress`), except: 'opens the details of a task on its files, and remembers that view' (ported with screen 6), and the cases about the browser's storage ('works without a storage', '... when the storage is blocked'): the app keeps `view.json`, and a missing or broken file is the default view (`a_missing_file_is_the_default_view`, `a_broken_file_is_ignored_and_replaced_on_the_next_save`).
- `position.test.ts` and `capabilities.test.ts`: only the cases of the rules the panel uses (`describeCondition`, the paths not taken, `stageName`, `offersWorkflow`, `canRunWorkflow`, `offersCi`).

Screen 4 (T-0150): every case of `features/git/__tests__/change-tree.test.ts`, `graph-layout.test.ts`, `lib/__tests__/changes.test.ts`, the `commentForAgent` cases of `AgentComment.test.tsx` and `formatBytes` of `ImageDiff.test.tsx` has a Rust test of the same name (`git/change_tree_tests.rs`, `git/graph_layout_tests.rs`, `git/change_rules_tests.rs`, `git/comment_rules_tests.rs`, `git/image_rules_tests.rs`), and so have the screen-4 cases of `commands.test.ts` and `view-store.test.ts`, except:

- `RollbackDialog.test.tsx`: its cases are tests of `git/rollback_rules.rs` by the same names; "starts on Cancel" is covered by the shot (the dialog opens with Cancel focused).
- `AgentComment.test.tsx` component cases ('names the lines of the quote, and drops it on remove', 'says when the quote is too long to send', 'keeps the quote when the send fails, and clears it when it works'): the label and chip rules have tests; the rest is covered by the shots and the owner's check.
- `ImageDiff.test.tsx` object-URL and query cases: the app reads the bytes and makes a GPUI image, with no URL.
- `view-store.test.ts` 'keeps a remembered Docs tab after a reload' and the cases of the open file: with screen 6 (below).
- `cutCommitSubject` and `simpleCommitDraft` (`core/git/commit-message.ts`): the daemon drafts the message; they come with the daemon port (phase 4).
- `ChangesPanel`, `DiffToolbar`, `MonacoDiff`, `GitGraph` and `FileBody` have no TypeScript tests. Their pure parts have new tests: `body_kind` and the notices, the commit box rules (`git/commit_rules.rs`), the width rules and the hunk shapes (`git/diff_rules.rs`), the lane geometry (`git/graph_lanes.rs`), `image_size`.

Screen 5 (T-0151): every case of `features/stages/editor/__tests__/{settings-json,move,branches,catalog,find,labels,stage-fields}.test.ts`, `lib/__tests__/skill-picker.test.ts`, the `boardSections` cases of `features/stages/__tests__/ProgressBoard.test.ts`, and of the core tests `stages/__tests__/pipeline-schema.test.ts` (pipeline and branch cases), `schema.test.ts`, `prompt.test.ts`, the `pipelineIssues` and catalog cases of `stage-catalog.test.ts`, the `terminalOfStage` and `canHaveOwnAgent` cases of `terminal-lane.test.ts`, `canEndShell`, and `activeSteps` / `stepRefsOf` of `default-pipeline.test.ts` (in `natamux-proto`) has a Rust test of the same name, except:

- 're-renders only the row of the task that changed' (`ProgressBoard.test.ts`): it tests a React memo.
- `StagePicker.test.tsx`: 14 of 15 have twins in `stages/editor_catalog_tests.rs`; 'adds the stage that is picked' is a click callback, covered by the shots.
- `PipelinePreview.test.tsx`: the texts, labels, tooltips and edges of 26 cases have twins in `stages/preview_rules_tests.rs`; the clicks, Enter and the ARIA parts ('C20 selects with Enter', 'selects the branch or a stage on click', 'selects with Enter on the If button', 'is not a group: no side by side list') are covered by the shots and the owner's check.
- `ProjectSettingsDialog.test.tsx` and `BranchDetails.test.tsx` are component tests. Their pure parts have tests in `projects/settings_text_tests.rs` (`base_of`, `import_question`, `blocked_reason`, `unchanged`) and `stages/editor_branches_tests.rs`; the rest is covered by the shots and the owner's check.
- Core cases with a field of the wrong JSON type (`enabled: "no"`, `endShell: "yes"`, a `workflow` of 1, a group or a branch in a path, a condition type "label"): such a file cannot be read into the app's types, so the Rust tests check that it is refused at read time.
- "fills every field of an empty file" (`pipeline-schema.test.ts`): the daemon fills the default pipeline; the Rust test checks the context sources and the tracker.
- `stageOfTerminal`, `terminalLane`, `terminalAtStart`, `endsShell1`, `pinnedTerminalsOf`, `stageAgent` (`terminal-lane.test.ts`): daemon rules, with the daemon port (phase 4).
- Regression of screen 4: a newer read of the shown file updates the diff's editors in place, so the cursor stays (`a_newer_read_of_the_same_file_updates_the_editors_in_place`, `git/diff_rules.rs`).

Screen 6 (T-0152): every case of `features/files/__tests__/explorer-tree.test.ts` and `file-drafts.test.ts`, `lib/__tests__/file-icons.test.ts`, the files cases of `view-store.test.ts` (the open file cases in `app/tab_rules.rs`) and `commands.test.ts`, 'reads the files and the sizes of a changed workspace again, not its shells' of `live.test.ts` (`dirty_reads` in `app/tab_rules.rs`: what `workspace.dirty` reads), and the files cases of `lib/__tests__/api.test.ts` (in `crates/natamux-client/tests/files_api.rs`) has a Rust test of the same name, except:

- `pathDragType` names a browser drag type: its twin tests `DraggedPath::accepts`, which a pane of another task uses to refuse the drag.
- `file-icons.test.ts`: the `it.each` table is one test (`a_path_gets_its_icon`); 'has every icon the maps name' checks the app's list of icon names until the SVGs come.
- `FileEditor.test.tsx`, `FileTree.test.tsx` and `DocsTab.test.tsx` are component tests. Their pure parts have twins of the same name in `files/file_drafts_tests.rs`, `files/explorer_tree_tests.rs` and `workspaces/docs_rules_tests.rs` (`edit_draft`, `after_save`, `is_stale`, `keep_mine`, the notices, `shown_folders`, `folders_above`, the tree keys, `current_doc`, `is_safe_link`, the view mode, the questions); the rendering, Markdown, task list, HTML, live and error cases are covered by the shots and the owner's check.
- `monaco-setup.test.ts`: it tests Monaco's bundle.
- 'saves without a version when none is given, to overwrite the file' (`api.test.ts`): the app always sends the version it read; Keep mine takes the version on disk.
- 'keeps the file of the Changes tab of the same workspace' (`view-store.test.ts`): the Changes tab keeps the file it shows in its own view (`git/changes_view.rs`), and opening a file in the editor tab does not touch it, so there is no shared state to test.
- The address cases of `view-store.test.ts` and `route.test.ts` ('takes the file of an editor address, and keeps the file of the Changes tab'): the app has no addresses.
- Regression of review 2: a details panel closed on Files and opened again reads its tree again (`reads_the_tree_again_when_a_closed_panel_opens_on_files`, `ShowWatch` in `app/tab_rules.rs`).

Screen 7 (T-0154): every case of `features/palette/__tests__/fuzzy.test.ts` and `palette.test.ts` (`palette/fuzzy_tests.rs`, `palette/palette_rows_tests.rs`, `palette/jump_rows_tests.rs`), `features/projects/__tests__/issues.test.ts`, `lib/__tests__/theme.test.ts`, `ui-size.test.ts`, `features/terminals/__tests__/terminal-font.test.ts`, `packages/desktop/src/app/__tests__/ghostty-config.test.ts`, `features/stages/editor/__tests__/settings-json.test.ts`, the screen-7 cases of `commands.test.ts` and the Issues tab, JSON tab and one-draft cases of `ProjectSettingsDialog.test.tsx` (`projects/settings_tab_rules_tests.rs`) has a Rust test of the same name, except:

- `CommandPalette.test.tsx`: `capRows`, the scope cycle and the create rows have twins in `palette/palette_rows_tests.rs`; 'keeps its rows in place when tasks change while it is open' is the snapshot the view takes at open; the keys, focus, inert page and the 500-workspace render cases are covered by the shots and the owner's check.
- `RepoPanel.test.tsx`: the worktree folder rules have twins in `projects/project_rules_tests.rs`; the Browse cases are not ported (no Browse in the app).
- `TerminalFontDialog.test.tsx` and `SettingsDialog.test.tsx`: their rules are tests of `terminals/terminal_font_tests.rs` and `app/appearance_rules_tests.rs`; the Desktop app cases of `SettingsDialog.test.tsx` are not ported (no Desktop app part).
- `terminal-font.test.ts`: the cases about the CSS font list (the icon font after the picked font, the UI keeping Geist Mono) have no CSS to check; the family and size fallbacks are tested.
- `theme.test.ts` `pageTheme`: it reads the page's `data-theme`.
- `App.test.tsx`: 'goes back to the token form when the daemon rejects the token' is a test of `app/sign_in_rules.rs`; the account menu's items and 'has no sign out in the desktop app, which signs in by itself' are covered by the `account-menu` shot.
- `ProjectSettingsDialog.test.tsx`: 'saves nothing when closed with Cancel' and the import and export file flows are covered by the shots and the owner's check.
- Screen 5's `settings_text_tests.rs` cases that encoded the start from the defaults changed with the JSON tab: `a_file_the_app_cannot_read_keeps_its_text_and_the_json_tab_says_why`, `save_stays_off_for_a_file_the_app_cannot_read_until_it_is_fixed`.

## Gate results

Gate per screen (plan): target numbers met, baselines in both themes, owner sign-off, every state covered with the same behaviour and API calls as the React screen.

| Part | Date | Result | Notes |
|---|---|---|---|
| `cargo xtask ci` | 2026-10-22 | Pass | Rust run 39824338735 on `dev` (66f2aac), job `check` |
| Ported rule tests (same names as the TypeScript tests) | 2026-10-22 | Pass | In `check`; the cases left out are listed above |
| Client tests and live feed | 2026-10-22 | Pass | In `check` |
| `live-daemon` screen-1 flow | 2026-10-22 | Pass | Run 39824338735, job `live-daemon` |
| `app-macos`: shots match baselines | | Open | No baselines yet: the compare step reports only until the owner signs screen 1 off |
| `app-macos`: `--smoke`, zip at most 60 MiB | 2026-10-22 | Pass | Run 39824338735, job `app-macos` |
| `app-windows`: build and `--smoke` | 2026-10-22 | Pass | Run 39824338735, job `app-windows` |
| Frames while scrolling the sidebar tree and the table with 10 agents (`--gate2`, mean of 4 runs) | | | |
| Cold start to the first frame (limit 300 ms) | | | |
| Cold start to the list with data (limit 1 s) | | | |
| Vietnamese Telex and VNI in every screen-1 text box | | | |
| Same API call as the React screen for each action, web dashboard shows the result | | | |
| `native-beta.yml` makes a pre-release, its zip passes `gh attestation verify`, `/releases/latest` still points at the newest Electron release | 2026-10-23 | Pass | Native beta run 39843667186 made `native-beta.1` from Rust run 39824338735 (66f2aac); `/releases/latest` is still v0.1.43 |
| README and site install page, PR into `main`: `scripts/readme.test.py` and the site build | 2026-10-23 | Pass | PR #430, merged; `audit` failed on a DOMPurify advisory of `main`'s own dependencies, not on this change |
| Owner sign-off, light and dark | | | |

### Screen 2

| Part | Date | Result | Notes |
|---|---|---|---|
| `cargo xtask ci` | 2026-10-23 | Pass | Rust run 39842444315 on `natamux-ci/t-0148` (a8cac9a), job `check` |
| Ported rule tests (same names as the TypeScript tests) | 2026-10-23 | Pass | In `check`; the cases left out are listed above |
| Client tests and live feed (five routes, `WorkspaceChanged`, `ShellsChanged`) | 2026-10-23 | Pass | In `check` |
| `live-daemon` screen-2 flow | 2026-10-23 | Pass | Run 39842444315, `runs_the_reads_and_writes_of_screen_2` |
| `app-macos`: the 8 screen-2 states, both themes, three sizes; shots match baselines | | Open | Shot in run 39842444315 (118 shots); no baselines until the owner signs screen 2 off |
| `app-macos`: `--smoke`, zip at most 60 MiB | 2026-10-23 | Pass | Run 39842444315, job `app-macos` |
| `app-windows`: build and `--smoke` | 2026-10-23 | Pass | Rust run 39846390721 on `dev` (b0d8c73, PR #431 merged), job `app-windows` |
| Frames with 10 agents printing and 4 panes on screen (`--gate2` `panes` report, mean of 4 runs) | | | |
| Key to glyph in a pane of the grid (p95 at most 20 ms, app part at most 17 ms) | | | |
| Vietnamese Telex and VNI in a pane and in the find field | | | |
| Same API call as the React screen for each action, web dashboard shows the result | | | |
| Owner sign-off, light and dark | | | |

### Screen 3

| Part | Date | Result | Notes |
|---|---|---|---|
| `cargo xtask ci` | 2026-10-23 | Pass | Rust run 39869173632 on `natamux-ci/t-0149-check` (a25a785), job `check` |
| Ported rule tests (same names as the TypeScript tests) | 2026-10-23 | Pass | In `check`; the cases left out are listed above |
| Client tests and live feed (eight routes, `TaskLogged`) | 2026-10-23 | Pass | In `check` |
| `live-daemon` screen-3 flow | 2026-10-23 | Pass | Run 39863431903, `runs_the_reads_and_writes_of_screen_3` |
| `app-macos`: the 11 screen-3 states, both themes, three sizes; shots match baselines | | Open | Shot in run 39869173632 (184 shots; run 39863431903 had 206 with the 1280x720 ones, since dropped); no baselines until the owner signs screen 3 off |
| `app-macos`: `--smoke`, zip at most 60 MiB | 2026-10-23 | Pass | Run 39863431903, job `app-macos` |
| `app-windows`: build and `--smoke` | 2026-10-23 | Pass | Rust run 39879426799 on `dev` (b2545a6, PR #432 merged), job `app-windows`; its `app-macos` passed on a rerun (the first try lost the network at the attest step) |
| The docked panel at 1440, both themes | | | Owner, on the CI build |
| Vietnamese Telex and VNI in the edit form, the Send back reason and the subtask field | | | Owner |
| Same API call as the React screen for each action, web dashboard shows the result | | | Owner |
| The notification window | | | Owner |
| Owner sign-off, light and dark | | | |

### Screen 4

| Part | Date | Result | Notes |
|---|---|---|---|
| `cargo xtask ci` | 2026-10-23 | Pass | Rust run 39899125106 on `natamux-ci/t-0150` (e625d71), job `check` |
| Ported rule tests (same names as the TypeScript tests) | 2026-10-23 | Pass | In `check`; the cases left out are listed above |
| Client tests (seven routes, `fresh`, the 404 and 413 errors) | 2026-10-23 | Pass | In `check` (`tests/git_api.rs`) |
| `live-daemon` screen-4 flow | 2026-10-23 | Pass | Run 39899125106, `runs_the_reads_and_writes_of_screen_4` |
| `supply-chain` with the new grammar crates | 2026-10-23 | Pass | Run 39899125106; the lockfile and the `cargo vet` exemptions came from a CI run (39889128968), not from a local cargo |
| `app-macos`: the 7 screen-4 states, both themes, three sizes; shots match baselines | | Open | Shot in run 39899125106 (226 shots); no baselines until the owner signs screen 4 off |
| `app-macos`: `--smoke`, zip at most 60 MiB | 2026-10-23 | Pass | Run 39899125106; the zip is about 50 MB with the grammars |
| `app-windows`: build and `--smoke` | 2026-10-23 | Pass | Rust run 39910369940 on `dev` (3ed1970, PR #433 merged), job `app-windows`; all its jobs passed |
| The diff of a real task in both themes and both layouts | | | Owner, on the CI build |
| Vietnamese Telex and VNI in the commit box and the comment box | | | Owner |
| Same API call as the React screen for each action, web dashboard shows the result | | | Owner |
| Owner sign-off, light and dark | | | |

### Screen 5

| Part | Date | Result | Notes |
|---|---|---|---|
| `cargo xtask ci` | 2026-10-23 | Pass | Rust run 39927345495 on `natamux-ci/t-0151` (2f57470), job `check` |
| Ported rule tests (same names as the TypeScript tests) | 2026-10-23 | Pass | In `check`; the cases left out are listed above |
| Client tests (`save_settings` with and without `If-Match`, 409, 400) | 2026-10-23 | Pass | In `check` (`tests/settings_api.rs`) |
| `live-daemon` screen-5 flow | 2026-10-23 | Pass | Run 39927345495, `runs_the_reads_and_writes_of_screen_5` |
| `app-macos`: the 7 screen-5 states, both themes, three sizes; shots match baselines | | Open | Shot in run 39927345495 (268 shots); no baselines until the owner signs screen 5 off |
| `app-macos`: `--smoke`, zip at most 60 MiB | 2026-10-23 | Pass | Run 39927345495 |
| `app-windows`: build and `--smoke` | 2026-10-23 | Pass | Rust run 39941043330 on PR #434 (e49fb11), job `app-windows` |
| Progress of real tasks, a pipeline edit saved and seen in the web dashboard, and a web edit seen in the app | | | Owner, on the CI build |
| Vietnamese Telex and VNI in the name, prompt and condition fields | | | Owner |
| Owner sign-off, light and dark | | | |

### Screen 6

| Part | Date | Result | Notes |
|---|---|---|---|
| `cargo xtask ci` | 2026-10-23 | Pass | Rust run 39954098408 on `natamux-ci/rust` (d60ca6a), job `check` |
| Ported rule tests (same names as the TypeScript tests) | 2026-10-23 | Pass | In `check`; the cases left out are listed above |
| Client tests (eight routes, `If-Match`, 409, 413, 404, bytes) | 2026-10-23 | Pass | In `check` (`tests/files_api.rs`) |
| `live-daemon` screen-6 flow | 2026-10-23 | Pass | Run 39954098408, `runs_the_reads_and_writes_of_screen_6` |
| `app-macos`: the 8 screen-6 states, both themes, three sizes; shots match baselines | | Open | Shot in run 39954098408 (316 shots); no baselines until the owner signs screen 6 off |
| `app-macos`: `--smoke`, zip at most 60 MiB | 2026-10-23 | Pass | Run 39954098408 |
| `app-windows`: build and `--smoke` | 2026-10-23 | Pass | Rust run 39963088535 on PR #435 (1ff379a), job `app-windows`; every job of that run passed. The `dev` run of the merge (39963121785, d5685cd) was cancelled |
| A file edited and saved in the app, seen in the web dashboard and on disk; an agent's write shows the banner | | | Owner, on the CI build |
| A path dropped on a real agent's terminal | | | Owner |
| The Docs of a real task, Rendered, both themes | | | Owner |
| Vietnamese Telex and VNI in the editor and the Docs edit | | | Owner |
| Owner sign-off, light and dark | | | |

### Screen 7

| Part | Date | Result | Notes |
|---|---|---|---|
| `cargo xtask ci` | 2026-10-23 | Pass | Rust run 39979437375 on `natamux-ci/rust` (f84319a), job `check`; again run 39983890846 on `natamux-ci/rust-check` (df01047, the shortcut sheet's columns kept inside the dialog after the shots) |
| Ported rule tests (same names as the TypeScript tests) | 2026-10-23 | Pass | In `check`; the cases left out are listed above |
| Client tests (`POST /repos`, the GitHub issues and import, the token order, the private token file) | 2026-10-23 | Pass | In `check` (`tests/projects_api.rs`, `config.rs`) |
| `live-daemon` screen-7 flow | 2026-10-23 | Pass | Run 39979437375, `runs_the_reads_and_writes_of_screen_7` |
| `supply-chain` | 2026-10-23 | Fail | Run 39979437375: `cargo deny` reports RUSTSEC-2026-0386 (tokio 1.52.1, `AsyncFd` I/O safety), an advisory published after the screen-6 runs. It is not from this change: `Cargo.lock` is the one of `dev`. Bumping tokio or allowing the advisory is the owner's call (dependency rules, cooldown) |
| `app-macos`: the 16 screen-7 states, both themes, three sizes; shots match baselines | | Open | Shot in run 39979437375 (412 shots); no baselines until the owner signs screen 7 off |
| `app-macos`: `--smoke`, zip at most 60 MiB | 2026-10-23 | Pass | Run 39979437375. With no token on the runner the app starts on Sign in, so `--smoke` types into its token field |
| `app-windows`: build and `--smoke` | | Open | Runs on the PR into `dev` |
| The palette and the jump palette on real tasks | | | Owner, on the CI build |
| A project added and an issue import tried in the app, seen in the web dashboard | | | Owner |
| An Issues-tab change saved and seen in the web | | | Owner |
| Sign in with a wrong and the right token | | | Owner |
| Theme, UI size and terminal font changes | | | Owner |
| Vietnamese Telex and VNI in the palette, the font dialog, the Projects form, the JSON tab and Sign in | | | Owner |
| Owner sign-off, light and dark | | | |
