# Development setup

Develop in a codespace. On the local machine, only edit files.

## First time

1. Create the GitHub repository and push `main`.
2. Open it in a codespace (the badge in the README, or github.com/codespaces).
3. In the codespace terminal:

```sh
pnpm install
```

4. Commit the generated `pnpm-lock.yaml`.

## Supply-chain settings

Set in `pnpm-workspace.yaml`:

- `minimumReleaseAge: 1440` — skip package versions published in the last 24 hours.
- `strictDepBuilds: true` — fail when a dependency has an unapproved install script. Approve only what is needed with `onlyBuiltDependencies`. Nothing is approved today; `esbuild` is in `ignoredBuiltDependencies`. SQLite comes from Node itself (`node:sqlite`), so it needs Node 22.13 or newer.

## Checks

```sh
pnpm build && pnpm lint && pnpm test
```

Run `build` first: packages import each other through their built `dist` folders, so `lint` (type check) and `test` need `@natamux/core` built. `pnpm format` fixes formatting and import order.

## CI

`.github/workflows/ci.yml` runs on pushes to `main` and on pull requests: frozen install, then `build` and `lint` in one job, and `test` split over three jobs (`vitest --shard`). The root scripts use `--if-present`, so a package without one of them is skipped.

Every action in `.github/workflows/` is pinned to a full commit SHA, with the version in a comment, for example `actions/checkout@<sha> # v7.0.1`. A moved tag then cannot change what a job runs. Dependabot updates the SHA and the comment together. When you add an action, pin it the same way.

## Run the daemon and the dashboard

With Docker. The codespace starts it for you each time it starts. By hand:

```sh
scripts/dev.sh up     # or: scripts/dev.sh demo, for a task with commits and changes
scripts/dev.sh seed   # tasks in every state, with fake agents at work
```

See `docs/docker.md`, which also covers the production setup. Without Docker, see `packages/web/README.md`: the dev container forwards port 5173 (the dashboard), and the daemon stays on `127.0.0.1:4317` and is reached through the dashboard's `/api` proxy.

## Not set up yet

- License: undecided. See `docs/plan-features.md`, section 9.
