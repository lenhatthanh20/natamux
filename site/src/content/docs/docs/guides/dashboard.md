---
title: Dashboard
description: Find your way around the Natamux dashboard, add and filter tasks, and see which task needs you.
sidebar:
  order: 1
---

The dashboard is the web app of the Natamux daemon. It shows every task, every workspace and every agent on one page, and it updates live.

![The dashboard with the task table, the Needs you strip and the sidebar](/screens/dashboard.png)

## Layout

The window has five parts.

| Part | What it holds |
|---|---|
| Title bar | The sidebar toggle, where you are, **Search or run a command**, the "N need you" chip, **Start a task** and the details toggle |
| Sidebar | A filter field, **All tasks**, **Agents**, **Needs you**, and **Workspaces** grouped by project |
| Main area | The task list as a table, board or progress view, or the open workspace |
| Details panel | The selected task, in two views: **Progress** (its stages, with Approve) and **Details** (notes, workspace, pull request, subtasks, tries) |
| Status bar | The connection state, running count, **Projects**, **Import issues**, **Notify me**, shortcuts and the account menu |

The details panel is open by default beside a workspace, so the stages stay in sight next to the terminal. In a narrow window, where the panel would cover the terminal, and for a task with no workspace, it starts closed. Open or close it with the title bar button (**Show details** / **Hide details**) or `Mod+Alt+B`. Natamux remembers your choice for each task. The palette command **Show the task progress** opens the details on **Progress**. The old link `/w/<id>/progress` does the same.

Under 768 px wide the status bar is hidden and the sidebar becomes a drawer. The status items move to the foot of the drawer.

Press `Mod+B` to hide or show the sidebar. Press `Mod+Alt+B` to hide or show the details. `Mod` is ⌘ on macOS and Ctrl elsewhere. Press `F6` to move between the parts. Drag the edge of the sidebar or the details panel to resize it.

## Sidebar

