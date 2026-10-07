# @helmsman/daemon

Background process that owns all task state. Clients talk to it over HTTP and WebSocket.

Run in a codespace: `pnpm --filter @helmsman/daemon dev`. Build `@helmsman/core` first (`pnpm build`).

## Settings (environment variables)

| Variable | Default | Meaning |
|---|---|---|
| `HELMSMAN_PORT` | `4317` | Port, bound to `127.0.0.1` only |
| `HELMSMAN_TOKEN` | random per run, printed at start | Required on every request |
| `HELMSMAN_DATA_DIR` | `<cwd>/.helmsman` | Task files and the SQLite index |
| `HELMSMAN_REPO_DIR` | `<cwd>` | The git repository tasks get worktrees from |
| `HELMSMAN_WORKTREES_DIR` | `<data dir>/worktrees` | Where worktrees are created, one folder per task id |

## Storage

- Tasks are markdown files with YAML front matter: `<data dir>/tasks/T-0001.md`. They are the source of truth.
- `<data dir>/index.sqlite` (Node's built-in `node:sqlite`, Node 22.13 or newer) is a cache of the task files plus the event log. The task cache is rebuilt from the files at start. The event log is not rebuilt.

## API

Send `Authorization: Bearer <token>`. `/health` needs no token.

| Method and path | Meaning |
|---|---|
| `GET /health` | Liveness |
| `GET /tasks?state=<state>` | List tasks, newest update first |
| `POST /tasks` | Create a draft task (`title` required) |
| `GET /tasks/:id` | One task |
| `GET /tasks/:id/events` | Event log of a task |
| `POST /tasks/:id/events` | Send an event `{ source, type, payload? }`. Returns `{ task, accepted }` |
| `POST /tasks/:id/workspace` | Create a git worktree and a branch `helmsman/t-0001-<title>` for the task. Answers 409 if the task has one, is done or cancelled, or git refuses |
| `DELETE /tasks/:id/workspace?deleteBranch=true` | Remove the worktree. Git refuses when it has files that are not committed (409). The branch is deleted only when it is fully merged. Returns `{ task, branchDeleted }` |
| `POST /hooks/claude-code` | Where Claude Code's `http` hooks post. Maps the hook to task events and always answers `200 {}`. The task comes from the `X-Helmsman-Task` header or from the workspace that holds the hook's `cwd`. See `docs/claude-code-adapter.md` |
| `GET /events` (WebSocket) | Live task changes. Use `?token=<token>` because browsers cannot set headers |

An event that does not fit the task's current state is recorded with `accepted: false` and does not change the task.
