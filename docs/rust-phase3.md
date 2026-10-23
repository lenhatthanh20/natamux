# Rust phase 3: the desktop app

Phase 3 of `plan-rust-migration.md`: the native app (`crates/natamux-app`) gets the screens of the React dashboard one by one, on today's Node daemon. This page covers the shared base and screen 1 (T-0146): the app shell, the sidebar, the task table and the task board; screen 2 (T-0148): the workspace with its terminals, and the Agents grid; screen 3 (T-0149): the details panel with the stage timeline and the gate actions; and screen 4 (T-0150): the Changes and History tabs of the workspace. It says how to get the beta, what CI checks, where the app differs from the React screens, and the gate results.

Screens 5 to 7 come in later tasks, each with its own spec and its own "Gate results" rows.

## Decisions (owner, 2026-10-22)

- This task builds the foundation and screen 1 only.
- The beta is a GitHub pre-release made from `dev`, with the attested CI build. It is unsigned until the Apple account exists. `scripts/release.sh` and the releases of `main` do not change.
- Inter, JetBrains Mono and the Lucide SVGs that screen 1 uses go in `crates/natamux-app/assets`, with their license files. No new crate.
- Earlier rules hold: no new crates without asking, `cargo` runs in CI only, Rust PRs target `dev`.
- Baselines are shot in CI and kept in the repo after the owner signs a screen off. Nothing is shot on the owner's Mac.

## Crates

| Crate | Layer | Job in phase 3 |
|---|---|---|
| `natamux-proto` | L0 | The types screen 1 reads and sends: repos with `worktreesDir`, templates, jobs, activity, diff stats, usage, the terminals list, task events, branches, changes, the graph, the new workspace input. `tests/core_values.rs` checks names and limits against `packages/core` |
| `natamux-client` | L3 | One function per route, in files that mirror the daemon's route files. The live feed reads the whole dashboard (tasks, repos, jobs, activity, usage, diff stats, terminals, the preparing step) |
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
| `check` | `cargo xtask ci`: fmt, clippy, all tests, the layers, and the colour-literal check (below). `scripts/compare-shots.test.py` |
| `app-macos` | Release build, `--smoke`, the GPUI-drew-it check, the screen-1 shots and their comparison, the size of the zipped app (at most 60 MiB), then attest and upload |
| `app-windows` | Release build and `--smoke` still pass. Runs on PRs into `dev` and on `dev`; snapshot pushes to `natamux-ci/**` skip it, as it is the slowest job |
| `live-daemon` | `natamux-client` against the daemon image of main, including the screen-1 flow (create a task, rename it, add a subtask, start a workspace, see `workspace.preparing` end, remove the workspace, delete the task) the screen-2 flow (create a task and its workspace, open Terminal 2 by socket, close it, read the ended terminals, push, remove the workspace, delete the task) the screen-3 flow (read the settings and the stage files, edit the notes and priority, set and clear a skip mark, skip a stage, send the task back, save a template, run one more try, then delete the try and the task) and the screen-4 flow (`tests/live_daemon_git.rs`: a new workspace has no changes and a graph with commits; commit, the commit message and undo answer 409, a file or an image that did not change 404, a message to the agent is taken (or refused with 409 before the agent runs); the daemon keeps its worktrees in its own volume, so the test cannot change a file there). Each flow cleans up after a failed step too |
| `supply-chain` | `cargo deny`, `cargo audit`, `cargo vet` |

The tests in `cargo xtask ci` that guard the look and the keys:

- **Contrast.** Both Natamux themes: every text pair at least 4.5:1, every glyph, focus ring and border pair at least 3:1 (`ui-spec.md` section 6). A test with a bad pair proves it fails.
- **Colour literals.** `cargo xtask colours` fails on `rgb(`, `rgba(`, `hsla(`, a hex integer such as `0x1b2a4aff` and a quoted `"#1B2A4A"` in `crates/natamux-app/src`, except in `components/colors.rs` and the hex data of the contrast test (`components/theme_contrast.rs`), by exact path. Views read the theme.
- **Keys.** Every key and menu item comes from the command registry. Two commands with the same key in the same context fail a test.

