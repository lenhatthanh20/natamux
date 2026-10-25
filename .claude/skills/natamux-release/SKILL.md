---
name: natamux-release
description: Prepare and publish a signed Natamux Rust release.
---

# Release Natamux

Use this skill only for a release PR or an owner approved release. Feature PRs do not add release notes or changelog entries.

## Release PR

1. Start from green `main`. Pick a stable `vX.Y.Z` tag that has not been used.
2. Set `X.Y.Z` in every `crates/*/Cargo.toml`, `packages/core/package.json`, `packages/web/package.json`, `site/package.json` when it has a version, `site/src/pages/index.astro`, and `crates/natamux-app/resources/Info.plist`. Refresh `Cargo.lock` in CI, not on the owner's Mac.
3. Add `docs/releases/vX.Y.Z.md` and the matching section of the site changelog. Keep the six download names in the notes. Read `docs/release-process.md` for the required wording.
4. Run `scripts/release.test.sh` in CI. Run Release with `publish=false` on the branch. Check its six assets and `SHA256SUMS.txt`, startup smoke results, and build attestations. Run `cargo xtask ci`, the four E2E groups on the Rust image, and the site checks. Merge only with green CI.

## Owner tag and signature

From the merged `main`, the owner runs:

```sh
git fetch --no-tags origin main && git switch --detach origin/main
scripts/release.sh vX.Y.Z
```

`release.sh` asks before it pushes a signed tag. Release CI then builds a draft. `scripts/sign-release.sh` checks the artifacts and asks before it signs and publishes. The minisign secret key stays on the owner's Mac. Do not put it in CI. Run `scripts/release.test.sh --published` after signing.

Never publish an unsigned draft. Do not push a `v*` tag from a feature task.
