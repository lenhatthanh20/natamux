# End-to-end tests

The production images, started with demo data, tested in a real browser with Playwright.

| Part | What it is |
|---|---|
| `compose.e2e.yaml` | On top of `compose.prod.yaml`: a fake agent (`scripts/fake-agent.sh`) in place of Claude Code, git names for commits, and a throwaway repository in a volume |
| `scripts/e2e.sh` | Checks and starts the images, adds the demo data, stops them. Needs Docker, curl and the GitHub CLI |
| `e2e/` | The Playwright tests. Not part of the pnpm workspace, so the images do not install it |
| `.github/workflows/rust.yml` | Builds the Rust production images, runs the four test groups, and on `main` publishes the images to ghcr.io |

## Demo data

`scripts/e2e.sh up` adds:

- **Demo: changes and history**: a workspace with two commits on a branch that diverges from its base, and changes that are not committed.
- **Command palette opened with Cmd+K**: a fake agent that keeps working in its terminal.
- Tasks that are ready, blocked, draft and done.

The dashboard is on http://localhost:8080 with the token `e2e-token`. Set `NATAMUX_PORT` or `NATAMUX_TOKEN` to change them.

## In CI

Rust CI runs these jobs when the daemon, image, or E2E code changes. It builds the production images from the branch, so the tests check the code of the pull request. A failed group keeps its Playwright report and measures. The images stay as the `rust-production-images` artifact for three days.

The Rust workflow has three E2E jobs:

| Job | Does | Can write packages |
|---|---|---|
| `e2e-image` | `scripts/e2e.sh build`, then saves the production Rust images and their ids | no |
| `e2e` | one job per group: loads the images, starts them, installs Playwright, runs the group | no |
| `e2e-publish` | on `main` only: checks the image ids, pushes the images, attests their build provenance | yes |

The tests share one daemon, so in one job they run one at a time. To save time, CI runs them in groups, each in its own `test` job with its own daemon and demo data. `e2e/groups.ts` names the groups, and the job sets `E2E_GROUP`:

| Group | Spec files |
|---|---|
| `timing` | `dashboard-measures.spec.ts`, `terminal-measures.spec.ts` |
| `cache` | `terminal-cache.spec.ts` |
| `terminal` | `terminal-gpu.spec.ts`, `terminal-layout.spec.ts`, `terminal-look.spec.ts`, `terminal-protocol.spec.ts` |
| `rest` | every other spec file, so a new file runs with no change to the workflow |

Without `E2E_GROUP`, `pnpm e2e` runs every spec file. `E2E_GROUP=<group> pnpm e2e` runs one group, as its CI job does. An unknown group stops the run with an error. `groups.spec.ts` checks that each test of a full run is in exactly one group, that the workflow runs every group, and that every spec file that records measures is in a named group. A slow new file goes in a named group, so the groups take about the same time.

Only `publish` can push images, and it installs no packages. A bad package in the Playwright install of `test` cannot push an image, and it cannot swap the artifact either, because `publish` checks the ids.

On `main`, after the tests pass, the images go to `ghcr.io/lenhatthanh20/natamux-daemon` and `natamux-web`, tagged `main` and `sha-<commit>`, each with a signed build provenance attestation from `rust.yml`.

## On your machine

Needs Docker (for example colima) and the GitHub CLI, signed in. Nothing is built: the images come from ghcr.io.

Before it starts them, `scripts/e2e.sh up` runs `gh attestation verify` on each image. It accepts images from the Rust workflow on `main`. The containers start from the digest that was checked.

```sh
scripts/e2e.sh up      # pull and check the images of main, start them, add the demo data
scripts/e2e.sh logs
scripts/e2e.sh down    # stop, and remove the demo data and the repository
```

Nothing is installed on your machine. `scripts/e2e.sh build` and `E2E_BUILD=1`, which build the images and so install packages, work only in CI or a codespace. The script refuses them elsewhere. The Playwright tests in `e2e/` run in CI only.

### A branch before it merges

`E2E_RUN=<run-id> scripts/e2e.sh up` starts the images that a Rust run built, for example the run of a pull request or of a `natamux-ci/` branch. It needs the GitHub CLI and Docker only.

```sh
gh run list -w rust.yml -b <branch> -L 1 --json databaseId --jq '.[0].databaseId'
E2E_RUN=<run-id> scripts/e2e.sh up
```

- It reads the run first and refuses it unless it is a run of `rust.yml` in `lenhatthanh20/natamux` itself, not of a fork. These branch images have no attestation; this check takes its place.
- It downloads the `rust-production-images` artifact, loads `natamux-daemon:local` and `natamux-web:local`, and starts them as `E2E_LOCAL=1` does.
- The artifact is there once the run's `build` job ends, and for 7 days.
- `scripts/e2e.test.sh` checks the run check with stubs for `gh` and `docker`.

`E2E_TAG=sha-<commit>` picks the images of another commit of `main`. The tag `main` moves with every merge, so a `sha-` tag picks a known build. Images published before the workflow attested its builds fail the check. The images are built for x86 Linux; on a Mac with Apple silicon, Docker runs them with Rosetta (colima: `--vz-rosetta`).

