# Docker

`compose.prod.yaml` runs the Rust daemon alone. It is a test stack for CI and codespaces, not a supported self-host. The images hold no coding agent and there is no browser UI. The [desktop app](https://natamux.github.io/docs/guides/desktop-app/) is the product.

The daemon answers `/api/*` itself. Docker keeps tasks, the index and worktrees in the `natamux-data` volume.

## Start

From a clone of this repository:

```sh
export NATAMUX_TOKEN="$(openssl rand -base64 32)"
export NATAMUX_REPO=/path/to/the/git/repository
docker compose -f compose.prod.yaml up -d --build
```

The daemon listens on port 4317 inside the container. Port 8080 is published, on loopback by default, and maps to it.

| Variable | Meaning |
|---|---|
| `NATAMUX_TOKEN` | Required token for every API request |
| `NATAMUX_REPO` | Required git repository, mounted at `/repo` |
| `NATAMUX_BIND` | Bind address; default `127.0.0.1` |
| `NATAMUX_PORT` | Published port; default `8080` |
| `NATAMUX_ALLOWED_HOSTS` | Host names the daemon accepts (comma list, no ports); default `daemon` plus the loopback names. Other `Host` values get 403 |

The daemon image contains the Rust daemon, Rust CLI, git and curl. The container runs without root, with a read-only root filesystem and no Linux capabilities. The repository must be writable by user 1000 inside the container for worktree commands.

## Test stack for CI and codespaces

`scripts/e2e.sh up` starts the local `natamux-daemon:local` image. `E2E_BUILD=1` builds it first (CI and codespaces only); `E2E_RUN=<run-id>` loads the image a Rust CI run built. It prints `API: http://localhost:8080/api` and the token. The token is `e2e-token`.

```sh
E2E_BUILD=1 scripts/e2e.sh up
scripts/e2e.sh logs
scripts/e2e.sh down
```

The `live-daemon` job in `.github/workflows/rust.yml` builds the same image and runs the live daemon tests against it. Do not use the test token for a service reachable from other machines.
