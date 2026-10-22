# Rust phase 3: the desktop app

Phase 3 of `plan-rust-migration.md`: the native app (`crates/natamux-app`) gets the screens of the React dashboard one by one, on today's Node daemon. This page covers the shared base and screen 1 (T-0146): the app shell, the sidebar, the task table and the task board. It says how to get the beta, what CI checks, where the app differs from the React screens, and the gate results.

Screens 2 to 7 come in later tasks, each with its own spec and its own "Gate results" rows.

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
| `live-daemon` | `natamux-client` against the daemon image of main, including the screen-1 flow: create a task, rename it, add a subtask, start a workspace, see `workspace.preparing` end, remove the workspace, delete the task |
| `supply-chain` | `cargo deny`, `cargo audit`, `cargo vet` |

The tests in `cargo xtask ci` that guard the look and the keys:

- **Contrast.** Both Natamux themes: every text pair at least 4.5:1, every glyph, focus ring and border pair at least 3:1 (`ui-spec.md` section 6). A test with a bad pair proves it fails.
- **Colour literals.** `cargo xtask colours` fails on `rgb(`, `rgba(`, `hsla(`, a hex integer such as `0x1b2a4aff` and a quoted `"#1B2A4A"` in `crates/natamux-app/src`, except in `components/colors.rs` and the hex data of the contrast test (`components/theme_contrast.rs`), by exact path. Views read the theme.
- **Keys.** Every key and menu item comes from the command registry. Two commands with the same key in the same context fail a test.

## Screenshot baselines

`app-macos` shoots each state in both themes (`light`, `dark`) at 1000x680 and 800x560: 16 states, 64 shots. Both sizes are under 1024, so the sidebar is a drawer and shows only in the `drawer` state. A shot is the app's window itself (`screencapture -l` with the window id, found by the app's process id), so the menu bar is not in it and the status bar is. Baselines are the PNGs in `crates/natamux-app/tests/shots/`, named `<state>-<theme>-<size>.png`.

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

## Where it differs from the React screens

- **Hidden controls.** Controls that open a screen that is not built yet stay hidden until that screen ships: the command center button, the details toggle, the Agents nav row, View changes, project Settings, Add a project, the Progress layout, the status bar's Projects, Import issues, Notifications, Keyboard and Account items, and **Show details** in the row menu. The needs-you pill shows and filters the list to the states that need you, but it opens no jump palette.
- **View state in a file.** The React app keeps `natamux.*` in the browser's localStorage. The app keeps one `view.json` in its data folder (`~/Library/Application Support/Natamux Native/view.json`, `%APPDATA%` on Windows), written atomically: sidebar folds (at most 300), blurred projects, pinned tasks, layout, sidebar open and width, show finished, seen states (at most 1000) and the last base branch per project. A broken file is ignored and replaced.
- **Pinned and folds are per app.** They are not shared with the browser or the Electron app, which keep their own.
- **Opening a task.** Enter or a double click opens the phase 1 terminal pane on the task's first agent terminal. The terminal keeps Esc, so an agent can be stopped with it (owner, 2026-10-22). Cmd+[ on macOS, **View → All tasks** or the sidebar go back. A task with no live terminal says: "Open this task in the dashboard for now. The native workspace view comes with screen 2."
- **Looks.** The widgets are Zed's `ui` kit with the Natamux theme, so spacing, focus rings and menus follow Zed, not the CSS of the dashboard.
- **No phone layout, no system notifications, no screen-reader live region.** The first stays in the web app. The others come with the details panel.
- **Tools menu.** The perf overlay, the bench and the IME check are commands of the **Tools** menu.
- **`--gate2`.** It scrolls the sidebar tree and a bench terminal while the bench runs, then shows the task table in main while the bench's 10 shells keep printing out of sight, and scrolls the table for 20 s. Then it closes the bench and types into a quiet shell. It saves three perf reports: bench, table and typing. Every report has `start.first_frame_ms` and `start.first_data_ms`, the times from the start of the app to the first frame and to the first task list.
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
  - No agent icons yet (known gap, for the owner's sign-off): sidebar rows show a generic bot icon with the agent's name as tooltip, and board cards and table cells show the agent's name as text. `agents/agent_choices.rs` has `agent_icon()` for when the agent SVGs and their licences are added.
  - The skeleton does not pulse, the toast timer pauses on hover only, the resize handle collapses during the drag, and context menus have no type-ahead.

## Ported tests

Each TypeScript case of spec Part D item 11 has a Rust test of the same name, except:

- `it.each` cases whose name has `%s` placeholders take a plain Rust name: "at %s is %s" (`background-jobs.test.ts`) is `formats_elapsed_time`, and "shows %s as %s (short) and %s (long)" (`tasks.test.ts`) is `shows_each_time_as_short_and_long`.
- `formatAge` "is short by default": Rust has no default arguments, so every caller of `format_age` passes its `AgeStyle`. The short case is in `shows_each_time_as_short_and_long`.
- `actionsFor` "only offers events the daemon accepts for that state": it runs core's `applyTaskEvent`, which has no Rust port. The event names are copied from the TypeScript table.
- "the last used branch is kept for each repository" (`lastBase`, `rememberBase`): it tests localStorage. `app/view_state.rs` has its own tests for the last base per project.
- React, DOM and store tests (`WorkspaceList.test.tsx`, `sidebar-static.test.ts`, the `useViewStore` route and persistence cases): the pure rules they cover have new tests in `sidebar/tree_model.rs` and `sidebar/sidebar_text.rs`.
- Cases that check object identity or one-at-a-time async deletes check equality and order instead: Rust has no object identity, and the delete loops are sequential.

## Gate results

Gate per screen (plan): target numbers met, baselines in both themes, owner sign-off, every state covered with the same behaviour and API calls as the React screen.

| Part | Date | Result | Notes |
|---|---|---|---|
| `cargo xtask ci` | | | |
| Ported rule tests (same names as the TypeScript tests) | | | |
| Client tests and live feed | | | |
| `live-daemon` screen-1 flow | | | |
| `app-macos`: shots match baselines | | | |
| `app-macos`: `--smoke`, zip at most 60 MiB | | | |
| `app-windows`: build and `--smoke` | | | |
| Frames while scrolling the sidebar tree and the table with 10 agents (`--gate2`, mean of 4 runs) | | | |
| Cold start to the first frame (limit 300 ms) | | | |
| Cold start to the list with data (limit 1 s) | | | |
| Vietnamese Telex and VNI in every screen-1 text box | | | |
| Same API call as the React screen for each action, web dashboard shows the result | | | |
| `native-beta.yml` makes a pre-release, its zip passes `gh attestation verify`, `/releases/latest` still points at the newest Electron release | | | |
| README and site install page, PR into `main`: `scripts/readme.test.py` and the site build | | | |
| Owner sign-off, light and dark | | | |
