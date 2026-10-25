# Development setup

Edit locally. Run builds and checks in CI or a codespace. Rust build scripts and proc macros can execute code, so `cargo` does not run on the owner's Mac.

## Codespace

Open the repository in a GitHub Codespace. The dev container installs Node 22, pnpm, Docker and the GitHub CLI. For the web dashboard and shared types:

```sh
pnpm install --frozen-lockfile
pnpm --filter @natamux/core --filter @natamux/web build
```

For a full Rust daemon and dashboard, build the production images in the codespace and start the demo:

```sh
scripts/e2e.sh build
E2E_LOCAL=1 scripts/e2e.sh up
```

Open forwarded port 8080. The demo token is `e2e-token`. Run `scripts/e2e.sh down` to stop it and remove the demo data. See [Docker](docker.md) and [End-to-end tests](e2e.md).

## Checks

Run these in the codespace or CI only:

```sh
cargo xtask ci
pnpm --filter @natamux/core --filter @natamux/web build
pnpm --filter @natamux/core --filter @natamux/web typecheck
pnpm --filter @natamux/core --filter @natamux/web test
```

`.github/workflows/rust.yml` checks Rust, operating systems, the release image and E2E. `.github/workflows/ci.yml` checks web and core. `.github/workflows/release.yml` builds six native release assets and runs a dry run on pull requests. Rust pull requests target `main`. Only `.github/workflows/rust-cache.yml` saves Cargo caches.

Every action in `.github/workflows/` is pinned to a full commit SHA. When adding an action, pin it the same way.

## Dependencies

`pnpm-workspace.yaml` keeps a 24-hour minimum release age and requires an explicit decision for dependency build scripts. `esbuild` is ignored. Ask before adding a package or Rust crate. The package manager version is pinned in `package.json`; use pnpm only.

See [Rust migration](plan-rust-migration.md) for the safe local setup and the reason for the CI-only Rust build rule.