The Rust workflow's `e2e` job runs the real production Rust daemon image in every group. It does not replace the daemon inside an older image. The image is built in CI only, never on the owner's Mac.

Then try the dashboard by hand or with `playwright-cli`:

```sh
playwright-cli open http://localhost:8080
playwright-cli localstorage-set natamux.auth e2e-token
playwright-cli reload
```

## Writing tests

- The tests share one daemon and run one after the other. A test that adds data gives it a name of its own, for example with `Date.now()`.
- `signIn(page)` in `e2e/tests/helpers.ts` puts the token in `localStorage`, so a test starts on the dashboard.
- The browser starts without WebGL, so the terminal draws its text into the page, where `.xterm-rows` shows it. Only `terminal-gpu.spec.ts` turns WebGL on (SwiftShader): it checks the GPU budget (two tasks of 4 shells switch with no new context), renderer changes while a terminal is hidden, that a renderer given back loses its context, and lost contexts, and it counts drawn pixels in screenshots, as a canvas has no text to read.
- `phone.spec.ts` runs with a phone screen; the other files with a desktop one.
- The images have no real agent. A test that needs one uses the fake agent of the demo data.
- Terminals the page keeps for later sit in the hidden `#terminal-holder` with their own `.xterm-rows`. Find rows inside a pane region (`rows(pane(page, 'Shell 1'))` in `terminal-helpers.ts`), never on the whole page.

## Terminal measures

These spec files time the terminal and the dashboard, so a change can show its effect with numbers.

| File | What |
|---|---|
| `terminal-measures.spec.ts` | The baseline: time to show a long history, sockets per tab switch, JS heap, keypress to paint, echo and Ctrl+C during a flood, daemon memory. It mostly records numbers. Limits: a 10,000-line plain history shows within 500 ms (median of 5, AC 8); in a flood of 5,000,000 lines in another shell, the echo p95 is at most 100 ms, until the prompt is back the second-longest long task is at most 100 ms and the longest at most 200 ms (one GC pause may pass 100), Ctrl+C reaches the prompt within 500 ms (median of 5), and the daemon memory grows by at most 100 MB (peak minus the median just before the flood; AC 25, 26) |
| `terminal-cache.spec.ts` | The terminal cache: showing a terminal again within 100 ms with no new socket, the limit of 8 hidden terminals, the heap of hidden terminals. While the hidden shells of another task flood, the echo is at most 100 ms (p95) with no long task over 150 ms, and the flooded task catches up when shown again (AC 45) |
| `terminal-protocol.spec.ts` | The catch-up of a new connection: no history scrolls by while the snapshot is written (the skeleton shows instead), and keys typed while `replayed` is held back (`page.routeWebSocket`) reach the shell once, after it, in order. Reconnect: a socket closed from the page comes back to the same shell with only the missed output, `exit` opens no new socket, and replies to queries printed while the socket was closed are never sent (`controlTerminalSockets`) |
| `terminal-layout.spec.ts` | A pane that changes size (New shell, Close shell, a review banner that comes and goes): Shell 1's screen keeps its old size until it is hidden, every frame drawn after that has the final size, it fades in, and each change sends one new size, for Shell 1 only. A new shell starts at its final size |
| `dashboard-measures.spec.ts` | The dashboard: JS and CSS bytes of the first load, and time from a reload to the first task row and to the `natamux:first-render` mark (median of 5). HTTP requests per minute in 60 s on the Terminal tab of one of 10 tasks whose fake agents work. Limit: at most 20% of what the polls before T-0033 asked for on that screen (activity every 3 s, diffstats every 15 s, the open task's changes and graph every 10 s each, shells every 5 s per sidebar row that read them). Count and total ms of long tasks in 60 s on the task table, with 50 new tasks and 3 fake agents at work. The agents' tasks are cancelled, their notes committed, and the tasks deleted after each test |
| `terminal-helpers.ts` | Shell tasks, panes, history, and timing in the page (`timeToShow`, `typeAndMeasure`). Times come from the page's clock (`event.timeStamp`, `requestAnimationFrame`), so the runner's delay does not count |
| `measures.ts` | `recordMeasure` writes `e2e/test-results/terminal-measures.json` and prints a `MEASURE` line. `writeSummary` adds a table to the job summary |
| `daemon-rss.ts` | Samples the memory (`VmRSS`) of the daemon process every 0.5 s through one `docker compose exec`. Its shells are not counted |

- Limits are checked on the median of 5 runs, not on one sample.
- Each test deletes its tasks, so their shells end.
- The flood tests wait at most 30 s for a prompt or an echo: with flow control these are quick, so a long wait is a failure.
- To compare two branches, download the measures from both runs: `gh run download <run-id> -p 'e2e-measures-*'`. Older runs have one `e2e-measures` artifact instead.

## Site captures

The website media is shot from a separate demo stack. See [scripts/site-capture/README.md](../scripts/site-capture/README.md).
