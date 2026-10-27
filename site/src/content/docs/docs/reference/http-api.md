---
title: HTTP API
description: A short overview of the daemon's HTTP API, for scripting.
sidebar:
  order: 5
---

:::caution
The API may change before version 1.0.
:::

The daemon owns all task state. The dashboard, the `natamux` command and the agents' hooks all use this API.

## Base URL and auth

- Base URL: `http://127.0.0.1:4317` by default (`NATAMUX_PORT` changes the port). When the daemon serves the dashboard (the desktop app, or Docker), the API is also under `/api` on the dashboard's address.
- Send the token in a header on every request:

  ```sh
  curl -H "Authorization: Bearer $NATAMUX_TOKEN" http://127.0.0.1:4317/tasks
  ```

- `GET /health` needs no token and answers `{"status":"ok"}`.
- A wrong or missing token gets 401.

### The agent token

Task terminals get a second token, the agent token, in `NATAMUX_TOKEN`. The daemon makes a new one at each start and never prints it or writes it to disk. Agents and the `natamux` command in a task terminal use it, so the history shows what an agent did.

With the agent token:

- `POST /tasks/:id/events` records `source: agent`, whatever `source` the request names.
- Stage actions record `source: agent`.
- These answer 403 `forbidden` and change nothing:
  - `POST /tasks/:id/events` with `review.approved`, `stage.approved`, `stage.passed`, `stage.skipped` or `state.overridden`
  - `POST /tasks/:id/stage` with `approve`, `skip` or `pass`
  - `PUT /tasks/:id/ci`, `PUT /tasks/:id/skip`, `PUT /tasks/:id/workflow`
  - `PUT /repos/:id/settings`

To do these from a script, use the daemon's own token, not the one in a task terminal.

## Main endpoints

| Method and path | Meaning |
|---|---|
| `GET /tasks?state=<state>` | List tasks |
| `POST /tasks` | Create a task (`title` is required) |
| `GET /tasks/:id` | One task |
| `DELETE /tasks/:id` | Delete the task, its notes and its history. A workspace and its branch are removed too, also when the branch has commits that are not merged. Answers 409 when that workspace has changes that are not committed, and the task, the worktree and the branch stay |
| `PATCH /tasks/:id` | Change `title`, `description`, `priority`, `labels` or `dependsOn` |
| `GET /tasks/:id/events` | The event log of a task |
| `POST /tasks/:id/events` | Send an event `{ source, type, payload? }`. Answers `{ task, accepted }` |
| `POST /tasks/:id/stage` | A stage action `{ action, stage?, terminal?, to?, reason? }`. Actions: `approve`, `reopen`, `skip`, `pass`, `fail`, `report`, `start` |
| `PUT /tasks/:id/ci` | Whether a stage waits for CI before it passes `{ stage, on }`. Answers 400 when the stage does not offer it |
| `PUT /tasks/:id/skip` | Whether a stage not reached yet is skipped when the task reaches it `{ stage, on }`. Answers 400 when marking the current stage or an earlier one or the context stage, and for any change on a task that is over |
| `PUT /tasks/:id/workflow` | Whether a stage runs as a workflow the next time it starts `{ stage, on }`. Answers 400 when the stage does not offer it or the agent is not Claude Code |
| `POST /tasks/:id/workspace` | Create the worktree and branch. Body `{ base? }` |
| `DELETE /tasks/:id/workspace?deleteBranch=true` | Remove the worktree |
| `POST /tasks/:id/agent` | Start an agent `{ agent, prompt?, base? }` |
| `POST /tasks/:id/agent/reopen` | Start the agent of Terminal 1 of a done or cancelled task again, on its last session: `{ task }`. The task keeps its state. Answers 409 when the task is not finished, has no workspace or no agent, or an agent runs in Terminal 1 |
| `GET /tasks/:id/changes` | Files changed in the workspace. The answer is kept for 5 seconds; `?fresh=1` reads git now |
| `GET /tasks/:id/graph` | The history of the task branch next to the base branch. Kept and `?fresh=1` as for changes |
| `POST /tasks/:id/commit-message` | A commit message for the changes, by the task's agent: `{ message, source, agent, reason? }`. `source` is `draft` when the agent failed and the message is a simple one; `reason` says why. Answers 409 when nothing changed |
| `POST /workspaces` | Make a task with a worktree and optionally start an agent. Answers 202 with the task as soon as it is made; the worktree and the agent come after (see `workspace.preparing`) |
| `GET /workspaces/preparing` | The workspaces being made, and the step of each: `{ preparing: { "T-0001": "checkout" } }` |
| `POST /hooks/:agent` | Where an agent's hook posts. Always answers `200 {}` for a known agent |
| `GET /repos` | Projects Natamux manages, and the daemon's worktree folder: `{ repos, worktreesDir }`. A project with its own worktree folder has `worktreesDir` |
| `POST /repos` | Add a project `{ path, name?, worktreesDir? }`. `worktreesDir` is an absolute folder for the project's worktrees; it is made when missing. Answers 400 when it is not absolute or cannot be made |
| `DELETE /repos/:id` | Forget a project. Any project can be removed. Answers 409 while it has workspaces |
| `GET /repos/:id/settings` | The settings of a project: `settings`, `error`, `path`, `text` (the text of `.natamux.json`, or `null` without a file) and `version` (`none` without a file) |
| `PUT /repos/:id/settings` | Save the settings to `.natamux.json`. Send the version you read in the header `If-Match: <version>`. Answers `{ settings, version }`. Answers 409 `conflict` when the file changed since that version. Keys of the file that Natamux does not know are kept |
| `GET /repos/:id/skills` | The skills of every agent for the project: `{ skills, notices }`. A skill is `{ name, description, origin, agents, path }`, and `origin` is `project`, `plugin` or `user`. A notice says, for example, that a plugin is on but not installed |

