# Development setup

Edit locally. Run builds and checks in CI or a codespace. Rust build scripts and proc macros can execute code, so `cargo` does not run on the owner's Mac.

## Codespace

Open the repository in a GitHub Codespace. The dev container installs Node 22, pnpm, Docker and the GitHub CLI. Node and pnpm are for `site/` only.

To start the daemon test stack (no UI, see [Docker](docker.md)):

```sh
E2E_BUILD=1 scripts/e2e.sh up
```

The daemon answers on forwarded port 8080 (API only, no UI). The demo token is `e2e-token`. Run `scripts/e2e.sh down` to stop it and remove the demo data.

## Checks

Run these in the codespace or CI only:

```sh
cargo xtask ci
pnpm --dir site install --ignore-workspace --frozen-lockfile
pnpm --dir site build
```

`.github/workflows/rust.yml` checks Rust, operating systems, the daemon image and the live daemon tests. `.github/workflows/release.yml` builds six native release assets, and runs a dry run on pull requests that change packaging and on release pull requests. Rust pull requests target `main`. Only a push to `main` saves Cargo caches. `rust-ok` must be green to merge into `main`.

Every action in `.github/workflows/` is pinned to a full commit SHA. When adding an action, pin it the same way.

## Dependencies

Ask before adding a package or Rust crate. `site/` has its own pnpm project, pinned in `site/package.json`; use pnpm only there.

See [Rust migration](plan-rust-migration.md) for the safe local setup and the reason for the CI-only Rust build rule.
