# Docker

Two setups, kept apart:

| | Development | Production |
|---|---|---|
| File | `compose.yaml` | `compose.prod.yaml` |
| Start | `docker compose up` | `docker compose -f compose.prod.yaml up -d --build` |
| Code | Your folder, mounted. A change reloads | Built into the images |
| Dashboard | Vite dev server, http://localhost:5173 | nginx, http://localhost:8080 |
| Daemon port | 4317, published to loopback | Not published: only the dashboard container reaches it |
| Token | `NATAMUX_TOKEN`, default `devtoken` | `NATAMUX_TOKEN`, **required**, long and random |
| Containers | Run as you (user 1000) | No root, read-only, no capabilities |

Neither image has Claude Code, so neither runs a real agent. They keep tasks, stages, workspaces and the dashboard. The development setup can run a fake agent: `scripts/dev.sh seed`.

## Development (in a codespace)

The dev container has Docker, installs the packages and builds them when the codespace is created. Each time the codespace starts, it runs `scripts/dev.sh up`, so the dashboard is ready on port 5173 with the token `devtoken`.

`scripts/dev.sh`:

| Command | What it does |
|---|---|
| `up` | Starts the containers and waits until the daemon and the dashboard answer |
| `demo` | `up`, then a task "Demo: changes and history" with a workspace, two commits on a branch that diverges from its base, and changes that are not committed. Run it again and it finds the same task |
| `seed` | `up` with the daemon set to run `scripts/fake-agent.sh` in place of Claude Code, then tasks at different stages and states (blocked, draft, done, in review, needs input, failed) and fake agents at work. Run it again and it finds the same tasks |
| `status` | `docker compose ps` |
| `logs` | Follows the logs |
| `down` | Stops the containers. Tasks and worktrees stay |

**Projects** adds more git repositories by their folder on the daemon's machine. In Docker that is a folder inside the container: mount it in `compose.yaml` first.

Each task's terminals get `PORT` (20000 + 10 × the task number). In the dev setup the shells run inside the `daemon` container, so a dev server started there is not reachable from the browser unless you publish that port in `compose.yaml`.

`up` also passes your git name and email (`git config user.name` and `user.email`) for commits made from the dashboard, and the codespace's `GITHUB_TOKEN` as `GH_TOKEN`, so **Push** and **Create PR** work. Set `NATAMUX_GIT_NAME`, `NATAMUX_GIT_EMAIL` or `GH_TOKEN` to use others.

`docker compose up` works too. The first start installs and builds if that has not happened. It starts:

- `setup`: installs packages when `node_modules` is missing, builds `@natamux/core` and the CLI, then stops.
- `core`: rebuilds `@natamux/core` when you change it. Restart `daemon` to pick that up.
- `daemon`: `tsx watch`, restarts on a change in `packages/daemon`.
- `web`: Vite, with `/api` passed to the daemon.

Open the **Ports** tab: 5173 is the dashboard (the codespace opens it for you). Token: `devtoken`.

The CLI inside the container, which already has the token and the address:

```sh
docker compose exec daemon natamux task create "First task"
docker compose exec daemon natamux workspace create T-0001
docker compose exec daemon natamux task list
```

Tasks and worktrees are in `.natamux/` in the repository (git ignores it). `docker compose down` keeps them.

**Ports out of the codespace.** `devcontainer.json` forwards 5173, 4317 and 8080. A forwarded port is private: only you, signed in to GitHub, can open it. Keep it so. To reach the daemon from your own machine, for example for Claude Code's hooks:

```sh
gh codespace ports forward 4317:4317
```

Do not make 4317 public: the only lock on it is the token, and `devtoken` is not a secret. If you ever do, set a long random `NATAMUX_TOKEN` first.

## Production

```sh
export NATAMUX_TOKEN="$(openssl rand -base64 32)"
export NATAMUX_REPO=/path/to/the/git/repository
docker compose -f compose.prod.yaml up -d --build
```

Dashboard at http://localhost:8080. Write the token down: you paste it into the dashboard, and into `NATAMUX_TOKEN` for the CLI.

| Variable | Meaning |
|---|---|
| `NATAMUX_TOKEN` | Required. The token every request needs |
| `NATAMUX_REPO` | Required. The git repository that tasks get worktrees from. Mounted at `/repo` |
| `NATAMUX_BIND` | Where the dashboard listens. Default `127.0.0.1`. Change it only for a reverse proxy that adds TLS |
| `NATAMUX_PORT` | Default `8080` |

What is in it:

- `daemon`: the Rust daemon, Rust CLI, git and curl. The dashboard build is embedded in the daemon. Data (tasks, index, worktrees) is in the volume `natamux-data`, mounted at `/data`.
- `web`: nginx, the built dashboard, and `/api` passed to the daemon (WebSocket too).
- Both: read-only file system, no Linux capabilities, `no-new-privileges`, restart unless stopped.

Things to know:

- **No TLS.** The token travels in clear text. Put a reverse proxy with TLS in front before you open it beyond localhost.
- **The repository must be writable by user 1000** inside the container, or `workspace create` fails (reading tasks still works).
- A worktree lives in the volume, in the container. Nothing on your machine can edit it until agents run inside the setup. That is why production is for tracking tasks for now.
- Hooks of agents reach the daemon through the dashboard's address: set `NATAMUX_URL=http://host:8080/api` where `natamux hooks claude-code` runs.
- A local build carries the tag `local`. The E2E workflow publishes the images of `main` to ghcr.io (see [e2e.md](e2e.md)). Base images are tags, not pinned digests.

## Checks

CI runs a `docker` job on every pull request: it checks both compose files, then starts development and asks it for `/health`, the dashboard page and the demo task's changes and graph. The production setup is built once per change, by the E2E workflow, which starts it from `compose.prod.yaml` and runs the browser tests against it (see [e2e.md](e2e.md)).

The E2E workflow starts the production images with demo data and tests the dashboard in a browser. See [e2e.md](e2e.md).

## Variables of the development setup

| Variable | Default | Meaning |
|---|---|---|
| `NATAMUX_TOKEN` | `devtoken` | Token of the daemon |
| `NATAMUX_UID`, `NATAMUX_GID` | `1000` | The user the containers run as |

Put them in a `.env` file next to `compose.yaml` (git ignores it).
