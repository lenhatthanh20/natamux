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

## Main endpoints

| Method and path | Meaning |
|---|---|
| `GET /tasks?state=<state>` | List tasks |
| `POST /tasks` | Create a task (`title` is required) |
| `GET /tasks/:id` | One task |
| `DELETE /tasks/:id` | Delete the task, its notes and its history. A workspace is removed too. Answers 409 when that workspace has changes that are not committed, and the task stays |
| `PATCH /tasks/:id` | Change `title`, `description`, `priority`, `labels` or `dependsOn` |
| `GET /tasks/:id/events` | The event log of a task |
| `POST /tasks/:id/events` | Send an event `{ source, type, payload? }`. Answers `{ task, accepted }` |
| `POST /tasks/:id/stage` | A stage action `{ action, stage?, terminal?, to?, reason? }`. Actions: `approve`, `reopen`, `skip`, `pass`, `fail`, `report`, `start` |
| `POST /tasks/:id/workspace` | Create the worktree and branch. Body `{ base? }` |
| `DELETE /tasks/:id/workspace?deleteBranch=true` | Remove the worktree |
| `POST /tasks/:id/agent` | Start an agent `{ agent, prompt?, base? }` |
| `GET /tasks/:id/changes` | Files changed in the workspace |
| `POST /workspaces` | Make a task with a worktree and optionally start an agent |
| `POST /hooks/:agent` | Where an agent's hook posts. Always answers `200 {}` for a known agent |
| `GET /repos` | Projects Natamux manages |
| `POST /repos` | Add a project `{ path, name? }` |
| `DELETE /repos/:id` | Forget a project. Any project can be removed. Answers 409 while it has workspaces |
| `GET /repos/:id/settings` | The settings of a project: `settings`, `error`, `path`, `text` (the text of `.natamux.json`, or `null` without a file) and `version` (`none` without a file) |
| `PUT /repos/:id/settings` | Save the settings to `.natamux.json`. Send the version you read in the header `If-Match: <version>`. Answers `{ settings, version }`. Answers 409 `conflict` when the file changed since that version. Keys of the file that Natamux does not know are kept |
| `GET /repos/:id/skills` | The skills of every agent for the project: `{ skills, notices }`. A skill is `{ name, description, origin, agents, path }`, and `origin` is `project`, `plugin` or `user`. A notice says, for example, that a plugin is on but not installed |

The task is named in a hook by the `X-Natamux-Task` header. The `X-Natamux-Terminal` header names the terminal.

## Live updates and terminals

A browser cannot set headers on a WebSocket, so:

- `GET /events` (WebSocket) streams live task changes, including `task.deleted` when a task is deleted. It also sends a `repos.changed` message when a project is added or removed, so a dashboard can read `GET /repos` again. Pass the token as `?token=<token>`.
- Terminals open with a one-time ticket. Call `POST /tasks/:id/terminal/ticket` to get `{ ticket }`, then open `GET /tasks/:id/terminal?ticket=...` (WebSocket) within 30 seconds.

## The full table

The daemon README lists every endpoint with its body and errors: [packages/daemon/README.md](https://github.com/lenhatthanh20/natamux/blob/main/packages/daemon/README.md).
