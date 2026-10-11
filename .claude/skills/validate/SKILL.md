---
name: validate
description: Run the Natamux checks (biome, typecheck, tests, site build) for a branch. Use before opening or merging a PR, after a code change, or when the user asks to test, lint or check. The local machine never runs pnpm or node, so checks run in the codespace or in CI.
---

# Validate a branch

## Rule

- Never run `pnpm`, `node`, `npx` or `npm` on the local machine. `.claude/settings.json` denies them.
- Checks run in the codespace (fast, before a push is final) or in CI (always, on the PR).

## Option A: codespace

1. Push the branch first. The codespace checks out what is on GitHub.
2. Find the codespace:
   ```sh
   gh codespace list --repo lenhatthanh20/natamux --json name,state
   ```
3. Run the checks. `remote-check.sh` is in this skill's folder:
   ```sh
   gh codespace ssh -c <name> -- bash -l -s -- <branch> < .claude/skills/validate/remote-check.sh
   ```
   - What it does: worktree `/tmp/v-<branch>` at the pushed commit, then `pnpm install`, `pnpm build`, `pnpm lint`, `pnpm test` and `scripts/fake-agent.test.sh`. It also builds `site/` when the branch changes it.
   - Add `--fix` after the branch name to run `pnpm format` first. The script then prints a patch between `=== PATCH ===` and `=== END ===`. Apply it locally with `git apply`, commit, push.
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

Jobs:

| Workflow | Job | Checks |
|---|---|---|
| CI | `build` | build, `pnpm lint` (biome on `packages` and `e2e`, typecheck), `pnpm test`, fake agent |
| CI | `coverage` | tests with coverage |
| CI | `docker` | both compose setups start and answer |
| CI | `audit` | `pnpm audit --audit-level=high` |
| CI | `os`, `node-24` | macOS, Windows, Node 24. Only on pushes to main |
| E2E | `e2e` | production images and Playwright, see the `e2e` skill |
| Site | `build` | only when `site/**` changes |

## Fixing without running anything

- Biome format: line width 100, 2 spaces, single quotes, semicolons only where needed. A call that does not fit on one line is split one call per line, the way the CI log shows.
- Imports are sorted by module path. Names inside braces are sorted without regard to case, with `type` imports mixed in by name.
- Copy the fix that the CI log prints ("Formatter would have printed the following content") exactly.
- Code that only fails on main (`os` job): a macOS or Windows difference. Check paths, line endings, file locks, and timeouts.
