---
title: GitHub
description: Import issues as tasks, read an issue as context, open issues and pull requests, and follow pull request checks.
sidebar:
  order: 4
---

Natamux talks to GitHub through the GitHub CLI, `gh`. It has no GitHub login of its own.

## Set up the GitHub CLI

Install `gh` on the machine that runs the daemon, then sign in once:

```sh
gh auth login
```

The daemon runs `gh` in your repository folder. It must find `gh` on its `PATH`.

These features need `gh` signed in:

| Feature | Where |
|---|---|
| Import issues as tasks | **Import issues** |
| Read an issue as task context | Context source `github-issue` |
| Open an issue for a task | **Create issue**, or automatically |
| Create a pull request | **Create PR** |
| Pull request status and checks | Details panel |

**Push** uses git, not `gh` directly. For `github.com` it asks `gh` for credentials when you have no other git credential helper set up.

If `gh` is missing or signed out, Natamux says so. The import dialog shows one of these messages:

- **The GitHub CLI is not installed. Install `gh`, run `gh auth login` in a terminal, then try again.**
- **GitHub CLI is not signed in. Run `gh auth login` in a terminal, then try again.**
- **Natamux could not read the issues from GitHub. Check the connection and try again.**

Click **Retry** after you fix it. **Show details** has the raw error.

## Import issues as tasks

1. Click **Import issues** in the status bar. You can also run **Import GitHub issues** from the command palette.
2. Tick the issues you want. The dialog lists up to 100 open issues of the repository, with their labels.
3. Click **Import N issues**, or press `Mod+Enter`.

Natamux makes one draft task for each issue and says **Imported N issues.** The task has the issue title as its title, the issue text as its notes, and the issue labels as its labels. The notes end with a line such as `Imported from https://github.com/owner/repo/issues/42`.

An issue becomes a task once. Issues that were imported already are ticked and greyed out, with the id of their task. With more than one repository, the import uses the repository selected in the **Repository** list of the task field. See [Dashboard](/docs/guides/dashboard/).

## Use a GitHub issue as task context

A task can read its context from a GitHub issue instead of its own notes. The first stage of the default pipeline fetches the context into `context.md`: the title, the link, the labels, the text and the comments of the issue. See [Stages](/docs/concepts/stages/).

In the dashboard, open **Start a task** (`c`), then pick **A GitHub issue** in **Context from**. Enter `#42` or the address of the issue. This list shows only when the project offers more than one context source, which is the default.

In the CLI:

```sh
natamux task create "Export tasks" --context github-issue:42
```

The reference can be `42`, `#42`, `owner/name#42` or the full issue address. An address of another repository reads the issue from that repository.

When the task has no linked issue yet, Natamux links it to the issue it reads. If it cannot find an issue number, the stage fails with **Give the issue number, such as #42, to read the context from**.

The **Progress** view of the details panel shows the context source and the issue in **Context and issue**.

## Open an issue for a task

Natamux can open a GitHub issue for a task that you made in the app, and link the two. The issue has the task title as its title. Its body is the spec (`spec.md`) when there is one, and otherwise the task notes, plus a line `Made by Natamux for T-0001.`

Natamux opens no issue on its own until you turn it on. Choose when in the **Issues** tab of [Project settings](/docs/guides/project-settings/#the-issues-tab). It writes `tracker.createIssue` in the `.natamux.json` file at the root of the repository.

| Value | Natamux opens the issue |
|---|---|
| `manual` (default) | Only when you click **Create issue** |
| `on-spec-approved` | When you approve the `spec` stage. In a pipeline without a spec stage, when the pull request opens. |
| `on-pr` | When the pull request opens |
| `never` | Never. The **Create issue** button is hidden. |

```json
{
  "tracker": {
    "type": "github",
    "createIssue": "on-spec-approved",
    "linkInPr": true
  }
}
```

To open an issue by hand, open the **Progress** view in the details of the task and click **Create issue** in **Context and issue**. A toast says **Opened issue #N.** The button shows only when the task has no issue yet, the tracker is `github`, and `createIssue` is not `never`. Set `tracker.type` to `none` to turn issues off.

If opening an issue fails in the background, the task stays without an issue and the daemon logs the error. Click **Create issue** to try again.

## Pull requests

**Create PR** in the workspace header pushes the branch and opens a pull request. See [Changes and history](/docs/guides/changes-and-history/).

A pull request that the agent or you open another way, such as with `gh pr create`, is found too. Once origin has the task branch, Natamux looks for an open or merged pull request from that branch when the agent stops, on each status check, and on **Check now**. It then links it to the task, as if **Create PR** had opened it. A closed pull request is not linked.

When the task is linked to an issue and `tracker.linkInPr` is true (the default), the pull request body ends with `Closes #N`. GitHub then closes the issue when the pull request merges. For an issue of another tracker, the line is `Tracked in KEY: address`.

### Checks and status

While a dashboard is open, Natamux asks GitHub about each open or draft pull request once a minute, with one request per repository. Once the checks of all of them have finished, it asks every 5 minutes. The **Pull request** section of the details panel shows:

- The pull request link.
- A summary such as `open, checks pass`, `open, checks fail` or `draft, checks running`.
- **Check now**, to ask at once.

When the pull request is merged and the task is at review or later, the task moves to Done.

## Link a task to Jira or another tracker

Link a task to an issue that is not on GitHub with `natamux task link`:

```sh
natamux task link T-0001 --issue ABC-123 --url https://jira.example.com/browse/ABC-123
```

`--issue` is the key and `--url` is its address. Add `--provider jira` to name the tracker. Without `--provider`, the provider is `github` when the address contains `github.com`, and `tracker` otherwise.

The link shows in **Context and issue** in the **Progress** view of the details panel. With `tracker.linkInPr` on, the pull request body ends with `Tracked in ABC-123: https://…`. Natamux does not read from or write to the other tracker. Jira is not used for context unless you add a `skill` or `mcp` context source. See [CLI reference](/docs/reference/cli/).
