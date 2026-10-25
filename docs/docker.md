# Docker

`compose.prod.yaml` runs the Rust daemon and a separate nginx dashboard. The daemon also embeds the dashboard files for direct launches. Docker keeps tasks, the index and worktrees in the `natamux-data` volume.

## Start

From a clone of this repository:

```sh
export NATAMUX_TOKEN="$(openssl rand -base64 32)"
export NATAMUX_REPO=/path/to/the/git/repository
docker compose -f compose.prod.yaml up -d --build
```

Open http://localhost:8080 and enter the token. The daemon listens on port 4317 inside the Compose network; nginx forwards `/api` and WebSocket traffic to it. Only port 8080 is published, on loopback by default.

| Variable | Meaning |
|---|---|
| `NATAMUX_TOKEN` | Required token for every API request |
| `NATAMUX_REPO` | Required git repository, mounted at `/repo` |
| `NATAMUX_BIND` | Dashboard bind address; default `127.0.0.1` |
| `NATAMUX_PORT` | Dashboard port; default `8080` |

The daemon image contains the Rust daemon, Rust CLI, git and curl. Both containers run without root, with read-only root filesystems and no Linux capabilities. The repository must be writable by user 1000 inside the daemon container for worktree commands.

The images contain no coding agent. A worktree created inside the volume is not directly editable from the host. Use the [desktop app](https://natamux.github.io/docs/guides/desktop-app/) for local agent work.

## Try the CI images

`scripts/e2e.sh up` pulls the images built from `main`, checks their GitHub build attestations, and adds demo tasks with a fake agent. It needs Docker and a signed-in GitHub CLI. The token for this demo is `e2e-token`.

```sh
scripts/e2e.sh up
scripts/e2e.sh logs
scripts/e2e.sh down
```

In CI or a codespace, build the current tree and start those images:

```sh
scripts/e2e.sh build
E2E_LOCAL=1 scripts/e2e.sh up
```

Do not use the demo token for a service reachable from other machines. Put a TLS reverse proxy in front of port 8080 before exposing it beyond loopback. The token is the API's access control.

The E2E workflow in `.github/workflows/rust.yml` builds these same images and runs the browser tests. See [End-to-end tests](e2e.md).
