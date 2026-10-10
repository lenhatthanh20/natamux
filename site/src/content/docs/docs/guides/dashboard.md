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
| Title bar | The sidebar toggle, where you are, **Search or run a command**, the "N need you" chip, **New workspace** and the details toggle |
| Sidebar | A filter field, **All tasks**, **Agents**, **Needs you**, and **Workspaces** grouped by repository |
| Main area | The task list as a table, board or progress view, or the open workspace |
| Details panel | The selected task: notes, workspace, pull request, subtasks, tries |
| Status bar | The connection state, running count, **Repos**, **Import issues**, **Notify me**, shortcuts and the account menu |

Under 768 px wide the status bar is hidden and the sidebar becomes a drawer. The status items move to the foot of the drawer.

Press `Mod+B` to hide or show the sidebar. Press `Mod+Alt+B` to hide or show the details. `Mod` is ⌘ on macOS and Ctrl elsewhere. Press `F6` to move between the parts. Drag the edge of the sidebar or the details panel to resize it.

## Sidebar

- **All tasks** shows the task list in the main area. The number is every task.
- **Agents** opens a grid of agent terminals. It starts with the agents that work or wait. Use **Add a pane** to add a task (up to 6 panes), and **Columns** to pick 1, 2, 4 or Auto.
- **Needs you** lists the tasks that wait for you. See [Needs you](#needs-you).
- **Workspaces** lists workspaces in folders, one per repository. A workspace row shows its state, title, age, branch, the size of its diff, and what the agent does now or asks.

In the workspace list:

- Click a repository to fold or unfold it. Click **+** next to it for **New workspace in** that repository. **Add a repository** and **Add a workspace** are in the section header.
- Finished workspaces (done or cancelled) are folded under "N finished".
- Double-click a title, or press `F2`, to rename it.
- Right-click a row, or press `Shift+F10`, for **View changes**, **Rename**, **Pin to the top**, **Copy branch name**, **Copy folder path** and **Remove workspace**. Pinned workspaces move to a **Pinned** section at the top.
- Press `Delete` on a row to remove the workspace. Natamux asks first. The task stays. The branch stays when it has commits that are not merged.

## Task views

Pick a view with the **Layout** switch above the list.

| View | Shows |
|---|---|
| Table | One row per task. Click a column header to sort. |
| Board | One column per state. **Show finished** adds the done and cancelled columns. |
| Progress | Tasks against their [stages](/docs/concepts/stages/) |

![The board view](/screens/board.png)

![The progress view](/screens/progress.png)

The table columns are Id, Task, State, Agent, Branch, Changes, PR and Last activity. A narrow window shows fewer columns. Below 560 px wide each task is a card. Changes and PR appear only when some task has a value.

The default order puts what needs you first: Needs input, In review, Failed, Blocked, Running, Ready, Draft, Done, Cancelled.

Click a row to select it and open its details. Double-click, or press `Enter`, to open its workspace. The workspace opens on the terminal while an agent works, and otherwise on the tab you used last.

## Add a task

1. Press `n`, or click the field **Add a task to the backlog (n)**.
2. Type a title.
3. Press `Enter`, or click **Add task**.

The task is created as a draft and selected. The field empties at once, so you can type the next title.

Two lists appear next to the field when they have something to choose:

- **Repository**, when you have more than one repository.
- **Template**, when you saved a template. Choose **No template** to skip it. Save a template from the **More actions** menu of a task: **Save as template**.

To start work right away, press `c` for **New workspace**. It makes a git worktree and a task, and can start an agent in it. See [Workspaces](/docs/concepts/workspaces/).

In the details panel you can edit a task (**Edit**, or `e`), add subtasks, and use the actions for its state, such as **Mark ready**, **Approve** or **Retry**. **Cancel task** asks first.

## Filter and search

- **State chips** above the list filter by state. Each chip shows its count. Click a chip again to clear it. In a narrow window the chips fold into one **State** menu.
- **Filter tasks** in the sidebar (press `/`) filters the list and the workspaces. It matches the id, title, agent, branch and labels. `Esc` clears the field.
- When nothing matches, **Clear filter** resets both the search and the chip.

The **Needs you** strip ignores the filter. It says **Not filtered** while a filter is on.

## Command palette

Press `Mod+K` or `F1`, or click **Search or run a command**. Type to search tasks, commands and repositories.

| You type | Result |
|---|---|
| Nothing | Needs you, recent workspaces and suggested commands |
| `>` and text | Commands only |
| `#` and digits | Tasks by id |
| Text with no match | **Add a task** or **New workspace** with that text as the title |

Press `Enter` to open a row. `Shift+Enter` adds a task to **Agents**. `Alt+1` to `Alt+9` pick a row by number. `Tab` changes the scope.

Press `Mod+J` for **Jump to a workspace**. It lists workspaces and their extra shells, with the ones that need you first.

The palette also changes the view (**All tasks**, **Agents**, table, board, progress), the theme, the text size, and runs **Add a repository** and **Import GitHub issues**. See [Keyboard shortcuts](/docs/reference/shortcuts/) for every key.

## Task states

| State | Meaning |
|---|---|
| Draft | A task in the backlog. |
| Ready | Marked ready to start. |
| Running | An agent works on it. |
| Needs input | The agent asked a question. |
| In review | Work is ready for you to review. |
| Blocked | You or Natamux blocked it. |
| Failed | The agent or a stage failed. |
| Done | Finished. |
| Cancelled | Cancelled. |

See [Tasks](/docs/concepts/tasks/) for how a task moves between states.

## Needs you

A task needs you when it is in Needs input, In review, Failed or Blocked state, or when it is running but has shown no sign of life for 10 minutes (it shows as quiet).

- The **Needs you** strip at the top of the sidebar lists these tasks, longest wait first. It shows three rows. **Show all N** lists the rest. When the list is empty it says **Nothing needs you**.
- The title bar shows a chip such as "2 need you". The status bar shows it too. Click either one to open a palette of those tasks.
- The browser tab title shows the count: `(2) Natamux`.

## Notifications

Click **Notify me** in the status bar to allow system notifications. When a task starts to wait for you while the dashboard is not in front, you get a notification titled with the task id and state, such as `T-0003: Needs input`, with the task title as text. Click it to bring the dashboard forward.

Natamux does not notify about tasks that already waited when you opened the dashboard.

The button is gone once you decide. If you blocked notifications, the status bar says **Notifications are blocked**. Allow them in the site settings of your browser.

## Repositories

Click **Repos** in the status bar, or **Add a repository** in the sidebar, to see the repositories the daemon manages. To add one:

1. Enter the **Folder**, a full path on the machine that runs the daemon.
2. Optionally enter a **Name**. The folder name is used otherwise.
3. Click **Add repository**.

Each repository has a **Settings** button for its stages, context sources and issue tracker.

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
