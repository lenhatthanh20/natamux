# @natamux/daemon

Background process that owns all task state. Clients talk to it over HTTP and WebSocket.

Run in a codespace: `pnpm --filter @natamux/daemon dev`. Build `@natamux/core` first (`pnpm build`).

## Settings (environment variables)

| Variable | Default | Meaning |
|---|---|---|
| `NATAMUX_PORT` | `4317` | Port, bound to `127.0.0.1` only |
| `NATAMUX_TOKEN` | random per run, printed at start | Required on every request |
| `NATAMUX_DATA_DIR` | `<cwd>/.natamux` | Task files and the SQLite index |
| `NATAMUX_REPO_DIR` | `<cwd>` | The git repository tasks get worktrees from |
| `NATAMUX_WORKTREES_DIR` | `<data dir>/worktrees` | Where worktrees are created, one folder per task id |
| `NATAMUX_<AGENT>_COMMAND` | `claude`, `codex`, `gemini`, `copilot`, `cursor-agent` | The program that starts each agent; `<AGENT>` is `CLAUDE`, `CODEX`, `GEMINI`, `COPILOT` or `CURSOR` (`scripts/dev.sh seed` sets a fake Claude) |
| `NATAMUX_<AGENT>_ARGS` | none | Words added to the agent's command line, such as `--allow-all-tools` for Copilot |
| `NATAMUX_HOST` | `127.0.0.1` | Address to bind. Only a container sets it, with its port published to loopback |
| `NATAMUX_WEB_INDEX` | none | Path of the built dashboard's `index.html` (for example `packages/web/dist/index.html`). With it, deep links work, see below |
| `NATAMUX_WEB_DIR` | none | The built dashboard (`packages/web/dist`). With it, the daemon serves the whole dashboard, and the API under `/api` too, on its own port; `NATAMUX_WEB_INDEX` defaults to its `index.html`. The desktop app uses this |

## Storage

