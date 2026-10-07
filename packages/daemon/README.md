# @helmsman/daemon

Background process that owns all task state. Clients talk to it over HTTP and WebSocket.

Run in a codespace: `pnpm --filter @helmsman/daemon dev`. Build `@helmsman/core` first (`pnpm build`).

## Settings (environment variables)

| Variable | Default | Meaning |
|---|---|---|
| `HELMSMAN_PORT` | `4317` | Port, bound to `127.0.0.1` only |
| `HELMSMAN_TOKEN` | random per run, printed at start | Required on every request |
| `HELMSMAN_DATA_DIR` | `<cwd>/.helmsman` | Task files and the SQLite index |

## Storage

- Tasks are markdown files with YAML front matter: `<data dir>/tasks/T-0001.md`. They are the source of truth.
- `<data dir>/index.sqlite` is a cache of the task files plus the event log. It is rebuilt from the files at start. The event log is not rebuilt.

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
| `GET /events` (WebSocket) | Live task changes. Use `?token=<token>` because browsers cannot set headers |

An event that does not fit the task's current state is recorded with `accepted: false` and does not change the task.
