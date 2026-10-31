# Rust phase 3: the desktop app

Phase 3 of `plan-rust-migration.md`: the native app (`crates/natamux-app`) gets the screens of the React dashboard one by one, on today's Node daemon. This page covers the shared base and screen 1 (T-0146): the app shell, the sidebar, the task table and the task board; screen 2 (T-0148): the workspace with its terminals, and the Agents grid; screen 3 (T-0149): the details panel with the stage timeline and the gate actions; screen 4 (T-0150): the Changes and History tabs of the workspace; screen 5 (T-0151): the Progress layout of All tasks and the pipeline editor of Project settings; screen 6 (T-0152): the Files tab of the details panel, the editor tab and the Docs tab of the workspace; screen 7 (T-0154), the last of phase 3: the command palette and the jump palette, the shortcut sheet, the account menu with the terminal font dialog, the Settings dialog, the Projects dialog, the Issues and JSON tabs of Project settings, Import issues, and Sign in; and the close-out (T-0156): the Zed look, the baselines and the owner's sign-off. It says how to get the beta, what CI checks, where the app differs from the React screens, and the gate results.

## Decisions (owner, 2026-10-22)

- This task builds the foundation and screen 1 only.
- The beta is a GitHub pre-release made from `dev`, with the attested CI build. It is unsigned until the Apple account exists. `scripts/release.sh` and the releases of `main` do not change.
- Inter, JetBrains Mono and the Lucide SVGs that screen 1 uses go in `crates/natamux-app/assets`, with their license files. No new crate. Since T-0156 (owner, 2026-10-24) the app uses Zed's fonts (`.ZedSans` = IBM Plex Sans, `.ZedMono` = Lilex, from Zed's `assets` crate) and Zed's icons; Inter, JetBrains Mono and the Lucide SVGs with a Zed twin were removed.
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
| `--open-task TASK` | Opens TASK once the dashboard lists it, as a click on its row does. A task it never lists prints `open-task: ...` on stderr and the app goes on. With `--fake-daemon`, no background job runs. `scripts/motion-cost.sh` uses it (see "Motion") |
| `--count-draws` | Prints `draws <n> active <true\|false>` once a second: the window's draws in that second and whether it has focus. It forces no frame (the perf overlay draws every frame). `scripts/motion-cost.sh` uses it (see "Motion") |

## What CI checks

| Job | Checks |
|---|---|
| `check` | `cargo xtask ci`: fmt, clippy, all tests, the layers, and the colour-literal check (below). `scripts/compare-shots.test.py`. A push to a branch whose name ends in `-check` runs only this job |
| `app-macos` | Release build, `--smoke`, `--smoke --fake-daemon` (the terminal grid and pipeline chart checks), the GPUI-drew-it check, the size of the zipped app (at most 60 MiB), then attest and upload. Runs when the app changed |
| `app-shots` | The state shots and their comparison, from the binary of `app-macos`. On demand only: one macOS job for a `Shots:` line, 4 for `[ci: shots]` or `[ci: all]`; none on `main` or without a tag. The job `shots` merges them into one artifact, `shots-macos` |
| `app-windows` | Release build and `--smoke` still pass. Runs on PRs into `dev` that change the app or the CLI, and on `dev`; snapshot pushes to `natamux-ci/**` skip it, as it is the slowest job, unless the commit message has `[ci: all]` |
| `live-daemon` | `natamux-client` against the daemon image of main, including the screen-1 flow (create a task, rename it, add a subtask, start a workspace, see `workspace.preparing` end, remove the workspace, delete the task) the screen-2 flow (create a task and its workspace, open Terminal 2 by socket, close it, read the ended terminals, push, remove the workspace, delete the task) the screen-3 flow (read the settings and the stage files, edit the notes and priority, set and clear a skip mark, skip a stage, send the task back, save a template, run one more try, then delete the try and the task) the screen-4 flow (`tests/live_daemon_git.rs`: a new workspace has no changes and a graph with commits; commit, the commit message and undo answer 409, a file or an image that did not change 404, a message to the agent is taken (or refused with 409 before the agent runs); the daemon keeps its worktrees in its own volume, so the test cannot change a file there), and the screen-6 flow (`tests/live_daemon_files.rs`: read the tree and a file, save it with its version, save with the old version (409), a path outside the worktree (409), write and read a stage file, list the attachments), and the screen-7 flow (`tests/live_daemon_projects.rs`: a wrong token answers 401 and the right one the task list, the projects, adding a relative path or a project that is there already (400), the GitHub issues and their import (409 with no signed-in `gh` in the image), an Issues-tab setting saved and put back). Each flow cleans up after a failed step too |
| `supply-chain` | `cargo deny`, `cargo audit`, `cargo vet` |

Rust tasks push their CI snapshots to two shared branches (owner, 2026-10-23): `natamux-ci/rust` runs the jobs its change needs (the shots when the app changed), `natamux-ci/rust-check` only `check`, for the compile loop. A task deletes only the temp branches it made.

Caches (T-0165): Actions keeps 10 GB of caches per repo. A run restores the caches of its own branch, of the base branch of its PR, and of `main`. A full set of caches is about 12 GB, so when `dev`, each PR and each snapshot branch saved its own, they pushed each other out: on 2026-10-24 all 9 GB were the daemon snapshots', and `dev` built `app-macos` in 34 minutes instead of 1.5. Now only a push to `main` saves caches, in the scope of `main` that every run reads: `check` (2.3 GB), `app-macos` (3.7 GB) and `app-windows` (3.0 GB), the jobs that are 12 to 34 minutes cold. Each keeps only its newest cache. The other jobs take about as long without a cache, so they have none. The `check` job warns when the caches pass 9.5 GB.

The tests in `cargo xtask ci` that guard the look and the keys:

- **Contrast.** Both Natamux themes: every text pair at least 4.5:1, every glyph, focus ring and border pair at least 3:1 (`ui-spec.md` section 6). A test with a bad pair proves it fails.
- **Colour literals.** `cargo xtask colours` fails on `rgb(`, `rgba(`, `hsla(`, a hex integer such as `0x1b2a4aff` and a quoted `"#1B2A4A"` in `crates/natamux-app/src`, except in `components/colors.rs` and the hex data of the contrast test (`components/theme_contrast.rs`), by exact path. Views read the theme.
- **Keys.** Every key and menu item comes from the command registry. Two commands with the same key in the same context fail a test.

## Screenshot baselines

`app-shots` shoots each state in both themes (`light`, `dark`) at 1000x680 and 800x560: 81 states, 324 shots, and the 4 shots of the window check, 328 in all. Both sizes are under 1024, so the sidebar is a drawer there. The runner's screen is 1024x768, so `tasks-table` and the states after screen 1 are shot at 1024x720 too, where the sidebar docks: 132 more shots, 460 in all. Since the lean layout (T-0158) the details panel docks at every shot size (see "Lean layout"). macOS keeps the window below the menu bar, so these come out 1024x684; the width, which docks the sidebar, holds. If a runner gives the window less than 1024 wide, the comparison says the sizes differ. A shot is the app's window itself (`screencapture -l` with the window id, found by the app's process id), so the menu bar is not in it and the status bar is. Baselines are the PNGs in `crates/natamux-app/tests/shots/`, named `<state>-<theme>-<size>.png`.

`scripts/compare-shots.py` compares a shot with its baseline. It fails when more than 0.5% of the pixels differ by more than 16 in a channel, or when the sizes differ, and it writes a diff image (differing pixels in red). The job summary lists each shot as matches, differs, or no baseline yet.

The owner signed all screens off on 2026-10-24 (T-0156, run 40002366175, the Zed look). The 408 state shots of run 40026431931 (the same look, polished) are the baselines. So that two runs give the same shot, a `--shot` hides the editors' scrollbars (Zed shows them for a moment after a scroll) and draws the window again every 200 ms for 4 s (Zed's terminal anchors its text to the bottom by the screen it read at its last draw, so a screen that came after it sat half a line higher in some runs). Since then the compare step fails on a shot that differs (`REPORT_ONLY` is empty in the `app-shots` job of `rust.yml`). A change of the look sets `REPORT_ONLY: "1"` until its new baselines, from a CI run, are signed off; the compare step is skipped meanwhile (every shot differs; the shots are in the artifact). The lean layout (T-0158) sets it now: its shots wait for the owner's sign-off.

