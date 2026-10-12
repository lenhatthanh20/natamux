---
title: Quick start
description: Go from first launch to a pull request with one agent working on one task.
sidebar:
  order: 2
---

This walk-through uses the desktop app and one agent. It takes about ten minutes. You need the [requirements](/docs/getting-started/install/#requirements) and a git repository you can experiment in.

## 1. Add a project

Open Natamux. The app starts its daemon and opens the dashboard, already signed in. On the first launch there is no project yet, and the sidebar says **No projects yet**.

Add the repository whose tasks Natamux runs. Use **File → Add Project…** (`Cmd+Shift+O` on macOS, `Ctrl+Shift+O` on Linux) and pick the top folder of the repository, the one that holds `.git`. Natamux warns you if the folder is not a git repository. You can also click **Add project** in the sidebar and type the folder.

Add more projects the same way, at any time.

:::note
Running the dashboard in a browser instead? Open it and paste the token into the **Daemon token** field. It is `NATAMUX_TOKEN` of the daemon, or the token the daemon printed when it started. The browser keeps it for this address only.
:::

## 2. Add a task

You have two ways to start.

- **Add a task** puts an idea in the backlog. Type a title in the field "Add a task to the backlog" (press `n`) and press Enter.
- **Start a task** starts work right away. Click the **+** button in the title bar, or press `c`.

For this guide, use **Start a task**. The dialog has two groups.

**Context** is what the agent reads as the task's context. The line under the heading says what that is for the source you pick.

| Field | What to enter |
|---|---|
| Title | A short title, for example `Fix the login redirect` |
| Context from | Where the context comes from. It shows when the project offers more than one source |
| Notes | Optional, with **My notes**. What to change and why, where to look, and how to tell it is done |
| Reference | With any other source. The issue or ticket to read, for example `#42` |
| Attachments | Optional. Screenshots, logs or specs. Click **Add files**, drop them, or paste a screenshot |

**Workspace and agent** is where and how the task runs.

| Field | What to enter |
|---|---|
| Project | The project to work in |
| Branch from | The branch to start from. It defaults to the current branch |
| Agent | Pick an agent, or "No agent, a plain shell" |

Click **Start task**. Natamux makes a task, a git worktree and a branch, and starts the agent in the task's terminal.

![The Start a task dialog with a title, notes, a branch and an agent](/screens/new-workspace.png)

If you added a task to the backlog first, open it and click **Start agent** (or **Start task** with no agent if you only want a shell).

## 3. Watch the agent work

Open the task. The **Terminal** tab shows the agent running in the task's worktree.

![A task's terminal with an agent working](/screens/terminal.png)

On the dashboard, the state of the task updates by itself, because Natamux's hooks tell it when the agent works, asks or finishes. You can close the dashboard and the agent keeps running.

## 4. Answer when it needs you

When an agent asks a question or waits for a permission, the task moves to **Needs your input**. The **Needs you** strip at the top of the dashboard names it, and the title bar shows a count such as "1 needs you".

1. Click the task in the strip.
2. Click **Answer in terminal** on the banner.
3. Type your answer in the terminal.

The task goes back to running. See [Dashboard](/docs/guides/dashboard/).

## 5. Review the stage

Each task moves through stages. The **Progress** view of the details panel shows them and the evidence of each one, beside the terminal.

![The Progress view of tasks against their stages](/screens/progress.png)

When a stage waits for you, the task is in review. Read what the agent produced, then:

- **Approve** moves on to the next stage.
- **Send back** returns the task to an earlier stage, with a note about what should change.
- **Skip** leaves the stage out.

See [Stages](/docs/concepts/stages/).

## 6. Look at the changes

Open the **Changes** tab. It lists the changed files and shows a diff for each one, side by side or inline. The **History** tab shows the task's branch as a graph.

![The Changes tab with a file tree and a diff](/screens/changes.png)

## 7. Commit, push and open a pull request

1. In the **Changes** tab, write a commit message and click **Commit all**. It commits every change, new files included.
2. Click **Push** in the task header. The button shows how many commits are ahead, for example **Push ↑2**.
3. Click **Create pull request**. It needs the `gh` command, logged in. When the pull request exists, the button becomes **Open pull request**.

Natamux never pushes or merges unless you click the button or run the command.

## 8. Finish

Merge the pull request on GitHub. Natamux checks pull requests of open tasks once a minute, and when one is merged it moves a task in review to done.

To clean up, open the task and remove its workspace. Natamux asks first and refuses if the worktree has changes that are not committed.

## Next

- [Tasks](/docs/concepts/tasks/) and [Workspaces](/docs/concepts/workspaces/)
- [Desktop app](/docs/guides/desktop-app/)
- [Troubleshooting](/docs/help/troubleshooting/)
