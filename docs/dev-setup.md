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

What guards the Rust crates:

- `Cargo.lock` is committed and every build uses `--locked`.
- `deny.toml` allows crates.io and the owner's forks only. Each fork has a `natamux/pin-<sha12>` branch at the locked commit, and a ruleset `natamux pins` on the fork that blocks a delete or a change of it. A new fork gets the same ruleset.
- The `supply-chain` job of `rust.yml` runs `cargo deny`, `scripts/audit.sh` and `cargo vet` when a change touches the lock or these files.
- `cargo audit` and `cargo deny` skip a crate that comes from git. `scripts/audit.sh` runs `cargo audit` a second time on a copy of the lock that names crates.io for those crates.
- `advisories.yml` runs the advisory checks on `main` every day. It runs `scripts/audit.sh osv`, which fails when OSV lists a crate of the lock as malicious (`osv-scanner`, pinned by its SHA-256). It also runs `scripts/audit.sh zed`, which fails on a Zed security advisory that `supply-chain/zed-advisories.txt` does not list. Read the advisory, patch the Zed fork when the bug is in a kept crate, then add the id.
- `cargo vet` imports the audits of every organization in the cargo-vet registry (`supply-chain/config.toml`). A crate with no audit there is in the exemptions list: trusted, not reviewed. Never add an audit for code that nobody read.
- An advisory that cannot be fixed gets an ignore in `deny.toml` with a reason and a review date. `scripts/advisory-dates.py` fails when the date has passed.
- Updates come through Dependabot after a 7-day cooldown. Take a version by hand only when it is 7 days old.
