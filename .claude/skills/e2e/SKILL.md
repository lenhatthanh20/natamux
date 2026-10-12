---
name: e2e
description: End-to-end testing of the Natamux dashboard on the production images. Use to try the dashboard in a browser on localhost, take screenshots, check a UI flow with playwright-cli, add or fix Playwright tests in e2e/, or read a failed E2E run.
---

# End-to-end tests

Full reference: `docs/e2e.md`.

## Parts

- `compose.e2e.yaml`: goes on top of `compose.prod.yaml`. It adds a fake agent (`scripts/fake-agent.sh`) in place of Claude Code, git names, and a throwaway repository in a volume.
- `scripts/e2e.sh up|build|seed|logs|down`: needs Docker, curl and `gh` (signed in). Allowed on the local machine: no pnpm or node runs on the host.
  - `up` pulls each image, runs `gh attestation verify` on it (built by `e2e.yml` on main of `lenhatthanh20/natamux`), and starts the containers from the checked digest, not the tag.
  - `E2E_LOCAL=1 up` starts `natamux-{daemon,web}:local` that are loaded already (CI test job).
- `e2e/`: the Playwright tests (`@playwright/test` 1.64.0, exact pin). They are outside the pnpm workspace and run only in CI.
- `.github/workflows/e2e.yml`: runs on every PR and push to main. Three jobs:
  - `build`: builds, saves the images as an artifact, outputs their ids. No package write.
  - `test`: loads the images, runs the Playwright tests. No package write.
  - `publish` (main only): checks the ids, pushes `ghcr.io/lenhatthanh20/natamux-{daemon,web}:main` and `:sha-<commit>`, attests build provenance. The only job with `packages: write`; installs nothing.
  - Keep it that way: never add a package install or a third-party action that runs code to `publish`, and never give `packages: write` to another job.

## Rules on the local machine

- Allowed: Docker (colima), `scripts/e2e.sh up|seed|logs|down` without `E2E_BUILD`, and `playwright-cli`, which the user installed and allows.
- Never install anything: no `pnpm install`, no `playwright install`, no `npm i -g`, and no browser downloads. If `playwright-cli` is missing or asks to install a browser, stop and ask the user.
- Never `E2E_BUILD=1` or `scripts/e2e.sh build` locally. A build runs `pnpm install` inside Docker. The script refuses it unless `CI` or `CODESPACES` is set.
- Never run the Playwright suite in `e2e/` locally. It needs `pnpm install`, so it runs in CI.
- Never skip or work around the attestation check. If it fails, stop and tell the user.

## Try the dashboard locally

1. Docker must run. Start colima with: `colima start --cpu 4 --memory 8 --vm-type vz --mount-type virtiofs --vz-rosetta`
2. Run `scripts/e2e.sh up`. It pulls and checks the images of main, starts them, and adds the demo data.
   - It shows the code of **main**, not of the current branch. To see a branch, use CI (its report or screenshots) or the codespace.
   - `E2E_TAG=sha-<commit>` picks another main commit.
   - "denied" on pull means the GHCR package is private. Ask the user to make it public, or to run `docker login ghcr.io`.
3. Open it with the token already set:
   ```sh
   playwright-cli open http://localhost:8080
   playwright-cli localstorage-set natamux.auth e2e-token
   playwright-cli reload
   ```
4. Check both themes and a phone:
   - Theme: Account menu → **Dark** or **Light**. It sets `data-theme` on `<html>`.
   - Phone: `playwright-cli resize 390 844`.
5. Run `scripts/e2e.sh down` when done. It removes the demo data and repository volumes, and nothing else.

`curl` to localhost may be denied by the permission classifier. Use playwright-cli or `scripts/e2e.sh`, not raw curl.

## Demo data (scripts/e2e.sh seed)

| Task | State | Use |
|---|---|---|
| Demo: changes and history | in review (held at Context) | workspace, 2 commits on a diverged branch, uncommitted changes to `demo/notes.md` and `demo/todo.txt` |
| Command palette opened with Cmd+K | in review, fake agent running `loop:` | agent terminal output: `● Read`, `● Edit notes/…`, `✓ N passed` |
| Fix token refresh race in the daemon | ready | |
| Windows path tests for repos | blocked | |
| Electron shell spike | draft | |
| Show the diff of a workspace | done | |

- The token is `e2e-token` and the port is 8080. `NATAMUX_TOKEN` and `NATAMUX_PORT` change them.
- Each task is held at its first stage with `state.overridden` (`{"to":"review","stage":"context"}`). Without that hold, the next stage starts an agent in the task's shell 1.
- The images have no `natamux` CLI. The fake agent's hooks do nothing, so move task state through the API (`POST /api/tasks/:id/events`).

## Writing tests (e2e/tests/*.spec.ts)

- `signIn(page)` in `helpers.ts` sets `localStorage['natamux.auth']` and waits for the heading "All tasks".
- `openTask(page, title)` switches to Table, double-clicks the row, and returns the tab list "View" (use `exact: true`, because "View of the details" also exists).
- Task rows and cards are named `"<title>, <state>, <id>, <age>"`. Match them with `taskNamed(title)`, and scope to `taskTable(page)` to avoid strict-mode clashes with the sidebar and Needs you.
- Terminal:
  - The browser runs without WebGL, so text is in `.xterm-rows` inside the region "Terminal screen".
  - Wait for `$` before typing.
  - Only the visible rows are in the DOM, so wait for steady output, not for a line that scrolls away.
  - Hidden terminals sit in `#terminal-holder` with their own rows. Use `rows(pane(page, 'Shell 1'))` from `terminal-helpers.ts`, never a page-wide `.xterm-rows`.
  - Terminal numbers: `terminal-measures.spec.ts` and `terminal-cache.spec.ts` record through `measures.ts` into `e2e/test-results/terminal-measures.json` (artifact `e2e-measures`, every run). `daemon-rss.ts` samples the daemon's memory. Assert limits on a median of `RUNS`, never on one sample. See `docs/e2e.md` "Terminal measures".
- The tests share one daemon and run one at a time. Give any data a test adds a unique name (`Date.now()`).
- `phone.spec.ts` runs on Pixel 7. The other files run on desktop at 1440×900.
- Selectors are accessible names. If a UI change renames a control, update the tests in the same PR.

## A failed E2E run

```sh
gh run view <run-id> --log-failed | grep -E "Error|Expected|Received|✘|›" | head -40
gh run download <run-id> -n e2e-report -D <scratchpad>/e2e-report
gh run download <run-id> -n e2e-measures -D <scratchpad>/e2e-measures
```

- The artifact has `test-failed-*.png`, `video.webm`, `trace.zip` and `error-context.md` (an aria snapshot of the page).
- Read the PNG and `error-context.md` first.
- The workflow also prints the container logs when it fails.
