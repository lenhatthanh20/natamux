---
name: e2e
description: End-to-end testing of the Natamux dashboard on the production images. Use to try the dashboard of main or of a branch in a browser on localhost, take screenshots, check a UI flow with playwright-cli, add or fix Playwright tests in e2e/, or read a failed E2E run.
---

# End-to-end tests

Full reference: `docs/e2e.md`.

## Parts

- `compose.e2e.yaml`: goes on top of `compose.prod.yaml`. It adds a fake agent (`scripts/fake-agent.sh`) in place of Claude Code, git names, and a throwaway repository in a volume.
- `scripts/e2e.sh up|build|seed|logs|down`: needs Docker, curl and `gh` (signed in). Allowed on the local machine: no pnpm or node runs on the host.
  - `up` pulls each image, runs `gh attestation verify` on it (built by `e2e.yml` on main of `lenhatthanh20/natamux`), and starts the containers from the checked digest, not the tag.
  - `E2E_LOCAL=1 up` starts `natamux-{daemon,web}:local` that are loaded already (CI test job).
  - `E2E_RUN=<run-id> up` loads the images of that E2E run (a branch), then starts them like `E2E_LOCAL=1`.
- `e2e/`: the Playwright tests (`@playwright/test` 1.64.0, exact pin). They are outside the pnpm workspace and run only in CI.
- `.github/workflows/e2e.yml`: runs on every PR and push to main. Three jobs:
  - `build`: builds, saves the images as the artifact `images` (`images.tar.zst`, 7 days), outputs their ids. No package write.
  - `test`: one job per group of `e2e/groups.ts` (`timing`, `cache`, `terminal`, `rest`), each with its own daemon; `E2E_GROUP` picks the group. Loads the images, runs the Playwright tests. No package write.
  - `publish` (main only): checks the ids, pushes `ghcr.io/lenhatthanh20/natamux-{daemon,web}:main` and `:sha-<commit>`, attests build provenance. The only job with `packages: write`; installs nothing.
  - Keep it that way: never add a package install or a third-party action that runs code to `publish`, and never give `packages: write` to another job.

## Rules on the local machine

- Allowed: Docker (colima), `scripts/e2e.sh up|seed|logs|down` without `E2E_BUILD`, and `playwright-cli`, which the user installed and allows.
- Never install anything: no `pnpm install`, no `playwright install`, no `npm i -g`, and no browser downloads. If `playwright-cli` is missing or asks to install a browser, stop and ask the user.
- Never `E2E_BUILD=1` or `scripts/e2e.sh build` locally. A build runs `pnpm install` inside Docker. The script refuses it unless `CI` or `CODESPACES` is set.
- Never run the Playwright suite in `e2e/` locally. It needs `pnpm install`, so it runs in CI.
- Never skip or work around the attestation check. If it fails, stop and tell the user.
- Branch images from a run artifact: only through `E2E_RUN`, which checks the run first.

## Try the dashboard locally

