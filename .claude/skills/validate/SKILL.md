---
name: validate
description: Run the Natamux checks (Rust CI and site build) for a branch. Use before opening or merging a PR, after a code change, or when the user asks to test, lint or check. The local machine never runs cargo, pnpm or node, so checks run in CI (or the codespace for the site).
---

# Validate a branch

## Rule

- Never run `cargo`, `pnpm`, `node`, `npx` or `npm` on the local machine. `.claude/settings.json` denies the web tools.
- Rust checks run in CI only (`cargo xtask ci` in `rust.yml`). The site build runs in CI (`site.yml`) or in the codespace.

## Option A: codespace (site build only)

1. Push the branch first. The codespace checks out what is on GitHub.
2. Find the codespace:
   ```sh
   gh codespace list --repo lenhatthanh20/natamux --json name,state
   ```
3. Run the checks. `remote-check.sh` is in this skill's folder:
   ```sh
   gh codespace ssh -c <name> -- bash -l -s -- <branch> < .claude/skills/validate/remote-check.sh
   ```
   - What it does: worktree `/tmp/v-<branch>` at the pushed commit, then `scripts/fake-agent.test.sh`. It also installs and builds `site/` when the branch changes it. It does not run Rust checks.
   - Runs take a few minutes. Use a timeout of 600000 ms.
4. Read the result: the last line is `ALL CHECKS PASSED` or the failing step.

Codespace gotchas:

- HTTP 402 "billing issue": the free quota is used up (it resets on the 1st of the month). Use option B. Do not retry.
- A login shell `cd`s to `/workspaces/natamux` unless `SHELL_LOGGED_IN=true`. The script uses absolute paths, so this does not matter.
- A background process started over `gh codespace ssh` needs `setsid … < /dev/null &`, or the session hangs.
- One long SSH session can stop printing without failing. Prefer short calls.

## Option B: CI

1. Push and open the PR (`gh pr create`).
2. Wait for the checks. `sleep` before a command is blocked. Use one call with a long timeout:
   ```sh
   until [ "$(gh pr checks <n> --json state --jq '[.[] | select(.state=="PENDING" or .state=="QUEUED" or .state=="IN_PROGRESS")] | length')" = "0" ]; do sleep 20; done; gh pr checks <n>
   ```
3. On a failure, read only the failed steps:
   ```sh
   gh run view <run-id> --log-failed | tail -80
   ```

Jobs. Rust PRs target `main`. The Rust workflow runs on `main`, PRs into `main`, and snapshots. `Site` checks site changes.

| Workflow | Job | Checks |
|---|---|---|
| Site | `build` | only when `site/**` changes |
| Rust | `check` | `cargo xtask ci`: fmt, clippy, tests, layers. Always runs |
| Rust | `app-macos`, `app-windows`, daemon and client jobs | only when the change can break them (`scripts/ci-changes.py`); `main` runs all |
| Rust | `app-shots (1..4)` | on demand only: a `Shots:` line (1 job) or `[ci: shots]` / `[ci: all]` (4 jobs) in the head commit message; never on `main` |
| Rust | `e2e-image`, `live-daemon` | build the daemon test image and run the live daemon tests against it (`scripts/e2e.sh`, `docs/docker.md`) |
| Rust | `rust-ok` | red when any Rust job failed or was cancelled. Required to merge into `main` |
| Rust | `supply-chain` | `cargo deny --locked check`, `cargo audit`, `cargo vet --locked` |
| Release | `validate`, `package` | check the release ref and package the native app for release |

- Rust snapshots: every task pushes to the shared `natamux-ci/rust` (the jobs the change needs) and `natamux-ci/rust-check` (only `check`, for the compile loop). Keep both branches at cleanup. Only a push to `main` saves Cargo caches (`check`, `app-macos`, `app-windows`); every other run restores them.
- `[ci: all]` in the head commit message of a snapshot or a PR runs every job, `app-windows` too.

## Fixing without running anything

- Copy the fix that the CI log prints (rustfmt "Diff in", clippy suggestions) exactly.
- Code that only fails on main (`os` job): a macOS or Windows difference. Check paths, line endings, file locks, and timeouts.
