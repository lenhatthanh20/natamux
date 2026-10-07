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
- `strictDepBuilds: true` — fail when a dependency has an unapproved install script. Approve only what is needed (for example `node-pty`) with `onlyBuiltDependencies`.

## CI

`.github/workflows/ci.yml` runs on pushes to `main` and on pull requests: frozen install, then `lint`, `build` and `test`. The root scripts use `--if-present`, so a package without one of them is skipped.

## Not set up yet

- License: undecided. See `docs/plan-features.md`, section 9.
- Forwarded ports: add when the daemon and dashboard exist.
