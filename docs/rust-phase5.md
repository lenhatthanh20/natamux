# Phase 5: Rust cutover

Done. The Rust implementation was merged into `dev` through [PR #460](https://github.com/lenhatthanh20/natamux/pull/460), and the cutover into `main` through [PR #463](https://github.com/lenhatthanh20/natamux/pull/463) on 2026-10-25. The first Rust release (tag, signing and publishing) remains with the owner.

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
| `cargo xtask ci` in CI | Passed on the cutover tree: [PR #463 checks](https://github.com/lenhatthanh20/natamux/pull/463/checks) and the follow-up [PR #467 checks](https://github.com/lenhatthanh20/natamux/pull/467/checks). |
| Release dry run and OS startup smoke tests | Passed on [PR #467 checks](https://github.com/lenhatthanh20/natamux/pull/467/checks): macOS arm64 and x64, Windows x64, Linux x64, six assets and `SHA256SUMS.txt`. |
| Signed update and last Electron update test | Rust signature and install tests passed in `check`. The release jobs check the last Electron DMG name and bundle contract on `legacy/electron`. CI has no signing key, so the owner updates a running Electron app at the first Rust release (`docs/release-process.md`, step 6). |
| Four E2E groups on the production Rust image | Passed on [PR #467 checks](https://github.com/lenhatthanh20/natamux/pull/467/checks). `terminal-protocol.spec.ts` "shows no history scrolling by while a snapshot is replayed" needed reruns; #472 changed the screen sampler timing. |
| Phase 4 bench method | Passed in [Rust run 40103239773](https://github.com/lenhatthanh20/natamux/actions/runs/40103239773): five alternating rounds. Plain parsing: Node 72.9 MB/s, Rust 94.1 MB/s; SGR parsing: 60.6 / 97.7 MB/s; a 10,000-line snapshot: 62.2 / 6.6 ms. See [Phase 4 gate](rust-phase4.md#gate). `daemon-bench` passed on the cutover tree. |
| Light and dark screenshots at two sizes | `check-shots.py` passed for `shots-mac-window` at 800×560 and 1000×680 in both themes. The owner approved the state shots on 2026-10-25; they are the baselines in `crates/natamux-app/tests/shots/`, and `app-shots` compares against them (`REPORT_ONLY` is off). |
| Release scripts and signing tests | Passed in `check` and `release` on [PR #467 checks](https://github.com/lenhatthanh20/natamux/pull/467/checks). |
| Cutover PR green on `main` | [PR #463](https://github.com/lenhatthanh20/natamux/pull/463) merged. `main` green at `e0353668` ([run 40194095330](https://github.com/lenhatthanh20/natamux/actions/runs/40194095330)). `dev` and the `natamux-ci/*` branches were deleted after that; `legacy/electron` is unchanged. |

No Rust, Node or Docker build ran on the owner's Mac.

## Performance (T-0178)

The app and the daemon with many shells open: fewer draws, reads and copies. Before is the last run before T-0178, after is its CI run on macOS. The numbers come from CI; a cell with "CI" is filled from the first run of the branch.

| Case | Measure | Before | After | Run |
|---|---|---|---|---|
| Motion, task table (`scripts/motion-cost.sh`) | Draws/s with focus, behind Finder | 8.3, 0.07 | CI | CI |
| Motion, open task T-0146 (`--open-task`, 4 quiet panes) | Draws/s and CPU % with focus; draws/s behind Finder | Not measured | CI (limits 1, 2%, 0.2) | CI |
| Gate 2 (`--gate2`, owner's Mac) | Frames over 1 refresh, `panes` report | Pass, no numbers recorded | CI | Owner |
| Daemon bench (`NATAMUX_BENCH=1`, `tests/daemon_bench.rs`) | RSS with 30 idle shells | Not measured | CI | CI |
| Daemon bench | Echo p50 / p95 of a quiet shell while 5 flood | Not measured | CI (limit p95 10 ms) | CI |
| Daemon bench | Snapshot of a full history at 120 cols: ms, allocations, MiB | Not measured | CI | CI |
| Daemon bench | Attach of a shell with a full history | Not measured | CI | CI |

The daemon bench writes its table to the job summary of the `daemon-bench` job.

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