Shots run on demand. A line `Shots: <state> <state> …` in the head commit message of a snapshot or a PR shoots only those states (each state at its sizes, both themes; about 1 minute instead of 22), in one `app-shots` job. `[ci: shots]` or `[ci: all]` shoots every state, in 4 jobs side by side: state number `i` goes to job `i % 4 + 1`, with all its sizes and themes (one after another the shots took 22 minutes). An unknown state fails the step.

### Screen-1 states

| State | Shows |
|---|---|
| `tasks-table` | All tasks as a table, one row selected |
| `tasks-board` | The same tasks as a board |
| `board-hover-card` | A board card under the pointer |
| `add-task-dialog` | The Add a task dialog with a typed title |
| `start-task` | The Start a task dialog |
| `remove-workspace` | The Remove workspace dialog: the terminals that end and the branch choice |
| `remove-workspace-anyway` | The Remove workspace dialog after Remove anyway: the files that are not committed are discarded |
| `clear-finished` | The Remove finished workspaces dialog: the site's cancelled task has files that are not committed, so it reads Remove anyway |
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
| `workspace-changes-comments` | The Changes tab with 3 review comments on 2 files (2 drafts, 1 sent, 1 outdated): blocks under their lines and the Comments list (T-0203; no reference shot) |
| `rollback` | The Roll back changes dialog over T-0143: both files checked, "1 new file will be deleted." |
| `image-diff` | T-0147 on Changes: the new `public/banner.png`, After only, on the checkerboard, with its size |
| `workspace-history` | T-0143 with the details panel on History (T-0183; was a main tab): the summary, the lanes, the ref chips with "+1" |
| `history-over-main` | T-0143's History over the terminals at full width, from the panel's expand button, which closes the panel (T-0231): author, hash and age columns, the Terminals button |

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
| `workspace-editor-tabs` | Eleven files open as tabs that wrap to more rows, `src/checkout.ts` active with a draft (its tab has the dot); `README.md` and `.github/README.md` share a name, so the second tab shows its folder `.github` |
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
| `account-menu` | The account menu under the title bar's Account button (above the status bar's Account item until T-0170) |
| `settings` | The Settings page (T-0174) on Appearance: theme, UI size, Task ids in the sidebar and the terminal font |
| `settings-general` | The Settings page on General: Notify me |
| `settings-search` | The Settings search "font": the font rows under Results |
| `projects` | The Projects dialog: the projects and the form with the daemon's worktree folder |
| `project-settings` | The Settings page of `natamux` as it opens: the three tabs, Stages |
| `project-settings-tab-issues` | The Issues tab: the four choices, Manual with Default |
| `project-settings-tab-json` | The JSON tab with the file |
| `project-settings-json-error` | The JSON tab with text that is not JSON: the alert, "1 problem", Save off |
| `import-issues` | Import issues with no `gh`: the error, Retry and Show details |
| `sign-in` | Sign in with an empty field: Connect off |
| `sign-in-error` | Sign in after the daemon refused the token: the message and the danger border |
| `add-menu` | The title bar's + menu: Add a task, Start a task (T-0158) |
| `details-empty` | All tasks with the details dock open and no task picked: "Select a task" (T-0158) |
| `rail-closed-panel` | T-0143's workspace with the details panel closed: the terminals and the rail at the right edge (T-0183) |
| `changes-over-main` | T-0143's Changes over the terminals with the details panel closed: main shows the whole Changes screen, files and commit box too (T-0183) |
| `stage-evidence` | T-0141's workspace, Progress: the finished Context card open with Show evidence open; a pass longer than the panel wraps inside the card (T-0190) |
| `start-task-branch-menu` | The Start a task dialog's Branch from menu with its Remote branches submenu open (T-0188) |
| `update-downloading` | The status bar's update item during a download: spinner, "Downloading 9.9.9 · 42 MB" (T-0214) |
| `update-ready` | The status bar's update item when an update is ready: "Restart to update" (T-0214) |
| `update-failed` | The status bar's update item after a manual check failed: "Update failed" (T-0214) |
| `settings-update-ready` | Settings on General with an update ready: the Version row's line and **Restart to Update** (T-0214) |
| `repo-trust-dialog` | The repo trust dialog of `natamux`: its commands with where each comes from, the setup script, the note, **Not now** and **Approve** (T-0224) |

## Where it differs from the React screens

