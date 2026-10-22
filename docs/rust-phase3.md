# Rust phase 3: the desktop app

Phase 3 of `plan-rust-migration.md`: the native app (`crates/natamux-app`) gets the screens of the React dashboard one by one, on today's Node daemon. This page covers the shared base and screen 1 (T-0146): the app shell, the sidebar, the task table and the task board; and screen 2 (T-0148): the workspace with its terminals, and the Agents grid. It says how to get the beta, what CI checks, where the app differs from the React screens, and the gate results.

Screens 3 to 7 come in later tasks, each with its own spec and its own "Gate results" rows.

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
| `natamux-app` | L4 | The Natamux theme, fonts, icons, the command registry, shared parts (dialog, menus, toasts, skeleton), the view state, the shell and screen 1 |

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
| `live-daemon` | `natamux-client` against the daemon image of main, including the screen-1 flow (create a task, rename it, add a subtask, start a workspace, see `workspace.preparing` end, remove the workspace, delete the task) and the screen-2 flow (create a task and its workspace, open Terminal 2 by socket, close it, read the ended terminals, push, remove the workspace, delete the task) |
| `supply-chain` | `cargo deny`, `cargo audit`, `cargo vet` |

The tests in `cargo xtask ci` that guard the look and the keys:

- **Contrast.** Both Natamux themes: every text pair at least 4.5:1, every glyph, focus ring and border pair at least 3:1 (`ui-spec.md` section 6). A test with a bad pair proves it fails.
- **Colour literals.** `cargo xtask colours` fails on `rgb(`, `rgba(`, `hsla(`, a hex integer such as `0x1b2a4aff` and a quoted `"#1B2A4A"` in `crates/natamux-app/src`, except in `components/colors.rs` and the hex data of the contrast test (`components/theme_contrast.rs`), by exact path. Views read the theme.
- **Keys.** Every key and menu item comes from the command registry. Two commands with the same key in the same context fail a test.

## Screenshot baselines

`app-macos` shoots each state in both themes (`light`, `dark`) at 1000x680 and 800x560: 24 states, 96 shots. Both sizes are under 1024, so the sidebar is a drawer there. The runner's screen is 1024x768, so `tasks-table` and the screen-2 states are shot at 1024x720 too, where the sidebar docks: 18 more shots. macOS keeps the window below the menu bar, so these come out 1024x684; the width, which docks the sidebar, holds. If a runner gives the window less than 1024 wide, the comparison says the sizes differ. A shot is the app's window itself (`screencapture -l` with the window id, found by the app's process id), so the menu bar is not in it and the status bar is. Baselines are the PNGs in `crates/natamux-app/tests/shots/`, named `<state>-<theme>-<size>.png`.

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

## Where it differs from the React screens

- **Hidden controls.** Controls that open a screen that is not built yet stay hidden until that screen ships: the command center button, the details toggle, View changes, project Settings, Add a project, the Progress layout, the status bar's Projects, Import issues, Notifications, Keyboard and Account items, and **Show details** in the row menu. The needs-you pill shows and filters the list to the states that need you, but it opens no jump palette.
- **View state in a file.** The React app keeps `natamux.*` in the browser's localStorage. The app keeps one `view.json` in its data folder (`~/Library/Application Support/Natamux Native/view.json`, `%APPDATA%` on Windows), written atomically: sidebar folds (at most 300), blurred projects, pinned tasks, layout, sidebar open and width, show finished, seen states (at most 1000) and the last base branch per project. A broken file is ignored and replaced.
- **Pinned and folds are per app.** They are not shared with the browser or the Electron app, which keep their own.
- **Opening a task.** Enter or a double click opens the task's workspace (screen 2, below). The terminal keeps Esc, so an agent can be stopped with it (owner, 2026-10-22). Cmd+[ on macOS, **View → All tasks** or the sidebar go back.
- **Looks.** The widgets are Zed's `ui` kit with the Natamux theme, so spacing, focus rings and menus follow Zed, not the CSS of the dashboard.
- **No phone layout, no system notifications, no screen-reader live region.** The first stays in the web app. The others come with the details panel.
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
  - Start a task: the branch and skill pickers are plain menus with no search; pasting a screenshot is not supported. The files-not-committed dialog offers Close only, with no View changes or Commit… (screen 2).
  - Agent icons (T-0148): the web's icons, `claude`, `gemini-ai`, `copilot`, `cursor` from material-icon-theme 5.39.0 (MIT) and the app's own `codex` (shape from lobe-icons, MIT), in `assets/icons/agents/` with both licences. In the light theme a `_light` icon is used when there is one. An unknown agent shows the generic bot icon.
  - The skeleton does not pulse, the toast timer pauses on hover only, the resize handle collapses during the drag, and context menus have no type-ahead.

## Screen 2: where it differs from the React screens

- **Tabs.** The workspace has only the Terminal tab. Changes and History come with screen 4, Docs and the editor tab with screen 6, so the keys 1 to 4 are not bound yet.
- **Hidden until their screen ships.** Commit… (screen 4): where React shows it in place of Push, Push says why it waits ("Commit your N changed files first"); the finished banner still says the files are not committed and Remove workspace waits. View changes of the wrap-up (screen 4). The state chip's "Show in progress" and the details toggle (screen 3). Show progress of the finished summary (screen 3). "Add to Agents" lives in the palette (screen 7).
- **No details panel yet.** The reference shots show the details panel at the right; the app's workspace fills that space until screen 3.
- **Pickers.** "Branch from" and "Add a pane" are plain menus with no search, as the pickers of screen 1.
- **Times.** "ended 7:05" of the wrap-up and "Finished" of the summary are in UTC and say so: the app has no local time zone helper yet.
- **Spinners.** The connection strip, the restore cover and the preparing step show a still loader icon, as the jobs menu does: an endless animation would draw the window every frame. As in React, "Connecting…" and "Reconnecting…" show only after 200 ms, so a quick connect shows no strip; a terminal that could not start says so at once.
- **Close a terminal.** No confirm, as in the React screen (`ShellPanes.tsx`), although `ui-spec.md` 4.23 asks for one when a program other than the shell runs. The wrap-up's Stop asks first, as React does.
- **Grid.** The panes sit in flex rows; the last pane of the last row takes the rest of it, as `paneSpan` does in the CSS grid.
- **Hidden terminals.** As `terminal-cache.ts`: at most 8 terminals stay connected off screen, each at most 20 minutes; the terminals of a task that is gone, finished or has no workspace go once hidden. The 20-minute limit is checked when the app draws, not by a timer.
- **No phone layout**: no pill switcher of terminals and no "Git actions" menu.
- **`--gate2`** shows the panes grid with four bench shells printing and saves the `panes` report (screen-1 section, `--gate2`). The bench shells are plain terminals, so their panes show the terminal mark, not an agent's.

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
| `app-windows`: build and `--smoke` | | Open | Skipped on snapshot pushes; runs on the PR into `dev` |
| Frames with 10 agents printing and 4 panes on screen (`--gate2` `panes` report, mean of 4 runs) | | | |
| Key to glyph in a pane of the grid (p95 at most 20 ms, app part at most 17 ms) | | | |
| Vietnamese Telex and VNI in a pane and in the find field | | | |
| Same API call as the React screen for each action, web dashboard shows the result | | | |
| Owner sign-off, light and dark | | | |
