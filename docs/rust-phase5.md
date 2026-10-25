# Phase 5: Rust cutover

The Rust implementation was merged into `dev` through [PR #460](https://github.com/lenhatthanh20/natamux/pull/460) at `3fdeb7bd`. The cutover to `main` is being prepared. The release tag, signing, and publishing remain with the owner.

## Decisions

- The Rust app and the last Electron app use the same `Natamux` user data folder. This lets the Rust CLI read the saved daemon token. The Rust app writes the daemon port to a private file because it may need another loopback port when 4317 is busy.
- The app sends the login shell `PATH` to its daemon through the child process's stdin and keeps the pipe open while it runs. The daemon waits at most 15 seconds for the bounded message and shuts down if its parent closes the pipe. Direct daemon starts do not wait for a parent message.
- The daemon embeds the web build in release builds. `NATAMUX_WEB_DIR` takes precedence when set. API and socket paths stay in the authenticated router.
- The update verifier uses the exact public key of the last Electron release. It accepts only a legacy Ed25519 minisign signature with the trusted comment `natamux <tag>`. The six release asset names remain the same so an older Electron app can find a Rust package.
- `ring` was already present in `Cargo.lock`. No new registry or Git source was added for the signature check.
- The updater uses the system `curl` executable for HTTPS and proxy handling. The Rust code accepts release URLs only from the Natamux GitHub repository, limits response size, verifies the signed sums, and hashes each streamed asset. Desktop packages must include or require `curl`.
- The production daemon image runs the Rust binary and embeds the web build. The separate nginx image remains for the existing port 8080 deployment.
- The release workflow builds the web dashboard once, then packages the Rust app, daemon and CLI on macOS, Windows and Linux. It keeps the six filenames of the Electron release. The Linux AppImage build uses AppImage's `appimagetool` 1.9.1 and a dated runtime in CI; it checks both downloads against their release digests before running the tool. The deb package declares `curl` because the updater uses it.
- The release PR changes Rust crate, web, site, macOS bundle, and homepage versions together. Feature PRs have no release notes.
- The macOS release jobs check the last Electron updater on `legacy/electron` still asks for the same DMG name and `Natamux.app`, then copy and verify the Rust bundle from the DMG with its version. CI has no signing key, so this checks the package contract; a signed upgrade of a running Electron app remains a release gate.

## Gate results

| Gate | Result |
| --- | --- |
| `cargo xtask ci` in CI | Passed for `594e2be9` on [PR #460 checks](https://github.com/lenhatthanh20/natamux/pull/460/checks). The cutover tree still needs a run. |
| Release dry run and OS startup smoke tests | Passed on [PR #460 checks](https://github.com/lenhatthanh20/natamux/pull/460/checks): macOS arm64 and x64, Windows x64, Linux x64, six assets and `SHA256SUMS.txt`. Cutover run pending. |
| Signed update and last Electron update test | Rust signature and install tests passed on [PR #460 checks](https://github.com/lenhatthanh20/natamux/pull/460/checks). The workflow checks the last Electron DMG name and bundle contract on `legacy/electron`; a running Electron-to-Rust upgrade dry run is still pending. |
| Four E2E groups on the production Rust image | Passed on [PR #460 checks](https://github.com/lenhatthanh20/natamux/pull/460/checks). Cutover run pending. |
| Phase 4 bench method | Passed in [Rust run 40103239773](https://github.com/lenhatthanh20/natamux/actions/runs/40103239773): five alternating rounds. Plain parsing: Node 72.9 MB/s, Rust 94.1 MB/s; SGR parsing: 60.6 / 97.7 MB/s; a 10,000-line snapshot: 62.2 / 6.6 ms. See [Phase 4 gate](rust-phase4.md#gate). |
| Light and dark screenshots at two sizes | The `app-macos` job on [PR #460 checks](https://github.com/lenhatthanh20/natamux/pull/460/checks) captured `shots-mac-window` at 800×560 and 1000×680 in both themes; `check-shots.py` passed. The state shot jobs also passed, but `REPORT_ONLY=1` leaves visual baseline approval open. |
| Release scripts and signing tests | Passed in `check` and `release` on [PR #460 checks](https://github.com/lenhatthanh20/natamux/pull/460/checks). |
| Cutover PR green on `main` | Pending. |

No Rust, Node or Docker build ran on the owner's Mac. The cutover PR and its CI results are still pending.

## Cutover deletions

The Rust crate tests now keep their last Node text fixtures under their own `tests/fixtures/` folders. This lets `include_str!` work after the Node sources are removed.

| Deleted path | Reason |
|---|---|
| `packages/desktop/` | The GPUI app and Rust updater replace Electron. The last Electron release stays on `legacy/electron`. |
| `packages/daemon/` | `natamux-daemon` serves the API and embedded dashboard. |
| `packages/cli/` | `natamux-cli` ships `natamux` and `nmx`. |
| `.github/workflows/native-beta.yml` | The Rust release workflow builds the native assets. |
| `compose.yaml`, `docker/dev.Dockerfile`, `scripts/dev.sh` | The old development containers run the removed Node daemon and CLI. Codespaces can build and start the production Rust images. |
| `compose.rust-daemon.yaml`, `compose.rust-e2e.yaml`, `compose.rust-cli.yaml` | The production daemon image is Rust, so a side-by-side or binary overlay is no longer needed. |
| `scripts/cli-parity.sh`, `scripts/xterm-grid.mjs`, `scripts/task-file-golden.mjs`, `scripts/ci-wait-artifact.sh` | These supported the old Node-versus-Rust comparison job. |
| `scripts/compare-terminal-bench.py`, `scripts/compare-terminal-bench.test.py` | The Phase 4 comparison result is recorded above; CI keeps a Rust bench run. |
| `scripts/package-desktop.sh`, `scripts/make-icons.sh`, `scripts/icons.test.sh` | The Rust packaging script and app resources replace the Electron package and icon build. |

The old Node comparison job in `rust.yml` and the Node health fallback in `compose.prod.yaml` are removed for the same reason.