- **All tasks** shows the task list in the main area. The number is every task.
- **Agents** opens a grid of agent terminals. It starts with the agents that work or wait. Use **Add a pane** to add a task (up to 6 panes), and **Columns** to pick 1, 2, 4 or Auto.
- **Needs you** lists the tasks that wait for you. See [Needs you](#needs-you).
- **Workspaces** lists workspaces in folders, one per project. How much room a row takes follows what it needs:
  - A task that waits on you, failed or is blocked is a tinted card. It shows the agent's question, or why the task failed, on up to two lines. On a task that asks, **Answer** opens the terminal of the agent that asked.
  - A task that works, waits for review or is ready has two lines: the title and age, then the task id, the step and stage (such as "3/5 Review"), the subtasks done, what the agent does now, the pull request and the agent's icon. Hover the id to see the branch.
  - A finished, cancelled or draft task is one short line.
  - The icon before the title has its own shape for each state: a ring for ready, a dot that breathes while an agent works, a diamond for a question, a square for review, a triangle for a failure and a check for done. A blue dot marks a change you have not seen yet.

In the workspace list:

- Click a project to fold or unfold it. Click **+** next to it for **Start a task in** that project. **Add a project** and **Start a task** are in the section header. With no project, the sidebar says **No projects yet** and offers **Add project**.
- Right-click a project, press `Shift+F10`, or click **...** next to it for **Start a task**, **Settings**, **Remove finished workspaces** and **Remove project**.
- Finished workspaces (done or cancelled) are folded under "N finished". To clear them, click **Remove finished workspaces** on that row, or choose it in the menu of the project. Natamux asks first and lists the tasks. It removes the workspaces one after the other. A workspace with changes that are not committed is not removed, and the message names it. Pinned workspaces and other projects are not touched. One message reports the result. The tasks stay, and a branch with commits that are not merged stays.
- Only the open workspace shows the work under it: one row per shell, then its open subtasks. Click a task, or press `Right` on it, to open it; the task that was open before folds. A shell row shows the state, the agent's icon, the stage (or **Shell 2** for an agent you started yourself), what the agent does and the shell (**S1**). A shell with no agent shows a terminal icon and its program, such as **zsh**. Click a shell row to open the task's terminal on it. While a subtask is open, its parent stays open too. Natamux reads the shells of tasks that work or wait, and of the open workspace.
- Double-click a title, or press `F2`, to rename it.
- Right-click a row, or press `Shift+F10`, for **View changes**, **Rename**, **Pin to the top**, **Copy branch name**, **Copy folder path**, **Remove workspace** and **Delete task**. **Remove workspace** deletes the worktree and keeps the task. **Delete task** deletes the task, its notes and its history, and the worktree when there is one. Both ask first. Pinned workspaces move to a **Pinned** section at the top.
- Press `Delete` on a row to remove the workspace. Natamux asks first. The task stays. The branch stays when it has commits that are not merged.

## Task views

Pick a view with the **Layout** switch above the list.

| View | Shows |
|---|---|
| Table | One row per task. Click a column header to sort. |
| Board | One column per state. An empty column is folded to a narrow rail; click it to open it. **Show finished** adds the done and cancelled columns. |
| Progress | Tasks against their [stages](/docs/concepts/stages/) |

![The board view](/screens/board.png)

![The progress view](/screens/progress.png)

The table columns are Id, Task, State, Agent, Branch, Changes, PR and Last activity. A narrow window shows fewer columns. Below 560 px wide each task is a card. Changes and PR appear only when some task has a value.

The default order puts what needs you first: Needs your input, In review, Failed, Blocked, Running, Ready, Draft, Done, Cancelled.

Click a row to select it and open its details. Double-click, or press `Enter`, to open its workspace. The workspace opens on the terminal while an agent works, and otherwise on the tab you used last.

Right-click a row, or press `Shift+F10`, for **Open workspace**, **Show details**, the actions for its state (such as **Retry**, **Approve** or **Start agent**), the copy items, **Cancel task** and **Delete task**. Both ask first. **Cancel task** marks the task cancelled and keeps it in the list. **Delete task** removes the task, its notes and its history.

## Add a task

1. Press `n`, or click the field **Add a task to the backlog (n)**.
2. Type a title.
3. Press `Enter`, or click **Add task**.

The task is created as a draft and selected. The field empties at once, so you can type the next title.

Two lists appear next to the field when they have something to choose:

- **Project**, when you have more than one project.
- **Template**, when you saved a template. Choose **No template** to skip it. Save a template from the **More actions** menu of a task: **Save as template**.

To start work right away, click **Start a task** next to **Add task**, or press `c`. It uses the title you typed, makes a git worktree and a task, and can start an agent in it. See [Workspaces](/docs/concepts/workspaces/).

In the details panel you can edit a task (**Edit**, or `e`), add subtasks, and use the actions for its state, such as **Mark ready**, **Approve** or **Retry**. **Cancel task** and **Delete task**, in the **...** menu, both ask first.

## Filter and search

- **State chips** above the list filter by state. Each chip shows its count. Click a chip again to clear it. In a narrow window the chips fold into one **State** menu.
- **Filter tasks** in the sidebar (press `/`) filters the list and the workspaces. It matches the id, title, agent, branch and labels. `Esc` clears the field.
- When nothing matches, **Clear filter** resets both the search and the chip.

The **Needs you** strip ignores the filter. It says **Not filtered** while a filter is on.

## Command palette

Press `Mod+K` or `F1`, or click **Search or run a command**. Type to search tasks, commands and projects.

| You type | Result |
|---|---|
| Nothing | Needs you, recent workspaces and suggested commands |
| `>` and text | Commands only |
| `#` and digits | Tasks by id |
| Text with no match | **Add a task** or **Start a task** with that text as the title |

Press `Enter` to open a row. `Shift+Enter` adds a task to **Agents**. `Alt+1` to `Alt+9` pick a row by number. `Tab` changes the scope.

Press `Mod+J` for **Jump to a workspace**. It lists workspaces and their extra shells, with the ones that need you first.

The palette also changes the view (**All tasks**, **Agents**, table, board, progress), the theme, the text size, and runs **Add a project** and **Import GitHub issues**. See [Keyboard shortcuts](/docs/reference/shortcuts/) for every key.

## Task states

| State | Meaning |
|---|---|
| Draft | A task in the backlog. |
| Ready | Marked ready to start. |
| Running | An agent works on it. |
| Needs your input | The agent asked a question, or ended its turn without `natamux done`. |
| In review | Work is ready for you to review. |
| Blocked | You or Natamux blocked it. |
| Failed | The agent or a stage failed. |
| Done | Finished. |
| Cancelled | Cancelled. |

See [Tasks](/docs/concepts/tasks/) for how a task moves between states.

## Needs you

A task needs you when it is in Needs your input, In review, Failed or Blocked state, or when it is running but has shown no sign of life for 10 minutes (it shows as quiet).

- The **Needs you** strip at the top of the sidebar lists these tasks, longest wait first. While tasks wait, the strip has an amber frame and a bell. With none it stays calm. Each row has two lines: the title, then the stage and what the agent asks, or why the task failed, with how long it waits. It shows three rows. **Show all N** lists the rest. When the list is empty it says **Nothing needs you**.
- The title bar shows a chip such as "2 need you". The status bar shows it too. Click either one to open a palette of those tasks.
- The browser tab title shows the count: `(2) Natamux`.

A running task whose agent waits for background work, such as a test run in a background shell, is not quiet and is not listed. See [Waiting for background work](/docs/concepts/agents-and-hooks/#waiting-for-background-work). A task whose workspace was removed leaves the strip.

## Notifications

Click **Notify me** in the status bar to allow system notifications. When a task starts to wait for you while the dashboard is not in front, you get a notification titled with the task id and state, such as `T-0003: Needs input`, with the task title as text. Click it to bring the dashboard forward.

Natamux does not notify about tasks that already waited when you opened the dashboard.

The button is gone once you decide. If you blocked notifications, the status bar says **Notifications are blocked**. Allow them in the site settings of your browser.

## Repositories

Click **Projects** in the status bar, or **Add a project** in the sidebar, to see the projects (git repositories) the daemon manages. To add one:

1. Enter the **Folder**, a full path on the machine that runs the daemon.
2. Optionally enter a **Name**. The folder name is used otherwise.
3. Click **Add project**.

Each project has a **Settings** item in its menu. See [Project settings](#project-settings).

To remove a project, choose **Remove project** in the menu of its group in the sidebar. Remove its workspaces first. Natamux only forgets the repository: the folder, its branches and its tasks stay. Add the folder again with the same name to get its tasks back. Any project can be removed. With no project left, the sidebar offers to add one.

## Project settings

Click **Settings** in the menu of a project to open **Project settings of <repo>**. It has two tabs: **Stages** (the default) and **JSON**. Settings are saved in `.natamux.json` in the repository, so commit the file to share them. For every field of the file, see the [configuration reference](/docs/reference/configuration/).

### The Stages tab

- **Pipeline preview.** A flow chart on top shows the stages as you edit them, with side-by-side stages in one column. A person icon marks a stage that waits for your approval. A pencil marks a prompt of your own. Click a node to select its stage.
- **Stage list.** One row per stage, with a switch (on or off) and a drag handle. Drag a row, or focus it and press `Alt+ArrowUp` or `Alt+ArrowDown`. The row menu has **Move up**, **Move down**, **Run alongside the stage above**, **Run on its own** and **Remove**. A stage that is off stays in the file, but new tasks do not get it.
- **Add stage** opens the catalog, grouped by phase. **Custom stage** makes a stage of your own. The [catalog](/docs/reference/configuration/#stage-catalog) lists every stage.
- **Details.** Click a stage to edit it:

| Field | Shown for | Meaning |
|---|---|---|
| **Name** | Every stage | The name in the dashboard. |
| **Wait for my approval** | Every stage | Shows **Default** or **Changed**. **Use default** goes back. A `review` or `manual` stage is always checked, with "This stage is a check by a person." For `release` the label is **Open the pull request when I click**. |
| **Tasks can run it as a workflow** | Agent stages | Gives each task a **Run with a workflow** checkbox on this stage. On by default for Plan and Implement. Shows **Default** or **Changed**. See [Run a stage as a workflow](/docs/concepts/stages/#run-a-stage-as-a-workflow). |
| **Tasks can check it on CI** | Testing stages and agent stages without an output file | Gives each task a **Check on CI** checkbox on this stage. On by default for Implement and Automated tests. See [Check a stage on CI](/docs/concepts/stages/#check-a-stage-on-ci). |
| **Agent** | Agent stages beside the stage in Shell 1, such as Test spec, and stages in their own shell, such as Review | The agent this stage starts. **Task's agent** keeps the task's own. |
| **Arguments** | The same stages | Words added to that agent's command line, such as `--model sonnet`. |
| **Prompt** | Agent and testing stages | The chips `{title}`, `{description}`, `{dir}` and `{ref}` insert placeholders. **Insert skill** opens a picker with the groups **Project**, **Plugins** and **Only on this machine**, and inserts `{skill:<name>}`. |
| **Command** | Testing stages | The command to run. The Lint and types and End-to-end tests stages need one. |
| **Output file** | Agent stages | The Markdown file the stage writes. Fixed for a catalog stage. |
| **Required sections** | Agent stages with an output file | Headings the file must have. |
| **Finished when** | Every stage | The sentence the progress view shows before the stage starts. |

- **Problems.** Each problem shows next to its stage, and Save is off until they are fixed. The rules are in [Rules for the pipeline](/docs/reference/configuration/#rules-for-the-pipeline).
- **Footer.** **Reset to default** puts the default pipeline in the editor. **Cancel** closes the screen. **Save** (`Mod+Enter`) writes the file. A line above them says: "New tasks use this pipeline. Tasks on their way keep their stages, but use the new prompts and approvals."
- **Phone.** Below 640 px the list and the details are one column. A stage opens in place of the list, with **Back**.

Save changes only `stages`. Other fields and keys it does not know stay in the file. When the stages equal the default pipeline, Save leaves `stages` out of the file.

### The JSON tab

The **JSON** tab edits the whole file as text. Edits move between the tabs. JSON that is not valid keeps you on this tab with the error shown, until you fix it. Use it for fields the **Stages** tab does not show, such as `requireCommit`, `requireClean` and `check`, and for `context`, `tracker` and `agents`.

### The file changed

If the file changed on disk since you opened it (another editor, a pull), Natamux says "The file changed on disk since you opened it." **Reload** reads the file again and drops your edits. **Overwrite** saves your version over it. Closing the screen with edits asks "Discard your changes?".

If the file is not valid, the screen shows the error and the default settings, and Save asks before it replaces the file.

## Import issues

Click **Import issues** in the status bar to turn open GitHub issues into draft tasks. See [GitHub](/docs/guides/github/).

## Account menu

The account icon at the right of the status bar opens a menu.

- **Theme**: System, Light or Dark.
- **UI size**: Compact, Default or Large.
- **Keyboard shortcuts**, also on `?`.
- **Sign out**. Natamux asks first. Your tasks keep running.

## Sign in

The first time, the dashboard asks for the **Daemon token**. Paste the token the daemon printed when it started. It stays in this browser. Sign out removes it.