- **Hidden controls.** None since screen 7. Controls that opened a screen not built yet were hidden until it shipped: the details toggle, **Show details** in the table's row menu and the status bar's notification item came back with screen 3; View changes, Commit…, and the Changes and History tabs with screen 4; the Progress layout and project Settings (the project menu and the card's gear) with screen 5; the Files tab, the Docs tab and key 4, **Show the open file**, **Show the files of the worktree** and the output chip of a stage card with screen 6; the command center button, Add a project, the status bar's Projects, Import issues, Keyboard and Account items, and the jump palette of the needs-you pill with screen 7. Two parts of the web stay out on purpose: Sign out (the Electron app has none either) and the Desktop app part of Settings (see "Screen 7: where it differs").
- **View state in a file.** The React app keeps `natamux.*` in the browser's localStorage. The app keeps one `view.json` in its data folder (`~/Library/Application Support/Natamux Native/view.json`, `%APPDATA%` on Windows), written atomically: sidebar folds (at most 300), blurred projects, pinned tasks, layout, sidebar open and width, show finished, seen states (at most 1000), the last base branch per project, and since screen 7 the theme, the UI size, the terminal font and the workspaces opened last (at most 20; the palette's Recent). A broken file is ignored and replaced. A token typed into Sign in is kept next to it in `daemon-access`.
- **Pinned and folds are per app.** They are not shared with the browser or the Electron app, which keep their own.
- **Opening a task.** Enter or a double click opens the task's workspace (screen 2, below). The terminal keeps Esc, so an agent can be stopped with it (owner, 2026-10-22). Cmd+[ on macOS, **View → All tasks** or the sidebar go back.
- **Looks.** Since the phase-3 close-out (T-0156) the app looks like Zed: the Natamux themes are built from Zed's One Dark and One Light (`assets/themes/natamux.json`), with a colour made lighter (dark) or darker (light) where it fails a contrast pair, and Natamux's own colour for a task state where One's is too faint; the icons are Zed's where Zed has one (`components/icons.rs` maps the web's Lucide names), and the Lucide SVGs with no Zed twin stay; dialogs have the spacing of Zed's `Modal` and its footer; the workspace and details tabs are Zed's `TabBar`; palette rows are Zed's `ListItem`; list rows hover and select in Zed's ghost colours with a 1 px focus ring; the title bar has Zed's height. Since T-0169 a state chip is a grey Zed chip with the state's colour only in its dot (the web keeps its coloured pills), and a finished stage card has its chevron on the line of its name. Most literal sizes are in rems, so the UI size scales them; the rows named under "UI size" (screen 7) stay in pixels. Spacing, focus rings and menus follow Zed, not the CSS of the dashboard. The compact, Zed-like layout came with T-0158 (see "Lean layout"). Still left: the Files tree on Zed's `TreeViewItem`, the board and Progress rows on `ListItem` sizes, and the Issues tab's choices as `ListItem` rows with a toggle state (today a radio drawn by the app).
  Since T-0246 the Progress view is a node flow: one node per stage on a faint dot grid, connectors between ports (solid done, solid in the state colour into the current node, dashed upcoming, faint dotted not taken), a group as two half-width nodes with fork and join curves (three or four stacked in a dashed frame) and a join dot, and a branch as an If frame with one lane per path (if, else if, unless, otherwise; a path not taken folds to one row). Only the running state glyph blinks. A path holds single stages in the schema, so a nested if is not supported yet; the lane draws its nodes with the same column code a nested step would use.
- **No phone layout.** It stays in the web app. The needs-you notifications and the screen-reader announcement came with screen 3 (below).
- **Tools menu.** The perf overlay, the bench and the IME check are commands of the **Tools** menu.
- **`--gate2`.** It scrolls the sidebar tree and a bench terminal while the bench runs, then shows the task table in main while the bench's 10 shells keep printing out of sight, and scrolls the table for 20 s. Then it opens the task's workspace with four of the bench's shells as panes, still printing, and scrolls the first pane for 20 s (screen 2). Then it closes the bench and types into a quiet shell. It saves four perf reports: bench, table, panes and typing. With `--terminals N --output agent` it benches up to 60 shells and saves two more, `hidden` and `show-latency` (docs/rust-phase1.md, "Gate 2: frame time and latency"). Every report has `start.first_frame_ms` and `start.first_data_ms`, the times from the start of the app to the first frame and to the first task list.
- **Smaller differences.**
  - Ages of a week or more show the date in UTC: the app has no local time zone helper yet.
  - The status bar's need-you count (the title bar pill until T-0158) opens the jump palette on what needs you, as the web (since screen 7; before, they filtered the list).
  - The status bar has a fourth link word, "Not connected", while the daemon cannot be used. A token the daemon refuses opens Sign in, as the web (since screen 7). It does not measure overflow; only the jobs label drops by width. The jobs menu does not update while open.
  - The drawer has no Tab focus trap. Esc and a click on the scrim close it.
  - Single-key commands are not in the menu bar, because a macOS menu fires a key without a modifier while the user types. Off macOS, Ctrl chords without Shift go to the shell inside a terminal.
  - New command ids: `app.quit`, `list.first` and `list.last` (the web's `list.ends`), `tree.expand` (from `tree.collapse`), and `tools.*`. The menu items that the web writes in its components are commands too, with no key: `task.add-subtask`, `row.pin`, `row.unpin`, `row.copy-branch`, `row.copy-path`, `task.copy-id`, `agent.start`, `agent.restart`, `task.cancel`, `task.delete`, `project.blur`, `project.show-names`, `project.remove-finished` and `project.remove`. A menu item shows the key of its command: F2 for Rename, Delete for Remove workspace, Enter for Open workspace.
  - Blurred projects show a soft blurred bar as wide as the text in place of it (`sidebar/blur_bar.rs`; GPUI has no text blur, so the bar is a drop shadow, not blurred letters). Tooltips keep bullets. Rows take a 2 px focus border. Esc in an empty filter moves the focus to the tree, so the single keys work at once.
  - In the sidebar tree `j` and `k` move the selection, so they are not type-ahead letters.
  - Board: a folded column's name is in its tooltip (no vertical text); no edge fades or scroll arrows. Table header cells are mouse-only. A disabled Start agent item has no "Waits for …" hint.
  - Start a task: the branch and skill pickers are plain menus with no search; pasting a screenshot is not supported. The files-not-committed dialog of Remove workspace offers Close, View changes and Commit… (since screen 4).
  - Agent icons (T-0148): the web's icons, `claude`, `gemini-ai`, `copilot`, `cursor` from material-icon-theme 5.39.0 (MIT) and the app's own `codex` (shape from lobe-icons, MIT), in `assets/icons/agents/` with both licences. In the light theme a `_light` icon is used when there is one. An unknown agent shows the generic bot icon.
  - The toast timer pauses on hover only, the resize handle collapses during the drag, and context menus have no type-ahead.

## Screen 2: where it differs from the React screens

- **Tabs.** Terminal, Changes, History and Docs (keys 1 to 4) since screen 6, and the editor tab while a file is open. Since T-0183 there is no tab bar: see "Lean layout", **Workspace**.
- **Add to Agents** is Shift+Enter (or Shift and a click) on a task in the palettes, as in the web (screen 7). Commit… and View changes came back with screen 4.
- **Pickers.** "Branch from" and "Add a pane" are plain menus with no search, as the pickers of screen 1.
- **Times.** "ended 7:05" of the wrap-up and "Finished" of the summary are in UTC and say so: the app has no local time zone helper yet.
- **Connection strip.** As in React, "Connecting…" and "Reconnecting…" show only after 200 ms, so a quick connect shows no strip; a terminal that could not start says so at once.
- **Close a terminal.** No confirm, as in the React screen (`ShellPanes.tsx`), although `ui-spec.md` 4.23 asks for one when a program other than the shell runs. The wrap-up's Stop asks first, as React does.
- **Grid.** The panes sit in flex rows; the last pane of the last row takes the rest of it, as `paneSpan` does in the CSS grid.
- **Hidden terminals.** As `terminal-cache.ts`, with a larger limit: the native app keeps at most 30 terminals connected off screen (`KEEP_ALIVE_TERMINALS`; the web keeps 8), each at most 20 minutes. The daemon parks them: no output reaches a hidden terminal until it is shown, then one catch-up comes at once; the terminals of a task that is gone, finished or has no workspace go once hidden. The 20-minute limit is checked when the app draws, not by a timer.
- **No phone layout**: no pill switcher of terminals and no "Git actions" menu.
- **`--gate2`** shows the panes grid with four bench shells printing and saves the `panes` report (screen-1 section, `--gate2`). The bench shells are plain terminals, so their panes show the terminal mark, not an agent's.

## Screen 3: where it differs from the React screens

- **Details panel modes.** Docked at 1280 and wider while main keeps 640, a sheet from 768 to 1279 (with a scrim under 1024), as `decideLayout`. The window is at least 800 wide, so the page mode of the web (under 768) never shows. The panel is 320 to 560 wide, kept in `view.json` (`detailsWidth`). Since T-0239 the docked panel is 280 to 560 wide, 320 by default (see the details dock below). As the web, the open state of each task's panel is not kept across runs; the picked tab is (`detailsView`).
- **Tabs.** Progress, Files (since screen 6, only for a task with a workspace) and Details; a remembered Files tab shows Progress for a task with no workspace. Since T-0183 Changes, History and Docs are views of the panel too, on a rail: see "Lean layout", **Details rail**.
- **Output chip.** Since screen 6 the output file chip of a stage card opens the file in the Docs tab. A chip of a file that is not written yet opens nothing.
- **Stage commands.** Skip the stage, Mark the stage as done, Approve the stage anyway, Send back and Show the task progress run from the palette, as in the web (since screen 7). The menu bar's **Task** menu keeps them too, with no key.
- **Keys.** `a` and `r` work from the details panel too (not in its text boxes, nor in a terminal); `e` edits, `i` shows or hides the selected task's panel, Mod+Alt+B shows or hides the panel. The phase-1 bench had Mod+Alt+B: it is Mod+Alt+T now. F6 does not cycle the parts of the window.
- **Show details** is in the table's row menu, as in the web; the sidebar's row menu has none, as in the web.
- **Notifications.** At first GPUI had no system notification API, so the app showed a small window of its own at the top right for 10 s. Since T-0201 (2026-10-27) our GPUI fork's `show_system_notification` posts a macOS notification (`UNUserNotificationCenter`; none when the app does not run from its bundle), the main window plays the alert sound once per batch, and `app/dock_badge.rs` sets the needs-you count on the Dock icon through `objc2-app-kit`, as Electron's `setBadgeCount` did. A task that starts to need the user, or becomes done, while the app window is not active rings; a click brings the app forward and opens the task. The account menu's **Notify me** check is the app's own switch (`notify` in `view.json`; a status bar bell until T-0170), off by default as the web starts without the browser's permission.
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
- **Icons.** The History tab, Collapse all, the fold, wrap and image toggles and the file rows use Zed's icons.
- **History's loading state** is the skeleton with no "Reading the history" text: GPUI has no screen-reader label yet.
- **Ages** in History are in UTC, as on the earlier screens.
- **Shot sizes.** At 1024x720 the sidebar docks and main is under 720 wide, so the Changes states show the narrow layout (the "Changed file" picker, Inline); at 1000x680 the sidebar is a drawer and they show the file panel and Side by side, as the reference does at 1440.

## Screen 5: where it differs from the React screens

- **Tabs.** Stages, Issues and JSON over one draft since screen 7. A file the Stages tab cannot read opens on the JSON tab, as the web, also when it comes from Import (screen 5 refused it there).
- **A file the app cannot read** (one the daemon reads, with a field the app's types cannot hold) opens with its text on the JSON tab, which says why (screen 5 started from the defaults).
- **Tab** moves through the stage rows, Add stage, the text fields on screen (or the picker's), Cancel and Save, in the order they are drawn. The checkboxes and the pick menus are not on it yet.
- **Dialogs over dialogs.** The questions of the editor (Discard your changes?, Reset to default, Remove, Import, a broken file) open over the project's Settings page, which keeps its state under them.
- **Stage keys** are written in the order of the schema, not of the file, and a key the schema does not know is dropped from a stage when the steps are written back. A field of the wrong JSON type cannot be read: the file is refused at Import (or the screen starts from the defaults), where the web shows the problem next to its stage.
- **Messages.** The pipeline problems copy zod's texts; "This is not JSON: …" gives the JSON reader's own message.
- **Export** asks where to save `<project>.natamux.json` and says "Exported <file>."; the web downloads it.
- **Settings read again.** The daemon sends no event when a project's settings change. As the web reads them again when its tab gets the focus, the app reads them again when its window becomes active, when the project list changes, and after its own save. The open dialog keeps its draft; a change by another client shows as Reload and Overwrite on Save.
- **Pickers.** The agent, kind, condition, "Add a stage…" and Insert skill pickers are plain menus with no search, as the pickers of screen 1.
- **Icons.** Zed's icons nearest to the web's: Book for BookOpen, ToolHammer for FlaskConical, PullRequest for Rocket, Person for Hand and UserRound, GitBranch for GitFork, Menu for GripVertical (`stages/kind_icon.rs`); the Progress segment uses ListTodo.
- **Progress.** The marker's tooltip names the status without the stage's name; the stage columns scroll sideways between the fixed Task and Next columns; a row's title has no underline on hover, the row is tinted.
- **Keys.** `stage.move` of the web is two commands, `stage.move-up` (Alt+Up) and `stage.move-down` (Alt+Down), in the stage list. Mod+Enter saves through the dialog's own key.
- **Shot sizes.** The editor states scroll the dialog to the stage list. At 1024x720 the list and the details sit side by side; at 1000x680 and 800x560 they are one column, so those shots show the list and the details are under it.
- **Window sizes.** The window is at least 800 wide, so the web's phone rules of the editor and the preview never apply; the list and the details sit side by side from a 1024-wide window.

## Screen 6: where it differs from the React screens

- **The editor is Zed's editor**, not Monaco: a writable buffer with no project, line numbers, the syntax colours of the grammars of screen 4 (a file of another language is plain text), soft wrap for prose files. A new text on disk goes into the same buffer, so the cursor and the scroll stay, and that change is in the undo history. Save is Mod+S through the app's command `file.save`.
- **Multi-line editors** (the editor tab, the Docs edit) take Enter, Tab, Shift+Tab, the arrows, Page Up and Page Down, and the moves to the start and end of the text. Single-line boxes leave the arrows and Enter to the lists and forms around them.
- **Markdown.** Zed's `markdown` crate (owner, 2026-10-24), in Zed's spacing (`workspaces/docs_markdown.rs`): no HTML, no Mermaid, no image loading, and only web and mail links open. An image written inline as a `data:` address is drawn (the crate decodes it with no loader), where the web shows only its alt text; an image from a file or the web shows nothing.
- **Drop on a terminal.** A path drags only inside the app, onto a terminal pane of the same task. The paste is bracketed when the program asked for it, as xterm does, and the pane takes the focus. A drag to another app is not offered.
- **Downloads.** An attachment that is not an image asks where to save it and says "Downloaded <name>."; the web downloads it.
- **Quitting** with edits that are not saved asks first, also for Docs edits; the web asks only for files of the editor, through the browser.
- **Reads.** As the web reads only the queries on screen, `workspace.dirty` and the 60 s fallback poll read again only the Files tree (while the panel shows Files), the Docs list or the open file (while its tab shows); a hidden one reads again when it shows (`dirty_reads`). The git state of the open workspace reads as before.
- **A path outside the worktree** is answered 409 by the daemon (`OutsideWorkspaceError`), not 400.
- **Icons.** Files show Zed's file icons (`icons/file_icons` of Zed's assets), picked from the web's Material Icon Theme name (`files/file_icon_view.rs`, owner 2026-10-24): fewer kinds than the web's set, so some types share an icon (all docs show the book, many configs the code icon). RefreshCw is Zed's RotateCw. The Rendered \| Source switch shows icons with its labels.
- **Sizes.** The `edited` and `Modified` marks use Zed's smallest label size (the web's 2xs is smaller). All code text (terminal, diff, the editor tab, the JSON tab, the Docs edit and source) uses the buffer font size, 13 (T-0158).
- **Shots.** At the three shot sizes the details panel is a sheet over main, so it hides the right part of the editor and Docs headers (Revert, Save, the view switch, Edit); the owner checks them docked on the CI build.
- **Window sizes.** The Docs list is always a column on the left: the window is at least 800 wide.

## Screen 7: where it differs from the React screens

- **The palettes** are the app's own view, not Zed's picker (that crate is not in the lock): the web's ranking (`fuzzy.ts`) is ported as it is, so the same query gives the same rows. As the web, the jump palette reads the terminals of every workspace while it is open when there are at most 20 (`terminal_reads`, `TerminalReader::Palette`); the rows take the lists as they come. Alt+1 to 9 match the key, not the typed character (Alt+digit types a symbol on macOS). The footer does not wrap.
- **Sign in.** The token typed in is kept in `daemon-access` in the app's data folder (mode 0600), not in the browser; the line says "It stays on this computer." Connect asks the daemon for the task list first: a refused token keeps Sign in with the web's message, and another error (the daemon is down) shows its message there too, where the web would open the dashboard and show the load error. A 401 later deletes only the app's own file and opens Sign in; the Electron app's `daemon-token` and `NATAMUX_TOKEN` are never changed. The macOS Keychain would need a new crate. There is no Sign out item, as in the Electron app.
- **Settings** has no Desktop app part (keep the computer awake, check for updates): those are settings of the Electron app. Theme and UI size use a text-only segmented control.
- **UI size** sets Zed's UI and buffer font sizes (16 and 13 times 0.92, 1 or 1.077), and since T-0156 most literal sizes of the views are in rems, so bars, dialogs, chips, toasts and spacing follow, as the web's CSS zoom scales everything. Still in pixels: the task table rows and header (`tasks/task_table.rs` `ROW`, `CARD_ROW`, `HEADER`), the sidebar rows (`sidebar/workspace_row.rs`, the height and padding of each tier), the Progress board rows and columns and the stage rows (`stages/progress_board*.rs`, `stages/stage_rows.rs`), the window breakpoints, the sidebar and panel widths kept in `view.json`, the terminal panes and the git graph lanes. A terminal font picked by the user keeps its size.
- **Terminal font.** As Zed (owner, 2026-10-24): with no font picked, a terminal takes Zed's buffer font (Lilex) and the buffer size (13 since T-0158, was Zed's 15; scaled by the UI size); the web uses Ghostty's font, else JetBrains Mono 13. The app reads no Ghostty config. A font picked on the Appearance page of Settings wins; a part left empty is Lilex or 13.
- **The account menu** opens where it was clicked, kept inside the window. "Terminal font…" and "Report an issue" show Zed's FontSize and Debug (a bug) icons.
- **Projects** has a **Browse…** button after Folder and after Worktree folder (T-0225), as the Electron app. It opens the system folder picker on the app's machine; a daemon on another machine refuses a path it does not have, with its reason under the field. The text fields use the buffer font.
- **Import issues** shows the raw message behind the empty state's own "Show details".
- **Status bar layout rules.** The bar uses only the width rules (`forced_by_width`), as before; the measured condensing of the web (`condense`, the `measure` cases) is ported with its tests but not used yet, so those items keep item-level `allow(dead_code)` in `app/status_layout.rs`.
- **The JSON tab** is a Zed editor with JSON colours and no line numbers, not a textarea; Tab types a tab there, so Tab does not leave it (Esc closes the dialog as before).
- **Seen in the CI shots** (run 39979437375):
  - Palette: the first nine rows show their Alt key (⌥1 to ⌥9) at the right, where the web only names it for screen readers; the footer says "Tab needs you" where the web draws ⇥. At the shot sizes the list ends inside Suggested and scrolls. Suggested follows the web's order since T-0156 (a rank per command).
  - Account menu: the section titles are "Theme" and "UI size" as written, not in caps; "Terminal font…" has the Zed FontSize icon.
  - Project settings: the tabs are Zed's tab bar (boxed tabs on a strip), not the web's underlined tabs. A button that is off (Save, Import, Add project, Start agent, the git buttons) is grey with a dimmed label, as the web greys it (see "Buttons" under the lean layout); and the JSON tab shows the parse error and the problem list above its editor, so they show without scrolling.
  - Import issues: Retry is a plain button with no border.
  - Projects: the fake daemon's worktree folder is `/work/.natamux/worktrees`, the reference's demo `/data/worktrees`.
  - Sign in: `NATAMUX_TOKEN` in the help line is not in the mono font; the window keeps the traffic lights and no title bar.
- **Keys.** F6 and Shift+F6 (move between the parts of the window) and the accessible diff view are not in the app, so the shortcut sheet does not list them. The sheet lists "First or last row", "Collapse or expand a group" and "Move a stage" as one row each, as the web, although the app has a command per key.

## Lean layout (T-0158): where it differs from the React screens

The owner asked for a layout as lean as Zed's (2026-10-24). Three mockups were reviewed by three agents, and a mix of them was picked (`.natamux-task/mockups/` of T-0158). The native app now differs from the web here:

- **Title bar.** Only the breadcrumb (the view name, or `project / id title` and the state glyph, whose tooltip has the branch), a `+` button and the Account button (since T-0170, as Zed's user menu). The `+` opens a menu: **Add a task** (`n`) and **Start a task** (`c`). No wordmark, command center, needs-you pill or panel toggles: Cmd+K opens the palette.
- **Status bar.** Since T-0170 it is as lean as Zed's. Left: toggles for the sidebar (Mod+B) and the Agents grid (new `agents.toggle`; off goes back to the task or list shown before). Right: need you and running as an icon and a count, as Zed's diagnostics (the words are in the tooltip; each hides at 0), jobs ("N jobs"), the link dot, and last the details toggle (Mod+Alt+B), as Zed puts the right dock's toggle at the right. The link word shows only while the link is down ("Connecting", "Reconnecting", "Not connected"); "Live" is in the dot's tooltip. Under 1024 wide jobs are icon-only. The notify switch is **Notify me** in the account menu, the account is in the title bar, and the keyboard item is gone (the account menu and `?` open the sheet). The Projects and Import issues items are gone: the palette has **Add a project** and **Import issues**.
- **Link changes.** A read that is slow or cannot connect no longer shows "Reconnecting": that part is read again later, and only a closed `/events` socket, or 3 failed task list reads in a row, takes the link down. Each link change (down with its reason, live again after N s, refused) is one line in `logs/app.log` in the data folder.
- **Sidebar filter.** A bar as high as Zed's tab bars (`Tab::container_height`) at the top of the sidebar, so one line runs under the filter, the workspace tabs and the details tabs (T-0170).
- **Sidebar.** One line per row, 26 px (terminal rows 24): state glyph, title (the id before it only with **Task ids in the sidebar**, Settings › Appearance, off by default since T-0240), and on the right the age or, for a task that needs the user, a short reason (**Answer**, "Plan failed", **Blocked**). A failed or waiting row has a 2 px bar in its state colour. The id, step, stage, activity, PR and full reason are in the title's tooltip. **Answer** is the first item of a waiting row's menu. Projects are a flat tree, not cards.
- **All tasks.** One 32 px toolbar: title, count, state filter, and icon toggles for Table, Board and Progress. No quick add row: **Add a task** is a small dialog (`n`, the `+` menu, the palette, the empty state, the palette's "Add task “q”"); Enter adds, Mod+Enter opens Start a task with the title. Import issues starts on the project Add task used last. The table has no frame; rows are 28 px.
- **Workspace (T-0183).** No header row and no tab bar, also for a task with no workspace and a finished one: the id, title and state are in the breadcrumb. Main always shows the terminals (or the wrap-up of a finished task), and Changes, Docs or the open file cover them while one is picked (owner, 2026-10-26). Each covering screen's own bar ends with **Terminals** (Esc); key `1`, a terminal row in the sidebar and Esc show the terminals again. **New terminal** is a split button in every terminal pane header of the workspace, before Maximize (not in the Agents grid); **Back to wrap-up** of a finished task's terminals is in their pane headers too. Push, Create PR and Open pull request moved under the commit box of the panel's Changes view, labelled; Commit… is gone there, as the commit box is right above. Nothing closes but the open file: **Close the file** (Mod+W); **Reopen the closed tabs** is gone. The commands are **Show the terminals** (`1`), **Show Changes** (`2`), **Show History** (`3`), **Show Docs** (`4`). A saved `history` tab opens on the terminals. The web keeps its tabs.
- **Details dock.** The panel docks at the right and pushes main from 768 wide while main keeps 400 (was 1280 and 640). The sidebar starts 280 wide (was 320, the same as the details), so at 1024 wide both dock beside a 424 px main. Since T-0239 each side has its own width, 320 by default: the sidebar `sidebarWidth` (280 to 480), the docked panel without its 40 px rail `detailsWidth` (280 to 560). A drag of a resize line moves only its side. From 1024 to 1279 wide both take at most 30% of the window, so at 1024 both are 307 beside a 410 px main. The resize line is the panel's only left border, as for the sidebar, so both lines are 1 px. It is open or closed for all tasks at once, as a Zed dock, and `view.json` keeps it (`detailsOpen`, open by default). With no task picked it says "Select a task". It hides while the Agents grid shows. As a Zed dock it has no close button (the status bar toggle, Mod+Alt+B and Esc close it); a sheet or a page keeps its close or Back. It never closes by itself: the web closes the panel when Changes opens and main is under 900; the app does not, as the flag is saved for every task. The status bar toggle is off, with its reason in the tooltip, over the Agents grid and where there is no task and the details would be a sheet or a page.
- **Details dock header (T-0171).** No tabs: the header shows the view's name (since T-0183) and, on Progress, the task id and its stage. Beside an open task the views and "..." are on the rail; over the list (no rail) the header keeps the views as icon buttons and "...". The pencil is gone: Edit task is the first entry of "...", and `e` still opens the form. The web keeps its tabs and pencil.
- **Details rail (T-0183).** The panel's views stand as icon buttons in a 40 px column at the right edge of the window, top to bottom: Progress, Changes (with the count of files to commit), History, Docs, Files and Details (the picked one on a grey pill, the name and key in the tooltip, Changes with "N files to commit"), then "..." (the task's menu) at the bottom. Changes, History and Docs are there only beside the open workspace, Files only for a task with a workspace. The rail shows beside an open task only, not over All tasks, the Agents grid or Settings. It stays while the panel is closed; a click on the picked view of an open panel closes the panel, as Zed's and VS Code's side bars. The rail is not counted in "main keeps 400", so at 1024 wide the panel still docks beside the sidebar (main 384); a sheet sits left of the rail. Changes: the panel has the files, the commit box and the git buttons under it, main the picked file's diff (a click on a file shows it); with the panel closed or on another view, main shows the whole Changes screen as before. Docs: the panel has the stage files and attachments, main the picked file. History: the graph in the panel, without the author column. Keys `2` to `4` open the panel on their view; `2` and `4` also cover the terminals.
- **Text sizes.** One scale (owner, 2026-10-24: the terminal looked big and the views did not match): code 13 (terminal, diff, editors; was 15 for the terminal and diff, 12 for the editors), the main text of a row or tab 13 (sidebar, task table, file trees, changed files, history, workspace and details tabs; `components/text_size.rs`), meta text 12, uppercase section heads 11.
- **Dialogs.** No description line, except Remove workspace (data loss) and confirms. The removed sentences are the tooltips of the dialogs' main buttons.
- **Settings (T-0174).** One Settings page, as Zed's, takes the place of the Settings dialog, the Terminal font dialog and the Project settings dialog (owner picked mockup A of three: Zed, VS Code, JetBrains). It fills the window body (the title bar says "Settings"; the status bar stays) over what main showed, which shows again when it closes. At the left a nav, as wide as Zed's panels (220): a search ("Search settings"), then **General** (Notify me; Updates: **Version** with the updater's state under it (T-0214), **Check for updates automatically** and **Check now…**, which reads **Restart to Update** while an update is ready), **Appearance** (theme, UI size, **Task ids in the sidebar** (T-0240), and the terminal font: family, size with − and +, a preview, **Use built-in font**), a **Keyboard shortcuts** link (the shortcut sheet), and **Projects** with `+` (Add a project) and one row per project. A search shows the rows it finds, working, under "Results", and the projects it finds as links; nav rows with nothing found turn muted. App settings apply at once; the font fields on Enter or when they lose focus, so a terminal does not lay out again on each key. A project's page is the editor of Project settings with its path line, its tabs and a bottom bar (Reset to default, Import…, Export, the problem count, **Discard**, **Save**). Its draft stays while other pages show and after Settings closes; a dot in the nav marks a project with unsaved changes; Discard asks, then reads the file again. A quit drops a draft without asking, as a quit dropped an open dialog. The stage list and the details sit side by side from 1200 wide (was 1024 for the dialog), so the details keep their width beside the nav. Esc closes the picker or skill menu first, then clears the search, then closes Settings; Mod+Enter saves a project page; Tab moves over the search, the nav and the page. The sidebar and details toggles (and Mod+B, Mod+Alt+B) over Settings only close it; opening a task, All tasks, the Agents toggle and need you close it and show their view. The project gear and menu, and the Projects dialog's Settings, open the project's page; Mod+, and the account menu open the page shown last (Appearance at first). The web keeps its dialogs.
- **Buttons (T-0168).** Every button is a grey Zed button (`components/button_style.rs`); the web's blue primary and red danger buttons have no twin. An action (a dialog's main button, Save, Approve, Commit, Start) is `Outlined`: grey with a border. A quiet one (Cancel, Keep editing, Retry, a text link) is `Subtle`: no border, grey on hover. An icon and a key hint sit before the label (`⎋ Cancel  ⌘↵ Save`), as the owner asked; only a dropdown keeps its chevron at the end. Key hints are Zed's `KeyBinding`, 12 px, as in Zed's modal footers, which also show Esc on Cancel. A danger answer is grey too: the title and the label say what goes. Why no tint, fill or transparent button: Zed's hover only darkens a tint by 5%, fades a fill by half and gives a transparent button no background, so they showed no grey hover (the details panel's Approve). A unit test fails on such a button. Text buttons are 22 px; their label is small in panels, rows and toolbars and the default size in dialog footers. Button rows wrap and labels from data are cut with an ellipsis, so no button goes out of a narrow window.

## Motion (T-0160)

The app moves what the web moves (`docs/ui-spec.md` 2.8), on the same 125 ms ticks: a running state glyph blinks once every 2 s (ticks 8 and 9), the busy spinner steps one of its 8 dashes a tick (the jobs item, the connection strip, the restore cover, the preparing step, a busy git button), and the skeleton list pulses. Nothing else moves. Unlike the web, the line into a running stage is still (owner, T-0164): only its glyph blinks.

- **How it stays light.** One clock for the app (`components/motion.rs`, frames in `components/motion_rules.rs`). An item asks for its frame while it renders, which marks the view that drew it. One timer sleeps until the next tick where a shown kind changes and notifies only those views; the sidebar and the task list are cached views, so a blink in one does not draw the other. With no moving item on screen no timer runs. No `with_animation`, no `request_animation_frame` loop.
- **Still forms.** A window without focus, and every `--shot`, draws the still forms, as the web's reduced motion: the disk, three dots, still bars. The timer stops without focus.
- **Reduce motion.** Not followed: GPUI has `App::reduce_motion`, but it never reads the macOS setting (Zed sets the flag from its own setting), and reading it needs an Objective-C call.
- **Cost.** `scripts/motion-cost.sh` runs the fake daemon's task table (3 running tasks, 2 jobs, no terminal output) for 15 s with focus and 15 s behind Finder, on the macOS runner, with `--count-draws`. CI runs it on a snapshot push with a line `Measure: motion` in its commit message. A second run opens task T-0146 with `--open-task` (4 shell panes, nothing printing), with no shot, so motion and the clock run as for a user. In that run the fake daemon has no running job, so it measures an idle open task: only the running glyphs blink, twice every 2 s. Both runs fail above 0.2 draws a second in the background. The open-task run also fails above 1 draw a second or 2% CPU with focus, and when a number is missing. The task-table run only reports with focus, as the jobs spinner of the fake daemon draws 8 times a second.

  | Build | Focused CPU % | Focused draws/s | Background CPU % | Background draws/s | Run |
  |---|---|---|---|---|---|
  | Before (still icons) | 0.7 | 0 | 0.7 | 0 | 40063971436 |
  | After (motion) | 2.2 | 8.3 | 0.9 | 0.07 | 40066463345 |

  With focus the window draws 8 times a second, for the jobs spinner; the blinks fall on its ticks, so they add no draws. Behind Finder it drew once, when the focus left (0.07 a second over 15 s). The still forms keep the shots the same as before, except the spinner places, which show three dots instead of the loader icon.

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
- `StageMarker.test.tsx`: `markerLabel`, `linkKind` and the even-pixel rule have Rust tests; the cases that check the SVG and the CSS classes ('draws the node of a branch as a diamond', 'keeps the label for screen readers', the shapes of 'shows … stage') are covered by the shots. 'draws the link into a running stage as one moving fill' is not ported: the app's line is still (T-0164).
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

Screen 7 (T-0154): every case of `features/palette/__tests__/fuzzy.test.ts` and `palette.test.ts` (`palette/fuzzy_tests.rs`, `palette/palette_rows_tests.rs`, `palette/jump_rows_tests.rs`), `features/projects/__tests__/issues.test.ts`, `lib/__tests__/theme.test.ts`, `ui-size.test.ts`, `features/terminals/__tests__/terminal-font.test.ts` (without its Ghostty cases), `features/stages/editor/__tests__/settings-json.test.ts`, the screen-7 cases of `commands.test.ts` and the Issues tab, JSON tab and one-draft cases of `ProjectSettingsDialog.test.tsx` (`projects/settings_tab_rules_tests.rs`) has a Rust test of the same name, except:

- `CommandPalette.test.tsx`: `capRows`, the scope cycle and the create rows have twins in `palette/palette_rows_tests.rs`; 'keeps its rows in place when tasks change while it is open' is the snapshot the view takes at open; the keys, focus, inert page and the 500-workspace render cases are covered by the shots and the owner's check.
- `RepoPanel.test.tsx`: the worktree folder rules have twins in `projects/project_rules_tests.rs`; the Browse cases have twins in `projects/projects_dialog_tests.rs` (T-0225).
- `TerminalFontDialog.test.tsx` and `SettingsDialog.test.tsx`: their rules are tests of `terminals/terminal_font_tests.rs` and `app/appearance_rules_tests.rs`; the Desktop app cases of `SettingsDialog.test.tsx` are not ported (no Desktop app part).
- `ghostty-config.test.ts` and the Ghostty cases of `terminal-font.test.ts`: the app reads no Ghostty config since T-0156 (owner, 2026-10-24); the terminal is as Zed's.
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
| `app-macos`: shots match baselines | 2026-10-24 | Pass | Baselines from run 40026431931 (T-0156, the Zed look) in `crates/natamux-app/tests/shots/`; run 40030114162 compares them and fails on a difference |
| `app-macos`: `--smoke`, zip at most 60 MiB | 2026-10-22 | Pass | Run 39824338735, job `app-macos` |
| `app-windows`: build and `--smoke` | 2026-10-22 | Pass | Run 39824338735, job `app-windows` |
| Frames while scrolling the sidebar tree and the table with 10 agents (`--gate2`, mean of 4 runs) | 2026-10-24 | Pass | Owner, on the beta; no numbers recorded |
| Cold start to the first frame (limit 300 ms) | 2026-10-24 | Pass | Owner, on the beta; no numbers recorded |
| Cold start to the list with data (limit 1 s) | 2026-10-24 | Pass | Owner, on the beta; no numbers recorded |
| Vietnamese Telex and VNI in every screen-1 text box | 2026-10-24 | Pass | Owner |
| Same API call as the React screen for each action, web dashboard shows the result | 2026-10-24 | Pass | Owner |
| `native-beta.yml` makes a pre-release, its zip passes `gh attestation verify`, `/releases/latest` still points at the newest Electron release | 2026-10-23 | Pass | Native beta run 39843667186 made `native-beta.1` from Rust run 39824338735 (66f2aac); `/releases/latest` is still v0.1.43 |
| README and site install page, PR into `main`: `scripts/readme.test.py` and the site build | 2026-10-23 | Pass | PR #430, merged; `audit` failed on a DOMPurify advisory of `main`'s own dependencies, not on this change |
| Owner sign-off, light and dark | 2026-10-24 | Pass | Owner, on run 40002366175 (T-0156: all screens in the Zed look); the baselines are of run 40026431931, the same look with softer state tints and smaller radii |

### Screen 2

| Part | Date | Result | Notes |
|---|---|---|---|
| `cargo xtask ci` | 2026-10-23 | Pass | Rust run 39842444315 on `natamux-ci/t-0148` (a8cac9a), job `check` |
| Ported rule tests (same names as the TypeScript tests) | 2026-10-23 | Pass | In `check`; the cases left out are listed above |
| Client tests and live feed (five routes, `WorkspaceChanged`, `ShellsChanged`) | 2026-10-23 | Pass | In `check` |
| `live-daemon` screen-2 flow | 2026-10-23 | Pass | Run 39842444315, `runs_the_reads_and_writes_of_screen_2` |
| `app-macos`: the 8 screen-2 states, both themes, three sizes; shots match baselines | 2026-10-24 | Pass | Baselines from run 40026431931 (T-0156, the Zed look) in `crates/natamux-app/tests/shots/`; run 40030114162 compares them and fails on a difference |
| `app-macos`: `--smoke`, zip at most 60 MiB | 2026-10-23 | Pass | Run 39842444315, job `app-macos` |
| `app-windows`: build and `--smoke` | 2026-10-23 | Pass | Rust run 39846390721 on `dev` (b0d8c73, PR #431 merged), job `app-windows` |
| Frames with 10 agents printing and 4 panes on screen (`--gate2` `panes` report, mean of 4 runs) | 2026-10-24 | Pass | Owner, on the beta; no numbers recorded |
| Key to glyph in a pane of the grid (p95 at most 20 ms, app part at most 17 ms) | 2026-10-24 | Pass | Owner, on the beta; no numbers recorded |
| Vietnamese Telex and VNI in a pane and in the find field | 2026-10-24 | Pass | Owner |
| Same API call as the React screen for each action, web dashboard shows the result | 2026-10-24 | Pass | Owner |
| Owner sign-off, light and dark | 2026-10-24 | Pass | Owner, on run 40002366175 (T-0156: all screens in the Zed look); the baselines are of run 40026431931, the same look with softer state tints and smaller radii |

### Screen 3

| Part | Date | Result | Notes |
|---|---|---|---|
| `cargo xtask ci` | 2026-10-23 | Pass | Rust run 39869173632 on `natamux-ci/t-0149-check` (a25a785), job `check` |
| Ported rule tests (same names as the TypeScript tests) | 2026-10-23 | Pass | In `check`; the cases left out are listed above |
| Client tests and live feed (eight routes, `TaskLogged`) | 2026-10-23 | Pass | In `check` |
| `live-daemon` screen-3 flow | 2026-10-23 | Pass | Run 39863431903, `runs_the_reads_and_writes_of_screen_3` |
| `app-macos`: the 11 screen-3 states, both themes, three sizes; shots match baselines | 2026-10-24 | Pass | Baselines from run 40026431931 (T-0156, the Zed look) in `crates/natamux-app/tests/shots/`; run 40030114162 compares them and fails on a difference |
| `app-macos`: `--smoke`, zip at most 60 MiB | 2026-10-23 | Pass | Run 39863431903, job `app-macos` |
| `app-windows`: build and `--smoke` | 2026-10-23 | Pass | Rust run 39879426799 on `dev` (b2545a6, PR #432 merged), job `app-windows`; its `app-macos` passed on a rerun (the first try lost the network at the attest step) |
| The docked panel at 1440, both themes | 2026-10-24 | Pass | Owner |
| Vietnamese Telex and VNI in the edit form, the Send back reason and the subtask field | 2026-10-24 | Pass | Owner |
| Same API call as the React screen for each action, web dashboard shows the result | 2026-10-24 | Pass | Owner |
| The notification window | 2026-10-24 | Pass | Owner |
| Owner sign-off, light and dark | 2026-10-24 | Pass | Owner, on run 40002366175 (T-0156: all screens in the Zed look); the baselines are of run 40026431931, the same look with softer state tints and smaller radii |

### Screen 4

| Part | Date | Result | Notes |
|---|---|---|---|
| `cargo xtask ci` | 2026-10-23 | Pass | Rust run 39899125106 on `natamux-ci/t-0150` (e625d71), job `check` |
| Ported rule tests (same names as the TypeScript tests) | 2026-10-23 | Pass | In `check`; the cases left out are listed above |
| Client tests (seven routes, `fresh`, the 404 and 413 errors) | 2026-10-23 | Pass | In `check` (`tests/git_api.rs`) |
| `live-daemon` screen-4 flow | 2026-10-23 | Pass | Run 39899125106, `runs_the_reads_and_writes_of_screen_4` |
| `supply-chain` with the new grammar crates | 2026-10-23 | Pass | Run 39899125106; the lockfile and the `cargo vet` exemptions came from a CI run (39889128968), not from a local cargo |
| `app-macos`: the 7 screen-4 states, both themes, three sizes; shots match baselines | 2026-10-24 | Pass | Baselines from run 40026431931 (T-0156, the Zed look) in `crates/natamux-app/tests/shots/`; run 40030114162 compares them and fails on a difference |
| `app-macos`: `--smoke`, zip at most 60 MiB | 2026-10-23 | Pass | Run 39899125106; the zip is about 50 MB with the grammars |
| `app-windows`: build and `--smoke` | 2026-10-23 | Pass | Rust run 39910369940 on `dev` (3ed1970, PR #433 merged), job `app-windows`; all its jobs passed |
| The diff of a real task in both themes and both layouts | 2026-10-24 | Pass | Owner |
| Vietnamese Telex and VNI in the commit box and the comment box | 2026-10-24 | Pass | Owner |
| Same API call as the React screen for each action, web dashboard shows the result | 2026-10-24 | Pass | Owner |
| Owner sign-off, light and dark | 2026-10-24 | Pass | Owner, on run 40002366175 (T-0156: all screens in the Zed look); the baselines are of run 40026431931, the same look with softer state tints and smaller radii |

### Screen 5

| Part | Date | Result | Notes |
|---|---|---|---|
| `cargo xtask ci` | 2026-10-23 | Pass | Rust run 39927345495 on `natamux-ci/t-0151` (2f57470), job `check` |
| Ported rule tests (same names as the TypeScript tests) | 2026-10-23 | Pass | In `check`; the cases left out are listed above |
| Client tests (`save_settings` with and without `If-Match`, 409, 400) | 2026-10-23 | Pass | In `check` (`tests/settings_api.rs`) |
| `live-daemon` screen-5 flow | 2026-10-23 | Pass | Run 39927345495, `runs_the_reads_and_writes_of_screen_5` |
| `app-macos`: the 7 screen-5 states, both themes, three sizes; shots match baselines | 2026-10-24 | Pass | Baselines from run 40026431931 (T-0156, the Zed look) in `crates/natamux-app/tests/shots/`; run 40030114162 compares them and fails on a difference |
| `app-macos`: `--smoke`, zip at most 60 MiB | 2026-10-23 | Pass | Run 39927345495 |
| `app-windows`: build and `--smoke` | 2026-10-23 | Pass | Rust run 39941043330 on PR #434 (e49fb11), job `app-windows` |
| Progress of real tasks, a pipeline edit saved and seen in the web dashboard, and a web edit seen in the app | 2026-10-24 | Pass | Owner |
| Vietnamese Telex and VNI in the name, prompt and condition fields | 2026-10-24 | Pass | Owner |
| Owner sign-off, light and dark | 2026-10-24 | Pass | Owner, on run 40002366175 (T-0156: all screens in the Zed look); the baselines are of run 40026431931, the same look with softer state tints and smaller radii |

### Screen 6

| Part | Date | Result | Notes |
|---|---|---|---|
| `cargo xtask ci` | 2026-10-23 | Pass | Rust run 39954098408 on `natamux-ci/rust` (d60ca6a), job `check` |
| Ported rule tests (same names as the TypeScript tests) | 2026-10-23 | Pass | In `check`; the cases left out are listed above |
| Client tests (eight routes, `If-Match`, 409, 413, 404, bytes) | 2026-10-23 | Pass | In `check` (`tests/files_api.rs`) |
| `live-daemon` screen-6 flow | 2026-10-23 | Pass | Run 39954098408, `runs_the_reads_and_writes_of_screen_6` |
| `app-macos`: the 8 screen-6 states, both themes, three sizes; shots match baselines | 2026-10-24 | Pass | Baselines from run 40026431931 (T-0156, the Zed look) in `crates/natamux-app/tests/shots/`; run 40030114162 compares them and fails on a difference |
| `app-macos`: `--smoke`, zip at most 60 MiB | 2026-10-23 | Pass | Run 39954098408 |
| `app-windows`: build and `--smoke` | 2026-10-23 | Pass | Rust run 39963088535 on PR #435 (1ff379a), job `app-windows`; every job of that run passed. The `dev` run of the merge (39963121785, d5685cd) was cancelled |
| A file edited and saved in the app, seen in the web dashboard and on disk; an agent's write shows the banner | 2026-10-24 | Pass | Owner |
| A path dropped on a real agent's terminal | 2026-10-24 | Pass | Owner |
| The Docs of a real task, Rendered, both themes | 2026-10-24 | Pass | Owner |
| Vietnamese Telex and VNI in the editor and the Docs edit | 2026-10-24 | Pass | Owner |
| Owner sign-off, light and dark | 2026-10-24 | Pass | Owner, on run 40002366175 (T-0156: all screens in the Zed look); the baselines are of run 40026431931, the same look with softer state tints and smaller radii |

### Screen 7

| Part | Date | Result | Notes |
|---|---|---|---|
| `cargo xtask ci` | 2026-10-23 | Pass | Rust run 39979437375 on `natamux-ci/rust` (f84319a), job `check`; again run 39983890846 on `natamux-ci/rust-check` (df01047, the shortcut sheet's columns kept inside the dialog after the shots) |
| Ported rule tests (same names as the TypeScript tests) | 2026-10-23 | Pass | In `check`; the cases left out are listed above |
| Client tests (`POST /repos`, the GitHub issues and import, the token order, the private token file) | 2026-10-23 | Pass | In `check` (`tests/projects_api.rs`, `config.rs`) |
| `live-daemon` screen-7 flow | 2026-10-23 | Pass | Run 39979437375, `runs_the_reads_and_writes_of_screen_7` |
| `supply-chain` | 2026-10-24 | Pass | Run 40002366175 (T-0156), with RUSTSEC-2026-0386 ignored and its reason in `deny.toml` and `.cargo/audit.toml` (owner, 2026-10-24; tokio 1.53.3 through Dependabot). Before: run 39979437375: `cargo deny` reports RUSTSEC-2026-0386 (tokio 1.52.1, `AsyncFd` I/O safety), an advisory published after the screen-6 runs. It is not from this change: `Cargo.lock` is the one of `dev`. Bumping tokio or allowing the advisory is the owner's call (dependency rules, cooldown) |
| `app-macos`: the 16 screen-7 states, both themes, three sizes; shots match baselines | 2026-10-24 | Pass | Baselines from run 40026431931 (T-0156, the Zed look) in `crates/natamux-app/tests/shots/`; run 40030114162 compares them and fails on a difference |
| `app-macos`: `--smoke`, zip at most 60 MiB | 2026-10-23 | Pass | Run 39979437375. With no token on the runner the app starts on Sign in, so `--smoke` types into its token field |
| `app-windows`: build and `--smoke` | 2026-10-23 | Pass | Rust run 39986604010 on PR #437 (ce984bb), job `app-windows`; again on `dev` run 39986700222 (5df60c1). Both runs failed only on `supply-chain` (RUSTSEC-2026-0386, ignored since T-0156) |
| The palette and the jump palette on real tasks | 2026-10-24 | Pass | Owner |
| A project added and an issue import tried in the app, seen in the web dashboard | 2026-10-24 | Pass | Owner |
| An Issues-tab change saved and seen in the web | 2026-10-24 | Pass | Owner |
| Sign in with a wrong and the right token | 2026-10-24 | Pass | Owner |
| Theme, UI size and terminal font changes | 2026-10-24 | Pass | Owner |
| Vietnamese Telex and VNI in the palette, the font dialog, the Projects form, the JSON tab and Sign in | 2026-10-24 | Pass | Owner |
| Owner sign-off, light and dark | 2026-10-24 | Pass | Owner, on run 40002366175 (T-0156: all screens in the Zed look); the baselines are of run 40026431931, the same look with softer state tints and smaller radii |