The task is named in a hook by the `X-Natamux-Task` header. The `X-Natamux-Terminal` header names the terminal.

## More endpoints

Files, git, GitHub, terminals and templates.

| Method and path | Meaning |
|---|---|
| `GET /tasks/:id/files` | The stage files of a task: `{ files }` |
| `GET /tasks/:id/files/:name` | One stage file, such as `spec.md`: `{ name, text }`. Answers 404 when it does not exist |
| `PUT /tasks/:id/files/:name` | Replace a stage file `{ text }`, for example to fix the spec before you approve it |
| `GET /tasks/:id/attachments` | The files attached to a task: `{ attachments }` |
| `GET /tasks/:id/attachments/:name` | One attachment. Images come with their type, any other file downloads |
| `GET /tasks/:id/ended-shells` | The last screens of the terminals of a done or cancelled task: `{ shells }`. Each has `name`, `agent`, `stage`, `session` (the agent's last session, or null), `endedAt` and `text` |
| `GET /tasks/:id/changes/file?path=<file>` | The diff of one changed file |
| `GET /tasks/:id/changes/image?path=<file>&side=original\|modified` | One side of a changed image |
| `GET /tasks/:id/tree?path=<folder>` | One folder of the worktree: `{ entries }`. No `path` is the root. `.git` is never listed |
| `GET /tasks/:id/file?path=<file>` | One file of the worktree: `{ path, text, binary, tooLarge, version }` |
| `PUT /tasks/:id/file?path=<file>` | Save a file that exists `{ text }`, with the `version` you read in `If-Match`. Answers 409 when the file changed since |
| `POST /tasks/:id/agent/message` | Paste a comment into the agent's terminal `{ text }`. Answers 400 when no agent runs there |
| `GET /tasks/:id/review-comments` | The review comments of the task: `{ comments }`, by file then line. Each has `id` (`c1`, `c2`, …), `path`, `side` (`old` or `new`), `start` and `end` (1-based lines; both 0 for the whole file), `quote`, `body`, `state` (`draft`, `sent` or `resolved`), `createdAt`, `sentAt`, `resolvedAt` and `outdated` (true when the quoted code is gone). Each comment follows its code when the file changes |
| `POST /tasks/:id/review-comments` | Add a draft `{ path, side, start, end, body }`: `{ comment }`, status 201. `body` is 1 to 5000 characters. Answers 400 for lines outside the file or `end` before `start` |
| `PATCH /tasks/:id/review-comments/:cid` | Change a comment `{ body?, state? }`: `{ comment }`. `body` only while it is a draft. `state` is `resolved` (from `sent`) or `sent` (from `resolved`). Answers 409 otherwise, 404 for an unknown id |
| `DELETE /tasks/:id/review-comments/:cid` | Delete a comment in any state. Answers 204 |
| `POST /tasks/:id/review-comments/send` | Write `.natamux-task/review-comments.md`, paste one prompt into the agent's terminal and mark every draft `sent`: `{ task, comments }`. Answers 409 when there is no draft or no agent runs there |
| `POST /tasks/:id/commit` | Commit all changes `{ message }` |
| `POST /tasks/:id/rollback` | Put changed files back as they are in the last commit `{ paths }` (1 to 5000): `{ files }`. A new file is deleted. Answers 404 for a path that is not changed, and 409 for a conflict; then nothing changes |
| `POST /tasks/:id/rollback/undo` | Bring back the files of the last rollback as they were before it: `{ files }`. Answers 409 when there is nothing to undo |
| `POST /tasks/:id/push` | Push the task branch |
| `POST /tasks/:id/pull-request` | Open the pull request: `{ task }` |
| `POST /tasks/:id/pull-request/refresh` | Ask GitHub now for the state of the pull request, instead of waiting for the next check |
| `POST /tasks/:id/issue` | Open a GitHub issue for the task now |
| `PUT /tasks/:id/issue` | Link an issue that exists `{ provider, key, url }` |
| `POST /tasks/:id/fan-out` | Run more tries of a task `{ count, start? }`. `count` is 1 to 5 |
| `GET /tasks/:id/terminals` | The open terminals of a task: `{ terminals, agents, lanes, programs, restoring }` |
| `DELETE /tasks/:id/terminals/:terminal` | Close one terminal. `:terminal` is its number |
| `POST /tasks/:id/terminal/ticket` | A one-time ticket for the terminal WebSocket |
| `GET /templates` | The saved task templates: `{ templates }` |
| `POST /templates` | Save a template |
| `POST /templates/:id/tasks` | Make a task from a template |
| `GET /github/issues?repo=<id>` | The open issues of a project: `{ issues }` |
| `POST /github/issues/import` | Import issues as draft tasks `{ numbers, repo? }`. Up to 100 numbers |
| `GET /git/branches?repo=<id>` | The branches of a project, for **Branch from** |
| `GET /git/remote-branches?repo=<id>` | The branches of `origin` as `{ branches: ["origin/<name>"] }`, also ones never fetched. Asks `origin` over the network; no `origin` remote gives an empty list |
| `GET /activity` | What the agent of each task does now, from its last hook |
| `GET /usage` | Tokens and estimated cost of each task: `{ usage: { "<taskId>": { tokens, costUsd, unpricedModels, stages: [{ stage, agent, model, input, cacheRead, cacheWrite, output, costUsd }] } } }`. `costUsd` is an estimate at API list prices, `null` when no row has a price. `stage` is `""` for work on no stage. Answers before the login `PATH` |
| `GET /jobs` | The background jobs that run now, oldest first: `{ jobs: [{ id, kind, label, taskId, startedAt }] }` |
| `GET /diffstats` | The size of the changes of every workspace |

## Live updates and terminals

A browser cannot set headers on a WebSocket, so:

- `GET /events` (WebSocket) streams live task changes, including `task.deleted` when a task is deleted. Pass the token as `?token=<token>`. Each message is JSON with a `kind`. Besides the task messages it sends:
  - `repos.changed` when a project is added or removed, so a dashboard can read `GET /repos` again.
  - `activity.changed` with `{ taskId, activity }` when what an agent does changes. `activity` is the task's whole activity by terminal, as `GET /activity` gives it; `{}` means none.
  - `usage.changed` with `{ taskId, usage }` when a task's usage changes. `usage` is the task's entry, as `GET /usage` gives it.
  - `shells.changed` with `{ taskId }` when a terminal of the task starts or ends, when the program in front of a terminal changes (checked about every 500 ms while it prints, and when its output goes quiet), when an agent that comes back after a restart starts or is up, or when a hook changes the state of an agent that works on no stage, unless the last `GET /tasks/<id>/terminals` found no agent in that terminal.
  - `workspace.dirty` with `{ taskId }` when files in the task's workspace may have changed: after an agent hook for a tool that ran or for the end of a turn, when its terminals go quiet, after a commit or a rollback, or after a save in the editor. Each comes 750 ms after a change. Up to 3 in a row come that soon; while changes go on after that, at most one per task every 10 s, and one more after the last change.
  - `review-comments.changed` with `{ taskId }` after a review comment is added, changed or deleted, a send, or when a file edit moved a comment. Read `GET /tasks/<id>/review-comments` again.
  - `workspace.preparing` with `{ taskId, step }` while a new workspace is made: `step` is `fetch` or `checkout`, then `null` when it ended. A failure also has `error`.
  - `jobs.changed` with `{ jobs }` when a background job starts or ends. `jobs` is the whole list, as `GET /jobs` gives it.
  - `resync` to one socket that missed messages because it read too slowly. The dashboard loads everything again.
- Terminals open with a one-time ticket. Call `POST /tasks/:id/terminal/ticket` to get `{ ticket }`, then open `GET /tasks/:id/terminal?ticket=...` (WebSocket) within 30 seconds.

## The full table

A request with a bad field answers 400 with an error that names the field and the reason, such as which field is too long.

The Rust daemon routes define each endpoint and its errors in [crates/natamux-daemon/src/http/routes](https://github.com/lenhatthanh20/natamux/tree/main/crates/natamux-daemon/src/http/routes).
