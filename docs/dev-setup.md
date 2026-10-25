# Development setup

Develop in a codespace. On the local machine, only edit files.

## First time

1. Create the GitHub repository and push `main`.
2. Open it in a codespace (the badge in the README, or github.com/codespaces).
3. In the codespace terminal:

```sh
pnpm install
```

4. Run checks in the codespace or CI. Do not build on the local Mac.

## Supply-chain settings

Set in `pnpm-workspace.yaml`:

- `minimumReleaseAge: 1440` — skip package versions published in the last 24 hours.
- `strictDepBuilds: true` — fail when a dependency has an unapproved install script. Approve only what is needed with `onlyBuiltDependencies`. Nothing is approved today; `esbuild` is in `ignoredBuiltDependencies`. SQLite comes from Node itself (`node:sqlite`), so it needs Node 22.13 or newer.

## Checks

```sh
cargo xtask ci
pnpm --filter @natamux/core --filter @natamux/web build
pnpm --filter @natamux/core --filter @natamux/web typecheck
pnpm --filter @natamux/core --filter @natamux/web test
```

Run these commands in a codespace or CI only. The web and core build runs before their type checks and tests.

## CI

`.github/workflows/rust.yml` runs Rust checks, OS startup checks, E2E and benchmarks. `.github/workflows/ci.yml` runs the web and core checks. `.github/workflows/release.yml` builds the six native release assets and runs a dry run on pull requests.

Every action in `.github/workflows/` is pinned to a full commit SHA, with the version in a comment, for example `actions/checkout@<sha> # v7.0.1`. A moved tag then cannot change what a job runs. Dependabot updates the SHA and the comment together. When you add an action, pin it the same way.

## Rust

During the Phase 5 cutover, Rust work (`docs/plan-rust-migration.md`) goes through `dev`, then a green cutover pull request to `main`. After cutover, Rust pull requests target `main`.

- Rust builds and tests run in CI (`.github/workflows/rust.yml`): `cargo xtask ci` (fmt, clippy, tests, layers), then `cargo deny`, `cargo audit` and `cargo vet`.
- Never run `cargo` on the owner's Mac, not even in a Natamux task terminal. Build scripts and proc macros run code with the user's rights.
- Optional: the hardened Docker command, and where a separate `natamux-dev` macOS user is allowed. Both are in `docs/plan-rust-migration.md`, "Safe local setup".

## Run the daemon and the dashboard

With Docker. The codespace starts it for you each time it starts. By hand:

```sh
scripts/dev.sh up     # or: scripts/dev.sh demo, for a task with commits and changes
scripts/dev.sh seed   # tasks in every state, with fake agents at work
```

See `docs/docker.md`, which also covers the production setup. Without Docker, see `packages/web/README.md`: the dev container forwards port 5173 (the dashboard), and the daemon stays on `127.0.0.1:4317` and is reached through the dashboard's `/api` proxy.

## Not set up yet

- License: undecided. See `docs/plan-features.md`, section 9.
