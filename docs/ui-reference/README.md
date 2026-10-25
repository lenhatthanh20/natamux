# UI reference

Screenshots of every screen and main state of the React dashboard, for the Rust rewrite (`docs/plan-rust-migration.md`). The native screens copy layout and behaviour from these. Looks come from Zed's `ui` kit and `theme` crate.

- Version: `origin/main` e93dba6 (0.1.43), images `natamux-daemon` and `natamux-web` `sha-e93dba6`, attestations verified.
- Data: the `storefront` demo project from `scripts/site-capture/seed-storefront.sh`, with stand-in agents. The agents' terminal text is scripted.
- Size: 1440x900 at device scale 2, saved as WebP 2000 px wide. Every shot has a `-dark` and a `-light` file in `shots/`.
- Behaviour is in `docs/ui-spec.md` (section numbers below) and in the React component. A shot shows one moment; the spec and the component are the source of truth.

## Screens

| Shot | Shows | Spec | Component |
|---|---|---|---|
| [tasks-table](shots/tasks-table-dark.webp) ([light](shots/tasks-table-light.webp)) | All tasks as a table; sidebar with projects and workspaces; status bar | 3.1, 4.25, 4.28 | `features/tasks/TaskTable.tsx`, `features/app/AppShell.tsx` |
| [tasks-board](shots/tasks-board-dark.webp) ([light](shots/tasks-board-light.webp)) | All tasks as a board | 4.26 | `features/tasks/TaskBoard.tsx` |
| [board-hover-card](shots/board-hover-card-dark.webp) ([light](shots/board-hover-card-light.webp)) | Board with the pointer on a card | 4.26, 5.3 | `features/tasks/TaskBoard.tsx` |
| [tasks-progress](shots/tasks-progress-dark.webp) ([light](shots/tasks-progress-light.webp)) | All tasks against their stages | 4.32 | `features/stages/ProgressBoard.tsx` |
| [agents-grid](shots/agents-grid-dark.webp) ([light](shots/agents-grid-light.webp)) | Agents grid: the agent panes of many tasks | 4.23 | `features/terminals/AgentPanes.tsx` |
| [add-task-typed](shots/add-task-typed-dark.webp) ([light](shots/add-task-typed-light.webp)) | Quick add to the backlog, with text typed | 4.3 | `features/tasks/TaskTable.tsx` |
| [start-task](shots/start-task-dark.webp) ([light](shots/start-task-light.webp)) | Start a task dialog | 4.14 | `features/workspaces/NewWorkspaceDialog.tsx` |
| [remove-workspace](shots/remove-workspace-dark.webp) ([light](shots/remove-workspace-light.webp)) | Remove the workspace of a task dialog | 4.14 | `features/workspaces/RemoveWorkspaceDialog.tsx` |
| [clear-finished](shots/clear-finished-dark.webp) ([light](shots/clear-finished-light.webp)) | Remove finished workspaces dialog (project menu) | 4.14 | `features/sidebar/ClearFinishedDialog.tsx` |
| [fan-out](shots/fan-out-dark.webp) ([light](shots/fan-out-light.webp)) | Run more tries dialog (details "..." menu) | 4.14 | `features/tasks/FanOutForm.tsx` |
| [task-details-review](shots/task-details-review-dark.webp) ([light](shots/task-details-review-light.webp)) | Details of a task waiting at Manual review | 4.27 | `features/tasks/TaskDetail.tsx` |
| [task-details-needs-input](shots/task-details-needs-input-dark.webp) ([light](shots/task-details-needs-input-light.webp)) | Details of a task whose agent asks a question | 4.27, 4.12 | `features/tasks/TaskDetail.tsx` |
| [task-context-menu](shots/task-context-menu-dark.webp) ([light](shots/task-context-menu-light.webp)) | Right-click menu of a sidebar task | 4.13 | `features/sidebar/WorkspaceRow.tsx` |
| [workspace-terminal-running](shots/workspace-terminal-running-dark.webp) ([light](shots/workspace-terminal-running-light.webp)) | Workspace: four agent terminals, a side by side group, Security review running | 4.23, 4.29 | `features/workspaces/WorkspaceHeader.tsx`, `features/terminals/ShellPanes.tsx` |
| [terminal-find](shots/terminal-find-dark.webp) ([light](shots/terminal-find-light.webp)) | Terminal with the find bar open (Mod+F) | 4.23 | `features/terminals/TerminalFind.tsx` |
| [workspace-terminal-needs-input](shots/workspace-terminal-needs-input-dark.webp) ([light](shots/workspace-terminal-needs-input-light.webp)) | Workspace of a task whose agent asks a question | 4.12, 4.23 | `features/terminals/TerminalPane.tsx` |
| [answer-banner](shots/answer-banner-dark.webp) ([light](shots/answer-banner-light.webp)) | The same task on Progress: the question and Answer in terminal | 4.27 | `features/stages/StageCard.tsx` |
| [workspace-terminal-plan-gate](shots/workspace-terminal-plan-gate-dark.webp) ([light](shots/workspace-terminal-plan-gate-light.webp)) | A Feature task waiting for the plan approval | 4.27 | `features/stages/ProgressTab.tsx` |
| [workspace-terminal-review](shots/workspace-terminal-review-dark.webp) ([light](shots/workspace-terminal-review-light.webp)) | A Bug fix task after review, waiting for the person | 4.23 | `features/terminals/ShellPanes.tsx` |
| [no-workspace](shots/no-workspace-dark.webp) ([light](shots/no-workspace-light.webp)) | Task with no workspace: branch picker, Create workspace, Start agent | 4.19, 4.29 | `features/workspaces/NoWorkspace.tsx` |
| [workspace-preparing](shots/workspace-preparing-dark.webp) ([light](shots/workspace-preparing-light.webp)) | Workspace being prepared: spinner and the step (the `/workspaces/preparing` answer is mocked; the real step is too short to catch) | 4.19, 4.29 | `features/workspaces/NoWorkspace.tsx` |
| [finished-wrap-up](shots/finished-wrap-up-dark.webp) ([light](shots/finished-wrap-up-light.webp)) | Workspace of a done task: finished banner and the wrap-up list with ended terminals | 4.29 | `features/workspaces/FinishedBanner.tsx`, `features/workspaces/WrapUp.tsx` |
| [workspace-progress](shots/workspace-progress-dark.webp) ([light](shots/workspace-progress-light.webp)) | Progress view: stage timeline with Approve, Skip, Send back | 4.27 | `features/stages/ProgressTab.tsx`, `features/stages/StageCard.tsx` |
| [stage-failed](shots/stage-failed-dark.webp) ([light](shots/stage-failed-light.webp)) | Progress with a failed stage: the reason, agent picker, Start agent, Skip, Send back (the terminal's "Stopped" line comes from pausing the stand-in, not from the product) | 4.27 | `features/stages/StageCard.tsx` |
| [send-back](shots/send-back-dark.webp) ([light](shots/send-back-light.webp)) | Send back dialog | 4.14 | `features/stages/SendBackDialog.tsx` |
| [details-files](shots/details-files-dark.webp) ([light](shots/details-files-light.webp)) | Details panel, Files tab | 4.33 | `features/files/FileTree.tsx` |
| [details-details](shots/details-details-dark.webp) ([light](shots/details-details-light.webp)) | Details panel, Details tab: notes, workspace, usage, subtasks | 4.27 | `features/tasks/TaskDetail.tsx`, `features/tasks/UsageSection.tsx`, `features/tasks/Subtasks.tsx` |
| [add-subtask](shots/add-subtask-dark.webp) ([light](shots/add-subtask-light.webp)) | Add subtask dialog (sidebar menu) | 4.14 | `features/tasks/AddSubtaskDialog.tsx` |
| [compare-tries](shots/compare-tries-dark.webp) ([light](shots/compare-tries-light.webp)) | Details tab of a task with tries: state and changes of each try, Keep this try | 4.27 | `features/tasks/CompareTries.tsx` |
| [details-more-menu](shots/details-more-menu-dark.webp) ([light](shots/details-more-menu-light.webp)) | The "..." menu of the details panel | 4.13 | `features/tasks/TaskDetail.tsx` |
| [workspace-changes-empty](shots/workspace-changes-empty-dark.webp) ([light](shots/workspace-changes-empty-light.webp)) | Changes with nothing uncommitted | 4.19, 4.24 | `features/workspaces/ChangesView.tsx` |
| [workspace-changes-list](shots/workspace-changes-list-dark.webp) ([light](shots/workspace-changes-list-light.webp)) | Changes: changed files, side by side diff, commit box, agent comment | 4.24 | `features/git/ChangesPanel.tsx`, `features/git/ChangedFiles.tsx`, `features/git/AgentComment.tsx` |
| [workspace-changes-diff](shots/workspace-changes-diff-dark.webp) ([light](shots/workspace-changes-diff-light.webp)) | Changes with one file chosen | 4.24 | `features/git/MonacoDiff.tsx`, `features/git/DiffToolbar.tsx` |
| [rollback](shots/rollback-dark.webp) ([light](shots/rollback-light.webp)) | Roll back changes dialog: checked files, a new file that will be deleted | 4.14, 4.24 | `features/git/RollbackDialog.tsx` |
| [image-diff](shots/image-diff-dark.webp) ([light](shots/image-diff-light.webp)) | Changes with a new image file chosen (checkerboard preview, size) | 4.24 | `features/git/ImageDiff.tsx` |
| [workspace-history](shots/workspace-history-dark.webp) ([light](shots/workspace-history-light.webp)) | History: the git graph of the task | 4.24 | `features/git/GitGraph.tsx` |
| [workspace-docs](shots/workspace-docs-dark.webp) ([light](shots/workspace-docs-light.webp)) | Docs: the stage files (spec, plan, review) | 4.29 | `features/workspaces/DocsTab.tsx` |
| [workspace-editor](shots/workspace-editor-dark.webp) ([light](shots/workspace-editor-light.webp)) | Editor tab with a Markdown file | 4.33 | `features/files/FileEditor.tsx` |
| [workspace-editor-source](shots/workspace-editor-source-dark.webp) ([light](shots/workspace-editor-source-light.webp)) | Editor tab with a changed TypeScript file | 4.33 | `features/files/MonacoFile.tsx` |
| [palette](shots/palette-dark.webp) ([light](shots/palette-light.webp)) | Command palette, empty: Needs you, suggestions | 4.17 | `features/palette/CommandPalette.tsx` |
| [palette-query](shots/palette-query-dark.webp) ([light](shots/palette-query-light.webp)) | Command palette with a query | 4.17 | `features/palette/CommandPalette.tsx` |
| [shortcuts](shots/shortcuts-dark.webp) ([light](shots/shortcuts-light.webp)) | Shortcut sheet (`?`) | 4.18, 5.1 | `features/app/ShortcutSheet.tsx` |
| [account-menu](shots/account-menu-dark.webp) ([light](shots/account-menu-light.webp)) | Account menu: theme, UI size, terminal font, Report an issue | 4.13 | `features/app/AccountMenu.tsx` |
| [terminal-font](shots/terminal-font-dark.webp) ([light](shots/terminal-font-light.webp)) | Terminal font dialog: family, size, preview | 4.14 | `features/terminals/TerminalFontDialog.tsx` |
| [settings-dialog](shots/settings-dialog-dark.webp) ([light](shots/settings-dialog-light.webp)) | Settings dialog | 4.34 | `features/app/SettingsDialog.tsx` |
| [projects](shots/projects-dark.webp) ([light](shots/projects-light.webp)) | Projects dialog: list and add a project | 4.14 | `features/projects/RepoPanel.tsx` |
| [project-settings](shots/project-settings-dark.webp) ([light](shots/project-settings-light.webp)) | Project settings as it opens | 4.31 | `features/projects/ProjectSettingsDialog.tsx` |
| [project-settings-tab-stages](shots/project-settings-tab-stages-dark.webp) ([light](shots/project-settings-tab-stages-light.webp)) | Stages tab: pipeline preview with a branch, stage list, stage details | 4.31, 4.32 | `features/projects/ProjectSettingsDialog.tsx`, `features/stages/editor/PipelinePreview.tsx` |
| [project-settings-tab-issues](shots/project-settings-tab-issues-dark.webp) ([light](shots/project-settings-tab-issues-light.webp)) | Issues tab: when to open a GitHub issue | 4.31 | `features/projects/IssuesTab.tsx` |
| [project-settings-tab-json](shots/project-settings-tab-json-dark.webp) ([light](shots/project-settings-tab-json-light.webp)) | JSON tab: `.natamux.json` | 4.31 | `features/projects/JsonTab.tsx` |
| [import-issues](shots/import-issues-dark.webp) ([light](shots/import-issues-light.webp)) | Import GitHub issues, error state (no `gh` in the demo) | 4.14, 4.19 | `features/projects/IssueImport.tsx` |
| [sign-in](shots/sign-in-dark.webp) ([light](shots/sign-in-light.webp)) | Sign in with the daemon token | 4.14 | `features/app/SignIn.tsx` |
| [skeleton](shots/skeleton-dark.webp) ([light](shots/skeleton-light.webp)) | Table and sidebar while the tasks load (request held) | 4.20 | `components/ui/Skeleton.tsx` |
| [toast](shots/toast-dark.webp) ([light](shots/toast-light.webp)) | A toast at the bottom right ("Copied the branch name.") | 4.15 | `components/ui/Toaster.tsx` |

## Not shot yet

- The phone layout: not needed for the desktop app; the web keeps it.

## How to reshoot

Use a separate stack, so `natamux-e2e` and `natamux-demo` are not touched:

1. Start the stack as in `scripts/site-capture/README.md` step 1, with `-p natamux-ref` and `NATAMUX_PORT=8091`.
2. Run `install-standins.sh` and `seed-storefront.sh` with `DEMO_CONTAINER=natamux-ref-daemon-1 DEMO_PORT=8091 PW_SESSION=ref`.
3. Shoot with `page.emulateMedia({ colorScheme })` per theme and `page.screenshot({ scale: 'device' })`.
4. Convert with the `linuxserver/ffmpeg` image: `-vf scale=2000:-1 -c:v libwebp -quality 82 -compression_level 6`.

States that need more than the seed. Run the scripts from `scripts/site-capture/` with the same env, and `. ./lib.sh` for the helpers. The daemon allows 30 open terminals; on a stack with many tasks, close old terminals first, or new stages fail their checks:

| Shot | How to reach it |
|---|---|
| `stage-failed` | `./new-task.sh fail`: the stand-in fails the Plan stage. Open the task's Progress tab |
| `finished-wrap-up`, `clear-finished` | `./new-task.sh done`. Wrap up: the task's Terminal tab. Clear finished: the project menu in the sidebar, **Remove finished workspaces**, then **Cancel** |
| `compare-tries` | `./new-task.sh tries` prints the task and its two tries. Shoot the Details tab of a try |
| `no-workspace`, `fan-out` | The task that `./new-task.sh tries` printed first (Ready, no workspace). Fan-out: its details **...** menu, **Run more tries…** |
| `image-diff` | `image <id>` writes an uncommitted `public/banner.png` in the task's worktree. Changes tab, choose the file |
| `rollback`, `remove-workspace` | Any task with a workspace and no uncommitted file (remove workspace shows a blocked dialog otherwise). Open the dialog, then **Cancel** |
| `skeleton` | Hold the task list, then reload: `page.route('**/api/tasks', route => new Promise(done => setTimeout(() => done(route.continue()), 60000)))` |
| `workspace-preparing` | The real step is too short to catch. Answer the list for the no-workspace task: `page.route('**/api/workspaces/preparing', route => route.fulfill({ json: { preparing: { '<id>': 'fetch' } } }))` |
| `toast` | **Copy branch name** in a sidebar task menu. Shoot within about 1 s: the toast closes after a few seconds |
| `terminal-find` | Click a terminal, press Mod+F, type a word that the terminal shows |
