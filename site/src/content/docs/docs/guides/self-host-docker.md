---
title: Self-host with Docker
description: Run the Natamux dashboard and daemon in hardened Docker containers.
sidebar:
  order: 9
---

The production compose file runs two containers: the **daemon** and the **web** dashboard behind nginx.

:::caution
The images contain no coding agents (no Claude Code and so on). This setup keeps tasks, stages, workspaces and the dashboard. A worktree lives in a Docker volume, so nothing on your machine can edit it. Today it is for tracking tasks. To run real agents, use the [desktop app](/docs/guides/desktop-app/).
:::

## Start it

You need Docker with Compose, and a git repository. From a clone of the Natamux repository:

```sh
export NATAMUX_TOKEN="$(openssl rand -base64 32)"
export NATAMUX_REPO=/path/to/the/git/repository
docker compose -f compose.prod.yaml up -d --build
```

Open `http://localhost:8080` and paste the token into the **Daemon token** field. Write the token down: you also need it for the CLI.

## Try it with demo data

To look around the dashboard without your own repository, start the images that CI builds on `main`. From a clone of the Natamux repository, with Docker and the GitHub CLI:

```sh
scripts/e2e.sh up
```

The script pulls `ghcr.io/lenhatthanh20/natamux-daemon` and `natamux-web`, checks their build attestation with `gh`, starts them, and adds demo tasks. Open `http://localhost:8080` and sign in with the token `e2e-token`. The demo uses a fake agent, and its data lives in Docker volumes. `scripts/e2e.sh down` stops it and removes the demo data. See `docs/e2e.md` in the repository.

## Settings

| Variable | Meaning |
|---|---|
| `NATAMUX_TOKEN` | Required. The token every request needs. Use a long random value |
| `NATAMUX_REPO` | Required. The git repository that tasks get worktrees from. Mounted at `/repo` |
| `NATAMUX_BIND` | Where the dashboard listens. Default `127.0.0.1`. Change it only for a reverse proxy that adds TLS |
| `NATAMUX_PORT` | Dashboard port. Default `8080` |

Compose refuses to start when `NATAMUX_TOKEN` or `NATAMUX_REPO` is missing.

## What runs

- **daemon**: the Rust daemon, the Rust CLI, git and curl. Its data (tasks, index, worktrees) is in the volume `natamux-data`, mounted at `/data`. Its port 4317 is not published: only the web container reaches it. The dashboard files are embedded in the daemon too.
- **web**: nginx with the built dashboard. It passes `/api` to the daemon, WebSocket too.
- Both containers have a read-only file system, no Linux capabilities and `no-new-privileges`, and restart unless you stop them.

## Use the CLI against it

Point the CLI at the dashboard's `/api` address, with your token:

```sh
export NATAMUX_TOKEN=<your token>
export NATAMUX_URL=http://localhost:8080/api
natamux task list
```

Agent hooks reach the daemon the same way: set `NATAMUX_URL=http://host:8080/api` where `natamux hook` runs.

## Security notes

- **There is no TLS.** The token travels in clear text. Put a reverse proxy with TLS in front before you open the dashboard beyond localhost, and only then change `NATAMUX_BIND`.
- Keep the token secret. It is the only lock on the API.
- The repository must be writable by user 1000 inside the container. Otherwise `workspace create` fails, although reading tasks still works.
- `compose.prod.yaml` builds the images locally, with the tag `local`. Base images are tags, not pinned digests. The images that `scripts/e2e.sh` pulls are built by CI from `main` and carry a build attestation.

## Stop and update

```sh
docker compose -f compose.prod.yaml down        # stops; the volume with your tasks stays
docker compose -f compose.prod.yaml up -d --build   # rebuilds with new code
```

## Development setup

In a Codespace, build the Rust production images with `scripts/e2e.sh build`, then run `E2E_LOCAL=1 scripts/e2e.sh up` for demo data on port 8080. See `docs/dev-setup.md` and `docs/docker.md` in the repository.