1. Docker must run. Start colima with: `colima start --cpu 4 --memory 8 --vm-type vz --mount-type virtiofs --vz-rosetta`
2. Start the images. Pick the code to see:
   - **main**: `scripts/e2e.sh up`. It pulls and checks the images of main, starts them, and adds the demo data.
     - `E2E_TAG=sha-<commit>` picks another main commit.
     - "denied" on pull means the GHCR package is private. Ask the user to make it public, or to run `docker login ghcr.io`.
   - **A branch** (a task's PR, or its `natamux-ci/<branch>` from Check on CI): load the images that its E2E run built. See "Images of a branch" below.
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

### Images of a branch

The `build` job of every E2E run uploads the production images of that commit as the artifact `images` (kept 7 days). No build runs locally.

1. Find the newest E2E run of the branch. Its `build` job must be done (about 2 min after the push):
   ```sh
   gh run list -R lenhatthanh20/natamux -w e2e.yml -b <branch> -L 1 --json databaseId,headSha,status
   ```
2. `E2E_RUN=<run-id> scripts/e2e.sh up`. It checks the run, downloads `images`, loads `natamux-{daemon,web}:local`, starts them and adds the demo data. Then steps 3–5 above.
   - "not an E2E run of …": the run is of another workflow or a fork. Stop and tell the user; never load those images another way.
   - "no images artifact": the artifact expired or the build job failed. Push again, or ask the user before `gh run rerun <run-id>`.
   - The images are x86 Linux; colima runs them with Rosetta (`--vz-rosetta`).
   - Say which commit you show: the `headSha` of step 1.
3. Runs made before `E2E_RUN` existed have `images.tar.gz`, which `E2E_RUN` does not read. Use a newer run.

## Demo data (scripts/e2e.sh seed)

| Task | State | Use |
|---|---|---|
| Demo: changes and history | in review (held at Context) | workspace, 2 commits on a diverged branch, uncommitted changes to `demo/notes.md` and `demo/todo.txt` |
| Command palette opened with Cmd+K | in review, fake agent running `loop:` | agent terminal output: `● Read`, `● Edit notes/…`, `✓ N passed` |
| Fix token refresh race in the daemon | ready | |
| Windows path tests for repos | blocked | |
| Desktop window layout | draft | |
| Show the diff of a workspace | done | |

- The token is `e2e-token` and the port is 8080. `NATAMUX_TOKEN` and `NATAMUX_PORT` change them.
- Each task is held at its first stage with `state.overridden` (`{"to":"review","stage":"context"}`). Without that hold, the next stage starts an agent in the task's shell 1.
- The images have no `natamux` CLI. The fake agent's hooks do nothing, so move task state through the API (`POST /api/tasks/:id/events`).

## Writing tests (e2e/tests/*.spec.ts)

- `signIn(page)` in `helpers.ts` sets `localStorage['natamux.auth']` and waits for the heading "All tasks".
- `openTask(page, title)` switches to Table, double-clicks the row, and returns the tab list "View" (use `exact: true`, because "View of the details" also exists).
- Task rows and cards are named `"<title>, <state>, <id>, <age>"`. Match them with `taskNamed(title)`, and scope to `taskTable(page)` to avoid strict-mode clashes with the sidebar.
- Terminal:
  - The browser runs without WebGL, so text is in `.xterm-rows` inside the region "Terminal screen". Only `terminal-gpu.spec.ts` turns WebGL on (SwiftShader) and counts drawn pixels instead.
  - Wait for `$` before typing.
  - Only the visible rows are in the DOM, so wait for steady output, not for a line that scrolls away.
  - Hidden terminals sit in `#terminal-holder` with their own rows. Use `rows(pane(page, 'Shell 1'))` from `terminal-helpers.ts`, never a page-wide `.xterm-rows`.
  - Terminal numbers: `terminal-measures.spec.ts` and `terminal-cache.spec.ts` record through `measures.ts` into `e2e/test-results/terminal-measures.json` (artifact `e2e-measures-<group>`, every run). `daemon-rss.ts` samples the daemon's memory. Assert limits on a median of `RUNS`, never on one sample. See `docs/e2e.md` "Terminal measures".
- The tests share one daemon and run one at a time. Give any data a test adds a unique name (`Date.now()`).
- A new spec file runs in the CI group `rest`. Add it to `GROUPS` in `e2e/groups.ts` only if it is slow. A file that calls `recordMeasure` must go in a named group (`groups.spec.ts` checks it); put it where the group times stay even.
- `phone.spec.ts` runs on Pixel 7. The other files run on desktop at 1440×900.
- Selectors are accessible names. If a UI change renames a control, update the tests in the same PR.

## A failed E2E run

```sh
gh run view <run-id> --log-failed | grep -E "Error|Expected|Received|✘|›" | head -40
gh run download <run-id> -n e2e-report-<group> -D <scratchpad>/e2e-report
gh run download <run-id> -p 'e2e-measures-*' -D <scratchpad>/e2e-measures
```

- The artifact has `test-failed-*.png`, `video.webm`, `trace.zip` and `error-context.md` (an aria snapshot of the page).
- Read the PNG and `error-context.md` first.
- The workflow also prints the container logs when it fails.