- Tasks are markdown files with YAML front matter: `<data dir>/tasks/T-0001.md`. They are the source of truth.
- `<data dir>/index.sqlite` (Node's built-in `node:sqlite`, Node 22.13 or newer) is a cache of the task files plus the event log. The task cache is rebuilt from the files at start. The event log is kept as one JSON line per event in `<data dir>/tasks/T-0001.events.jsonl` next to the task file, and the index is rebuilt from it.
- Stage files of a task are in `.natamux-task/` in its worktree; copies are archived in `<data dir>/tasks/T-0001/` when a stage ends and restored into a new worktree.
- Per-repository settings (the stage pipeline, checks) are `.natamux.json` in the repository, read through `/repos/:id/settings`.

## API

Send `Authorization: Bearer <token>`. `/health` needs no token.

| Method and path | Meaning |
|---|---|
| `GET /health` | Liveness |
| `GET /tasks?state=<state>` | List tasks, newest update first |
| `POST /tasks` | Create a task (`title` required; also `description`, `priority`, `labels`, `agent`, `dependsOn`, `parentId`, `context` as `{ source, ref? }` where source is `manual`, `github-issue`, `skill` or `mcp`, and `repo`, a repository id from `/repos`, 404 when unknown). It gets the stages of its repository's settings and starts the first one (Context) at once |
| `GET /tasks/:id` | One task |
| `PATCH /tasks/:id` | Body: any of `title`, `description`, `priority`, `labels`, `dependsOn`. Other fields answer 400. Logs `task.edited` with the names of the fields that changed; nothing is written when none did |
| `POST /tasks/:id/stage` | Body `{ action, stage?, terminal?, to?, reason? }`. Action is `approve`, `reopen` (go back to `to`, with `reason` for the agent), `skip`, `pass`, `fail`, `report` (the stage is done: run its checks; `natamux done` sends it with `terminal`) or `start`. `stage` names the stage inside a group of side-by-side stages. Answers `{ task }`, or an error when the task is not in a state that fits |
| `GET /tasks/:id/files` | Stage files of the task (`context.md`, `spec.md`, …): `{ files }`. Read from the worktree, or from the archive when there is none |
| `GET /tasks/:id/files/:name` | `{ name, text }`, or 404 |
| `PUT /tasks/:id/files/:name` | Body `{ text }`. Replaces the file, for an edit by hand |
| `POST /tasks/:id/issue` | Opens a GitHub issue for the task now (`gh issue create`). Answers 201 with `{ task }` |
| `PUT /tasks/:id/issue` | Body `{ provider, key, url }`. Links the task to an issue or ticket elsewhere (`{ task }`) |
| `GET /repos/:id/settings` | The repository's `.natamux.json`: the loaded settings and its `path` |
| `PUT /repos/:id/settings` | Body: the settings (the stage pipeline, checks). Saves them to `.natamux.json`: `{ settings }` |
| `GET /tasks/:id/events` | Event log of a task |
| `POST /tasks/:id/events` | Send an event `{ source, type, payload? }`. Returns `{ task, accepted }` |
| `GET /repos` | Repositories whose tasks Natamux manages: `{ repos: [{ id, name, path }] }`. The first is always `default`, the repository the daemon was started for |
| `POST /repos` | Body `{ path, name? }`. Adds the repository whose top folder is `path` (absolute, on the daemon's machine). Its id comes from the name or the folder name. Answers 201; 400 for a path that is not absolute, inside a repository but not its top, or added already; 409 when git finds no repository there. Kept in `<data dir>/repos.json` |
| `GET /git/branches?repo=` | Local branches of a repository (the default one when `repo` is left out) and the one its checkout is on: `{ branches, current }` |
| `POST /tasks/:id/workspace` | Create a git worktree and a branch `natamux/t-0001-<title>` for the task. Body `{ base? }`: start from that local branch instead of HEAD (409 when there is no such branch). When the worktree has a `.natamux-setup` script (`.natamux-setup.ps1` on Windows), it runs in the task's first terminal (`sh ./.natamux-setup`). Answers 409 if the task has one, is done or cancelled, or git refuses |
| `DELETE /tasks/:id/workspace?deleteBranch=true` | Remove the worktree. Git refuses when it has files that are not committed (409). The branch is deleted only when it is fully merged. Returns `{ task, branchDeleted }` |
| `GET /tasks/:id/changes` | Files changed in the workspace compared with its last commit, like VS Code Source Control. Returns `{ changes: [{ path, status, originalPath }] }`. Status is `M`, `A`, `D`, `R`, `U` (new file) or `C` (conflict). Answers 409 when the task has no workspace |
| `GET /tasks/:id/changes/file?path=` | Both sides of one changed file: `{ path, status, original, modified, binary, tooLarge }`. The path must be one of the changes (404 otherwise). A binary file or one over 1 MB comes without text |
| `GET /tasks/:id/graph` | History of the task branch next to the branch that the main checkout is on, newest first, in topological order (at most 200 commits). Returns `{ branch, head, base, ahead, behind, commits: [{ hash, parents, author, date, subject, refs, onBranch }], truncated }`. `base`, `ahead` and `behind` are null when the main checkout is not on a branch. Answers 409 when the task has no workspace |
| `POST /tasks/:id/agent` | Body `{ agent, prompt?, base? }`. A task with no workspace gets one first, from `base` when it is given; the stage that waits for it starts this agent. Writes the agent's command into a small script (`agent-launch-<task id>.sh` in the data folder) and types only a short line that sources it into the task's terminal, starting the shell when needed, so the settings path and the prompt are not echoed on the screen (on Windows the command itself is typed). The shell starts as a login shell (`-l`), like a Terminal window, so `~/.zprofile` and `~/.bash_profile` set up the PATH. When `NATAMUX_BIN_DIR` is set, the script puts that folder first on the agent's PATH. Agents: `claude-code`, `codex`, `gemini`, `copilot`, `cursor` (see `docs/agent-adapters.md`). Sets `agent` on the task and logs `agent.launched`. Answers 404 for another agent |
| `POST /workspaces` | Body `{ title, repo?, base?, agent?, prompt?, context? }`. Makes a task, adds its worktree from `base` (a local branch; the checkout's branch without one) and, with `agent`, starts that agent with `prompt`. The prompt also becomes the task's notes. Answers 201 with `{ task }`, 409 for an unknown base branch (no task is made) and 404 for an unknown repository or agent |
| `POST /tasks/:id/fan-out` | Body `{ count: 1 to 5, start? }`. Copies the task `count` times ("<title> (try 2)", …) with its notes, priority, labels and dependencies. The task and its copies get the label `tries-t-0001`. With `start: true` each copy gets a workspace and Claude Code. Answers 201 with `{ tasks }` |
| `POST /tasks/:id/agent/message` | Body `{ text }`. Pastes the text into the task's first terminal (bracketed paste, then Enter), so the agent there takes it as its next prompt. Logs `review.commented`. Answers 409 when that terminal is not open |
| `POST /tasks/:id/commit` | Body `{ message }`. Commits every change of the workspace, new files included. Returns `{ task, hash }` and logs `git.committed`. Answers 409 when there is nothing to commit |
| `POST /tasks/:id/push` | Pushes the task branch to `origin` and sets its upstream. Logs `git.pushed`. For github.com, git asks `gh auth git-credential` for the password after any helper you have, so `GH_TOKEN` or `gh auth login` is enough |
| `POST /tasks/:id/pull-request` | Pushes, then runs `gh pr create` from the task branch into the default branch, with the task's title and description. Keeps the address in `pr` and logs `pr.opened`. When a pull request for the branch exists, keeps its address. Answers 409 when the task has a pull request already or `gh` fails |
| `POST /tasks/:id/pull-request/refresh` | Reads the task's pull request from GitHub now (`gh pr view`) instead of at the next minute. The daemon does this by itself once a minute for every task with an open pull request. The answer goes into `prStatus` (`{ state: open, draft, merged or closed, checks: passing, failing, pending or none }`). Logs `ci.passed`, `ci.failed`, `pr.closed` and `pr.merged` when they change; `pr.merged` moves a task in review to done |
| `GET /tasks/:id/terminals` | Names of the open terminals of the task: `{ terminals: ['1', '2'] }` |
| `DELETE /tasks/:id/terminals/:terminal` | Ends that shell: it is forgotten at once and its screen is dropped, so a shell opened with the same name next is a new process with an empty screen. The process gets SIGHUP, and SIGKILL after 2 seconds if it is still alive. Returns the terminals that stay open |
| `POST /tasks/:id/terminal/ticket` | A ticket that opens the terminal socket once, within 30 seconds: `{ ticket }`. A browser cannot set headers on a WebSocket, so this keeps the daemon token out of the socket URL. Answers 409 when the task has no workspace |
| `GET /tasks/:id/terminal?ticket=&cols=&rows=&terminal=` (WebSocket) | A shell in the task's workspace. `terminal` names it (`1`, `2`, …; `1` when left out); the agent runs in `1`. It keeps running when the socket closes; the next socket first gets the shell's size set to its own, then the recent output (at most 200,000 characters, cut to the last screen and 1,000 lines). Frames are JSON: the dashboard sends `{ type: 'input', data }` and `{ type: 'resize', cols, rows }`, the daemon sends `{ type: 'output', data }`, `{ type: 'exit', code }` and `{ type: 'error', message }`. A bad ticket closes the socket with code 4401. The shell gets `NATAMUX_TASK_ID`, so `natamux hook` names the task. Removing the workspace ends the shell |
| `GET /templates` | Task templates, sorted by name: `{ templates: [{ id, name, title, description, priority, labels }] }`. Each is a markdown file in `<data dir>/templates` |
| `POST /templates` | Body `{ name, title, description?, priority?, labels? }`. Saves a template; its id comes from the name, and a template with the same name is replaced. Answers 201 |
| `POST /templates/:id/tasks` | Body `{ title }`. A new draft task with the template's notes, priority and labels. Answers 201, or 404 for an unknown template |
| `GET /activity` | What the agent of each task does now, from its last hook: `{ activity: { 'T-0001': { text: 'Edit src/a.ts', at } } }`. Kept in memory; a task drops out when its agent's turn ends or it asks the user |
| `GET /diffstats` | The size of the work of every task with a workspace: `{ stats: { 'T-0001': { files, added, removed } } }`, counted from where the branch left the branch of the main checkout, uncommitted changes included. Files git does not track yet are not counted |
| `GET /github/issues` | Open issues of the repository, from `gh issue list` (at most 100): `{ issues: [{ number, title, body, url, labels, taskId }] }`. `taskId` names the task made from the issue, or is null |
| `POST /github/issues/import` | Body `{ numbers }`. Makes a draft task from each open issue that was not imported yet: its title, its body plus `Imported from <url>` as notes, its labels. Returns `{ tasks }` |
| `POST /hooks/:agent` | Where an agent posts its hook, for example `/hooks/claude-code`. The adapter of that agent turns the hook into task events. Always answers `200 {}` (even for a hook that matches no task or changes nothing), `404` for an agent without an adapter, `400` for input that is not a hook of that agent. The task comes from the `X-Natamux-Task` header or from the workspace that holds the hook's `cwd`. See `docs/agent-adapters.md` and `docs/claude-code-adapter.md` |
| `GET /events` (WebSocket) | Live task changes. Use `?token=<token>` because browsers cannot set headers |

## Deep links

The dashboard keeps its view in the address (`/board`, `/agents`, `/t/T-0018`, `/w/T-0018/changes?file=src/a.ts`). When `NATAMUX_WEB_INDEX` is set, a `GET` that matches no API route and accepts `text/html` (a reload or a pasted link) is answered with that `index.html` and status 200, without a token: the page holds no data. Any other request keeps its answer: a known API route still needs the token, and an unknown path that does not accept HTML is a 404 (401 without a token). Without `NATAMUX_WEB_DIR` the daemon serves only this one file, so the page works only behind something that serves the scripts and styles (Vite in development, nginx in Docker; both reach the daemon through `/api`). With `NATAMUX_WEB_DIR` it serves them too, without a token, from that folder only, with the same security headers as the nginx setup; files under `assets/` are cached for good, since their names hold a hash.

An event that does not fit the task's current state is recorded with `accepted: false` and does not change the task.
