# Docker

Two setups, kept apart:

| | Development | Production |
|---|---|---|
| File | `compose.yaml` | `compose.prod.yaml` |
| Start | `docker compose up` | `docker compose -f compose.prod.yaml up -d --build` |
| Code | Your folder, mounted. A change reloads | Built into the images |
| Dashboard | Vite dev server, http://localhost:5173 | nginx, http://localhost:8080 |
| Daemon port | 4317, published to loopback | Not published: only the dashboard container reaches it |
| Token | `HELMSMAN_TOKEN`, default `devtoken` | `HELMSMAN_TOKEN`, **required**, long and random |
| Containers | Run as you (user 1000) | No root, read-only, no capabilities |

Neither setup runs agents yet. They keep tasks, workspaces and the dashboard.

## Development (in a codespace)

The dev container has Docker, installs the packages and builds them when the codespace is created. Each time the codespace starts, it runs `scripts/dev.sh up`, so the dashboard is ready on port 5173 with the token `devtoken`.

`scripts/dev.sh`:

| Command | What it does |
|---|---|
| `up` | Starts the containers and waits until the daemon and the dashboard answer |
| `demo` | `up`, then a task "Demo: changes and history" with a workspace, two commits on a branch that diverges from its base, and changes that are not committed. Run it again and it finds the same task |
| `status` | `docker compose ps` |
| `logs` | Follows the logs |
| `down` | Stops the containers. Tasks and worktrees stay |

**Repos** adds more repositories by their folder on the daemon's machine. In Docker that is a folder inside the container: mount it in `compose.yaml` first.

Each task's terminals get `PORT` (20000 + 10 × the task number). In the dev setup the shells run inside the `daemon` container, so a dev server started there is not reachable from the browser unless you publish that port in `compose.yaml`.

`up` also passes your git name and email (`git config user.name` and `user.email`) for commits made from the dashboard, and the codespace's `GITHUB_TOKEN` as `GH_TOKEN`, so **Push** and **Create pull request** work. Set `HELMSMAN_GIT_NAME`, `HELMSMAN_GIT_EMAIL` or `GH_TOKEN` to use others.

`docker compose up` works too. The first start installs and builds if that has not happened. It starts:

- `setup`: installs packages when `node_modules` is missing, builds `@helmsman/core` and the CLI, then stops.
- `core`: rebuilds `@helmsman/core` when you change it. Restart `daemon` to pick that up.
- `daemon`: `tsx watch`, restarts on a change in `packages/daemon`.
- `web`: Vite, with `/api` passed to the daemon.

Open the **Ports** tab: 5173 is the dashboard (the codespace opens it for you). Token: `devtoken`.

The CLI inside the container, which already has the token and the address:

```sh
docker compose exec daemon helmsman task create "First task"
docker compose exec daemon helmsman workspace create T-0001
docker compose exec daemon helmsman task list
```

Tasks and worktrees are in `.helmsman/` in the repository (git ignores it). `docker compose down` keeps them.

**Ports out of the codespace.** `devcontainer.json` forwards 5173, 4317 and 8080. A forwarded port is private: only you, signed in to GitHub, can open it. Keep it so. To reach the daemon from your own machine, for example for Claude Code's hooks:

```sh
gh codespace ports forward 4317:4317
```

Do not make 4317 public: the only lock on it is the token, and `devtoken` is not a secret. If you ever do, set a long random `HELMSMAN_TOKEN` first.

## Production

```sh
export HELMSMAN_TOKEN="$(openssl rand -base64 32)"
export HELMSMAN_REPO=/path/to/the/git/repository
docker compose -f compose.prod.yaml up -d --build
```

Dashboard at http://localhost:8080. Write the token down: you paste it into the dashboard, and into `HELMSMAN_TOKEN` for the CLI.

| Variable | Meaning |
|---|---|
| `HELMSMAN_TOKEN` | Required. The token every request needs |
| `HELMSMAN_REPO` | Required. The git repository that tasks get worktrees from. Mounted at `/repo` |
| `HELMSMAN_BIND` | Where the dashboard listens. Default `127.0.0.1`. Change it only for a reverse proxy that adds TLS |
| `HELMSMAN_PORT` | Default `8080` |

What is in it:

- `daemon`: Node, git and the built daemon. Data (tasks, index, worktrees) in the volume `helmsman-data`, mounted at `/data`.
- `web`: nginx, the built dashboard, and `/api` passed to the daemon (WebSocket too).
- Both: read-only file system, no Linux capabilities, `no-new-privileges`, restart unless stopped.

Things to know:

- **No TLS.** The token travels in clear text. Put a reverse proxy with TLS in front before you open it beyond localhost.
- **The repository must be writable by user 1000** inside the container, or `workspace create` fails (reading tasks still works).
- A worktree lives in the volume, in the container. Nothing on your machine can edit it until agents run inside the setup. That is why production is for tracking tasks for now.
- Hooks of agents reach the daemon through the dashboard's address: set `HELMSMAN_URL=http://host:8080/api` where `helmsman hooks claude-code` runs.
- The images are not pushed anywhere and carry the tag `local`. Base images are tags, not pinned digests.

## Checks

CI runs a `docker` job on every pull request: it checks both compose files, builds and starts production and asks it for `/api/health`, the task list and the dashboard page, then does the same for development. The setups were written without running Docker on the author's machine, so this job is the first real test of a change.

## Variables of the development setup

| Variable | Default | Meaning |
|---|---|---|
| `HELMSMAN_TOKEN` | `devtoken` | Token of the daemon |
| `HELMSMAN_UID`, `HELMSMAN_GID` | `1000` | The user the containers run as |

Put them in a `.env` file next to `compose.yaml` (git ignores it).