## Screenshot baselines

`app-macos` shoots each state in both themes (`light`, `dark`) at 1000x680 and 800x560: 42 states, 168 shots, and the 4 shots of the window check, 172 in all. Both sizes are under 1024, so the sidebar is a drawer there, and the details panel a sheet over main with a scrim. The runner's screen is 1024x768, so `tasks-table` and the states of screens 2 to 4 are shot at 1024x720 too, where the sidebar docks and the details panel is a sheet without a scrim: 54 more shots, 226 in all. The details panel docks only from 1280, so no shot shows it docked: run 39863431903 asked for 1280x720 and every one came out 1024x684 with the sheet, so that size was dropped (see "Screen 3: where it differs"). macOS keeps the window below the menu bar, so these come out 1024x684; the width, which docks the sidebar, holds. If a runner gives the window less than 1024 wide, the comparison says the sizes differ. A shot is the app's window itself (`screencapture -l` with the window id, found by the app's process id), so the menu bar is not in it and the status bar is. Baselines are the PNGs in `crates/natamux-app/tests/shots/`, named `<state>-<theme>-<size>.png`.

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

## Where it differs from the React screens

- **Hidden controls.** Controls that open a screen that is not built yet stay hidden until that screen ships: the command center button, project Settings, Add a project, the Progress layout, and the status bar's Projects, Import issues, Keyboard and Account items. The details toggle, **Show details** in the table's row menu and the status bar's notification item came back with screen 3; View changes, Commit…, and the Changes and History tabs with screen 4. The needs-you pill shows and filters the list to the states that need you, but it opens no jump palette.
- **View state in a file.** The React app keeps `natamux.*` in the browser's localStorage. The app keeps one `view.json` in its data folder (`~/Library/Application Support/Natamux Native/view.json`, `%APPDATA%` on Windows), written atomically: sidebar folds (at most 300), blurred projects, pinned tasks, layout, sidebar open and width, show finished, seen states (at most 1000) and the last base branch per project. A broken file is ignored and replaced.
- **Pinned and folds are per app.** They are not shared with the browser or the Electron app, which keep their own.
- **Opening a task.** Enter or a double click opens the task's workspace (screen 2, below). The terminal keeps Esc, so an agent can be stopped with it (owner, 2026-10-22). Cmd+[ on macOS, **View → All tasks** or the sidebar go back.
- **Looks.** The widgets are Zed's `ui` kit with the Natamux theme, so spacing, focus rings and menus follow Zed, not the CSS of the dashboard.
- **No phone layout.** It stays in the web app. The needs-you notifications and the screen-reader announcement came with screen 3 (below).
- **Tools menu.** The perf overlay, the bench and the IME check are commands of the **Tools** menu.
- **`--gate2`.** It scrolls the sidebar tree and a bench terminal while the bench runs, then shows the task table in main while the bench's 10 shells keep printing out of sight, and scrolls the table for 20 s. Then it opens the task's workspace with four of the bench's shells as panes, still printing, and scrolls the first pane for 20 s (screen 2). Then it closes the bench and types into a quiet shell. It saves four perf reports: bench, table, panes and typing. Every report has `start.first_frame_ms` and `start.first_data_ms`, the times from the start of the app to the first frame and to the first task list.
- **Smaller differences.**
  - Ages of a week or more show the date in UTC: the app has no local time zone helper yet.
  - The needs-you pill and the status bar's "N need you" filter the list to every task that needs you (needs input, review, blocked, failed, and running but quiet), in place of the web's jump palette. A "Needs you" chip in the view header shows it and turns it off.
  - The status bar has a fourth link word, "Not connected", where the web signs out. It does not measure overflow; labels drop by width only. The jobs menu does not update while open, and its spinner is a still icon.
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

- **Tabs.** Terminal, Changes and History (keys 1 to 3) since screen 4. Docs and the editor tab come with screen 6, so the key 4 is not bound yet.
- **Hidden until their screen ships.** "Add to Agents" lives in the palette (screen 7). Commit… and View changes came back with screen 4.
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
- **Tabs.** Progress and Details only. The Files tab comes with screen 6; a remembered Files tab shows Progress.
- **Hidden until their screen ships.** The output file chip of a stage card shows the file and its size but opens nothing: the Docs tab comes with screen 6.
- **Notes** are plain text with their line breaks; Markdown is not drawn yet.
- **Markers do not move.** The marker of a running stage and the line into it are still, as the screen-2 spinners: an endless animation would draw the window every frame.
- **Stage commands.** The web runs Skip the stage, Mark the stage as done, Approve the stage anyway, Send back and Show the task progress from its palette (screen 7). Until then they are in the menu bar's **Task** menu, with no key.
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
- The details cases of `view-store.test.ts` have twins of the same name in `app/view_state_tests.rs` ('opens the details of a task on its progress' tests `ViewState::show_progress`), except: 'opens the details of a task on its files, and remembers that view' (the Files tab comes with screen 6), and the cases about the browser's storage ('works without a storage', '... when the storage is blocked'): the app keeps `view.json`, and a missing or broken file is the default view (`a_missing_file_is_the_default_view`, `a_broken_file_is_ignored_and_replaced_on_the_next_save`).
- `position.test.ts` and `capabilities.test.ts`: only the cases of the rules the panel uses (`describeCondition`, the paths not taken, `stageName`, `offersWorkflow`, `canRunWorkflow`, `offersCi`).

Screen 4 (T-0150): every case of `features/git/__tests__/change-tree.test.ts`, `graph-layout.test.ts`, `lib/__tests__/changes.test.ts`, the `commentForAgent` cases of `AgentComment.test.tsx` and `formatBytes` of `ImageDiff.test.tsx` has a Rust test of the same name (`git/change_tree_tests.rs`, `git/graph_layout_tests.rs`, `git/change_rules_tests.rs`, `git/comment_rules_tests.rs`, `git/image_rules_tests.rs`), and so have the screen-4 cases of `commands.test.ts` and `view-store.test.ts`, except:

- `RollbackDialog.test.tsx`: its cases are tests of `git/rollback_rules.rs` by the same names; "starts on Cancel" is covered by the shot (the dialog opens with Cancel focused).
- `AgentComment.test.tsx` component cases ('names the lines of the quote, and drops it on remove', 'says when the quote is too long to send', 'keeps the quote when the send fails, and clears it when it works'): the label and chip rules have tests; the rest is covered by the shots and the owner's check.
- `ImageDiff.test.tsx` object-URL and query cases: the app reads the bytes and makes a GPUI image, with no URL.
- `view-store.test.ts` 'keeps a remembered Docs tab after a reload': the app keeps only the Terminal, Changes and History tabs until the Docs tab ships (screen 6); a stored `docs` reads as no tab.
- `view-store.test.ts` 'closes back to the tab the user picked last, or the terminal' and the other cases of the open file: the editor tab comes with screen 6.
- `cutCommitSubject` and `simpleCommitDraft` (`core/git/commit-message.ts`): the daemon drafts the message; they come with the daemon port (phase 4).
- `ChangesPanel`, `DiffToolbar`, `MonacoDiff`, `GitGraph` and `FileBody` have no TypeScript tests. Their pure parts have new tests: `body_kind` and the notices, the commit box rules (`git/commit_rules.rs`), the width rules and the hunk shapes (`git/diff_rules.rs`), the lane geometry (`git/graph_lanes.rs`), `image_size`.

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
| `app-windows`: build and `--smoke` | | Open | Runs on the PR into `dev` |
| The diff of a real task in both themes and both layouts | | | Owner, on the CI build |
| Vietnamese Telex and VNI in the commit box and the comment box | | | Owner |
| Same API call as the React screen for each action, web dashboard shows the result | | | Owner |
| Owner sign-off, light and dark | | | |
