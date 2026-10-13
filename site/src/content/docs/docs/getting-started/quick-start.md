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
| Context from | Where the context comes from: **My notes**, **A GitHub issue**, or **An agent skill** when an agent has a skill. It shows when there is more than one source |
| Skill | With **An agent skill**. Click **Pick a skill** and choose the skill that reads the context |
| Notes | Optional, with **My notes**. What to change and why, where to look, and how to tell it is done |
| Reference | With any other source. The issue, ticket or record to read, for example `#42` |
| Attachments | Optional. Screenshots, logs or specs. Click **Add files**, drop them, or paste a screenshot |

**Workspace and agent** is where and how the task runs.

| Field | What to enter |
|---|---|
| Project | The project to work in |
| Branch from | The branch to start from. It defaults to the current branch |
| Agent | Pick an agent, or "No agent, a plain shell" |

Click **Start task**. Natamux makes a task, a git worktree and a branch, and starts the agent in the task's terminal.

<figure class="shot">
<img class="shot-dark" src="/screens/ui/start-task-dark.webp" alt="The Start a task dialog. The Context group has a title, Context from set to My notes, notes and attachments. The Workspace and agent group has the project, the branch to start from and the agent." width="1600" height="1000" loading="lazy" decoding="async" />
<img class="shot-light" src="/screens/ui/start-task-light.webp" alt="The Start a task dialog. The Context group has a title, Context from set to My notes, notes and attachments. The Workspace and agent group has the project, the branch to start from and the agent." width="1600" height="1000" loading="lazy" decoding="async" />
<figcaption>The Start a task dialog: what the agent reads, and where and how the task runs.</figcaption>
</figure>

If you added a task to the backlog first, open it and click **Start agent** (or **Start task** with no agent if you only want a shell).

## 3. Watch the agent work

Open the task. The **Terminal** tab shows the agent running in the task's worktree.

<figure class="shot">
<img class="shot-dark" src="/screens/ui/task-open-dark.webp" alt="An open task on the Terminal tab with agent shells, the shells listed under the task in the sidebar, and the stages in the Progress panel on the right." width="1600" height="1000" loading="lazy" decoding="async" />
<img class="shot-light" src="/screens/ui/task-open-light.webp" alt="An open task on the Terminal tab with agent shells, the shells listed under the task in the sidebar, and the stages in the Progress panel on the right." width="1600" height="1000" loading="lazy" decoding="async" />
<figcaption>An open task: its shells in the middle, its stages on the right.</figcaption>
</figure>

On the dashboard, the state of the task updates by itself, because Natamux's hooks tell it when the agent works, asks or finishes. You can close the dashboard and the agent keeps running.

## 4. Answer when it needs you

When an agent asks a question or waits for a permission, the task moves to **Needs your input**. Its row in the sidebar becomes a card with the agent's question, and the title bar shows a count such as "1 needs you".

1. Click the task in the sidebar.
2. Click **Answer in terminal** on the banner.
3. Type your answer in the terminal.

The task goes back to running. See [Dashboard](/docs/guides/dashboard/).

## 5. Review the stage

Each task moves through stages. The **Progress** view of the details panel shows them and the evidence of each one, beside the terminal.

<figure class="shot">
<img src="/screens/ui/task-spec-approve-dark.webp" alt="An open task at the Spec stage. A banner under the header says Ready for review: Spec, with Approve and Send back, and the Progress panel shows the Spec stage card with its Approve button." width="1600" height="1000" loading="lazy" decoding="async" />
<figcaption>Spec is ready for review. Approve it from the banner or from the stage card.</figcaption>
</figure>

When a stage waits for you, the task is in review. Read what the agent produced, then:

- **Approve** moves on to the next stage.
- **Send back** returns the task to an earlier stage, with a note about what should change.
- **Skip** leaves the stage out.

With the default pipeline you approve three times: the spec, the plan and the test spec.

- After you approve the plan, two agents work at once: the task's agent implements in Shell 1, and a second agent writes the test spec in Shell 2.
- When both are done, the tests run, then a review agent reads the changes in Shell 5.
- If the review finds problems, it sends the task back to Implement by itself, at most two times.

See [The review loop](/docs/concepts/review-loop/) and [Stages](/docs/concepts/stages/).

## 6. Look at the changes

Open the **Changes** tab. It lists the changed files and shows a diff for each one, side by side or inline. The **History** tab shows the task's branch as a graph.

<figure class="shot">
<img class="shot-dark" src="/screens/ui/changes-generate-dark.webp" alt="The Changes tab: the changed files on the left, a side by side diff on the right, and under the list the commit message box filled by Generate, with Commit all." width="1600" height="1000" loading="lazy" decoding="async" />
<img class="shot-light" src="/screens/ui/changes-generate-light.webp" alt="The Changes tab: the changed files on the left, a side by side diff on the right, and under the list the commit message box filled by Generate, with Commit all." width="1600" height="1000" loading="lazy" decoding="async" />
<figcaption>The Changes tab, with a commit message drafted by Generate.</figcaption>
</figure>

## 7. Commit, push and open a pull request

1. In the **Changes** tab, write a commit message, or click **Generate** to let the agent draft one. Then click **Commit all**. It commits every change, new files included.
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
