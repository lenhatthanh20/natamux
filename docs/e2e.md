# End-to-end tests

The production images, started with demo data, tested in a real browser with Playwright.

| Part | What it is |
|---|---|
| `compose.e2e.yaml` | On top of `compose.prod.yaml`: a fake agent (`scripts/fake-agent.sh`) in place of Claude Code, git names for commits, and a throwaway repository in a volume |
| `scripts/e2e.sh` | Starts the images, adds the demo data, stops them. Needs only Docker and curl |
| `e2e/` | The Playwright tests. Not part of the pnpm workspace, so the images do not install it |
| `.github/workflows/e2e.yml` | Builds the images, runs the tests, and on `main` publishes the images to ghcr.io |

## Demo data

`scripts/e2e.sh up` adds:

- **Demo: changes and history**: a workspace with two commits on a branch that diverges from its base, and changes that are not committed.
- **Command palette opened with Cmd+K**: a fake agent that keeps working in its terminal.
- Tasks that are ready, blocked, draft and done.

The dashboard is on http://localhost:8080 with the token `e2e-token`. Set `NATAMUX_PORT` or `NATAMUX_TOKEN` to change them.

## In CI

Every pull request and every push to `main` runs the workflow. It builds the images from the branch (`E2E_BUILD=1`), so the tests check the code of the pull request. When a test fails, the run keeps the Playwright report, with pictures, videos and traces, as the `e2e-report` artifact.

On `main`, after the tests pass, the images go to `ghcr.io/lenhatthanh20/natamux-daemon` and `natamux-web`, tagged `main` and `sha-<commit>`.

## On your machine

Needs Docker, for example colima. Nothing is built: the images come from ghcr.io.

```sh
scripts/e2e.sh up      # pull the images of main, start them, add the demo data
scripts/e2e.sh logs
scripts/e2e.sh down    # stop, and remove the demo data and the repository
```

Nothing is installed on your machine. `E2E_BUILD=1`, which builds the images and so installs packages, works only in CI or a codespace. The script refuses it elsewhere. The Playwright tests in `e2e/` run in CI only.

`E2E_TAG=sha-<commit>` picks the images of another commit of `main`. The tag `main` moves with every merge, so a `sha-` tag runs exactly the build you checked. The images are built for x86 Linux; on a Mac with Apple silicon, Docker runs them with Rosetta (colima: `--vz-rosetta`).

Then try the dashboard by hand or with `playwright-cli`:

```sh
playwright-cli open http://localhost:8080
playwright-cli localstorage-set natamux.auth e2e-token
playwright-cli reload
```

## Writing tests

- The tests share one daemon and run one after the other. A test that adds data gives it a name of its own, for example with `Date.now()`.
- `signIn(page)` in `e2e/tests/helpers.ts` puts the token in `localStorage`, so a test starts on the dashboard.
- The browser starts without WebGL, so the terminal draws its text into the page, where `.xterm-rows` shows it.
- `phone.spec.ts` runs with a phone screen; the other files with a desktop one.
- The images have no real agent. A test that needs one uses the fake agent of the demo data.
