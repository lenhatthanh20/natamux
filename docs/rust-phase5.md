# Phase 5: Rust cutover

Phase 5 is in progress. This page records decisions and gate results for the cutover. The release tag, signing, and publishing remain with the owner.

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
| `cargo xtask ci` in CI | Pending |
| Release dry run and OS startup smoke tests | Pending |
| Signed update and last Electron update test | Pending |
| Four E2E groups on the production Rust image | Pending |
| Phase 4 bench method | Pending |
| Light and dark screenshots at two sizes | Pending |
| Release scripts and signing tests | Pending |
| Cutover PR green on `main` | Pending |

The GitHub CLI could not reach the configured proxy during implementation. No local build or package check was run.
