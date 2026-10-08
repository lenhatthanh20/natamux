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
| `PATCH /tasks/:id` | Body: any of `title`, `description`, `priority`, `labels`, `dependsOn`. Other fields answer 400. Logs `task.edited` with the names of the fields that changed; nothing is written when none did |
| `GET /tasks/:id/events` | Event log of a task |
| `POST /tasks/:id/events` | Send an event `{ source, type, payload? }`. Returns `{ task, accepted }` |
| `GET /git/branches` | Local branches of the repository and the one its checkout is on: `{ branches, current }` |
| `POST /tasks/:id/workspace` | Create a git worktree and a branch `helmsman/t-0001-<title>` for the task. Body `{ base? }`: start from that local branch instead of HEAD (409 when there is no such branch). When the worktree has a `.helmsman-setup` script (`.helmsman-setup.ps1` on Windows), it runs in the task's first terminal (`sh ./.helmsman-setup`). Answers 409 if the task has one, is done or cancelled, or git refuses |
| `DELETE /tasks/:id/workspace?deleteBranch=true` | Remove the worktree. Git refuses when it has files that are not committed (409). The branch is deleted only when it is fully merged. Returns `{ task, branchDeleted }` |
| `GET /tasks/:id/changes` | Files changed in the workspace compared with its last commit, like VS Code Source Control. Returns `{ changes: [{ path, status, originalPath }] }`. Status is `M`, `A`, `D`, `R`, `U` (new file) or `C` (conflict). Answers 409 when the task has no workspace |
| `GET /tasks/:id/changes/file?path=` | Both sides of one changed file: `{ path, status, original, modified, binary, tooLarge }`. The path must be one of the changes (404 otherwise). A binary file or one over 1 MB comes without text |
| `GET /tasks/:id/graph` | History of the task branch next to the branch that the main checkout is on, newest first, in topological order (at most 200 commits). Returns `{ branch, head, base, ahead, behind, commits: [{ hash, parents, author, date, subject, refs, onBranch }], truncated }`. `base`, `ahead` and `behind` are null when the main checkout is not on a branch. Answers 409 when the task has no workspace |
| `POST /tasks/:id/agent` | Body `{ agent, prompt? }`. Types the agent's command into the task's terminal, starting the shell when needed. Only `claude-code` today (see `docs/claude-code-adapter.md`). Sets `agent` on the task and logs `agent.launched`. Answers 404 for another agent and 409 without a workspace |
| `POST /tasks/:id/fan-out` | Body `{ count: 1 to 5, start? }`. Copies the task `count` times ("<title> (try 2)", …) with its notes, priority, labels and dependencies. The task and its copies get the label `tries-t-0001`. With `start: true` each copy gets a workspace and Claude Code. Answers 201 with `{ tasks }` |
| `POST /tasks/:id/agent/message` | Body `{ text }`. Pastes the text into the task's first terminal (bracketed paste, then Enter), so the agent there takes it as its next prompt. Logs `review.commented`. Answers 409 when that terminal is not open |
| `POST /tasks/:id/commit` | Body `{ message }`. Commits every change of the workspace, new files included. Returns `{ task, hash }` and logs `git.committed`. Answers 409 when there is nothing to commit |
| `POST /tasks/:id/push` | Pushes the task branch to `origin` and sets its upstream. Logs `git.pushed`. For github.com, git asks `gh auth git-credential` for the password after any helper you have, so `GH_TOKEN` or `gh auth login` is enough |
| `POST /tasks/:id/pull-request` | Pushes, then runs `gh pr create` from the task branch into the default branch, with the task's title and description. Keeps the address in `pr` and logs `pr.opened`. When a pull request for the branch exists, keeps its address. Answers 409 when the task has a pull request already or `gh` fails |
| `POST /tasks/:id/pull-request/refresh` | Reads the task's pull request from GitHub now (`gh pr view`) instead of at the next minute. The daemon does this by itself once a minute for every task with an open pull request. The answer goes into `prStatus` (`{ state: open, draft, merged or closed, checks: passing, failing, pending or none }`). Logs `ci.passed`, `ci.failed`, `pr.closed` and `pr.merged` when they change; `pr.merged` moves a task in review to done |
| `GET /tasks/:id/terminals` | Names of the open terminals of the task: `{ terminals: ['1', '2'] }` |
| `DELETE /tasks/:id/terminals/:terminal` | Ends that shell. Returns the terminals that stay open |
| `POST /tasks/:id/terminal/ticket` | A ticket that opens the terminal socket once, within 30 seconds: `{ ticket }`. A browser cannot set headers on a WebSocket, so this keeps the daemon token out of the socket URL. Answers 409 when the task has no workspace |
| `GET /tasks/:id/terminal?ticket=&cols=&rows=&terminal=` (WebSocket) | A shell in the task's workspace. `terminal` names it (`1`, `2`, …; `1` when left out); the agent runs in `1`. It keeps running when the socket closes; the next socket first gets the last 200,000 characters of output. Frames are JSON: the dashboard sends `{ type: 'input', data }` and `{ type: 'resize', cols, rows }`, the daemon sends `{ type: 'output', data }`, `{ type: 'exit', code }` and `{ type: 'error', message }`. A bad ticket closes the socket with code 4401. The shell gets `HELMSMAN_TASK_ID`, so `helmsman hook` names the task. Removing the workspace ends the shell |
| `GET /templates` | Task templates, sorted by name: `{ templates: [{ id, name, title, description, priority, labels }] }`. Each is a markdown file in `<data dir>/templates` |
| `POST /templates` | Body `{ name, title, description?, priority?, labels? }`. Saves a template; its id comes from the name, and a template with the same name is replaced. Answers 201 |
| `POST /templates/:id/tasks` | Body `{ title }`. A new draft task with the template's notes, priority and labels. Answers 201, or 404 for an unknown template |
| `GET /diffstats` | The size of the work of every task with a workspace: `{ stats: { 'T-0001': { files, added, removed } } }`, counted from where the branch left the branch of the main checkout, uncommitted changes included. Files git does not track yet are not counted |
| `GET /github/issues` | Open issues of the repository, from `gh issue list` (at most 100): `{ issues: [{ number, title, body, url, labels, taskId }] }`. `taskId` names the task made from the issue, or is null |
| `POST /github/issues/import` | Body `{ numbers }`. Makes a draft task from each open issue that was not imported yet: its title, its body plus `Imported from <url>` as notes, its labels. Returns `{ tasks }` |
| `POST /hooks/:agent` | Where an agent posts its hook, for example `/hooks/claude-code`. The adapter of that agent turns the hook into task events. Always answers `200 {}` (even for a hook that matches no task or changes nothing), `404` for an agent without an adapter, `400` for input that is not a hook of that agent. The task comes from the `X-Helmsman-Task` header or from the workspace that holds the hook's `cwd`. See `docs/agent-adapters.md` and `docs/claude-code-adapter.md` |
| `GET /events` (WebSocket) | Live task changes. Use `?token=<token>` because browsers cannot set headers |

An event that does not fit the task's current state is recorded with `accepted: false` and does not change the task.
