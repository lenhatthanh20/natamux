# Site capture kit

Reshoots the media of the website (stills, clips, film) after a release.

## What it is

- A real dashboard: the attested `main` images of the daemon and web app, with real stages,
  branches, groups, send back, gates and shells.
- Stand-in agents: small scripts (`standins/`) that print short text and drive the real stages
  through the `natamux` API. They replace Claude Code, Codex, Gemini CLI, Copilot CLI and Cursor.
- Honesty note: keep this sentence next to every recording on the site and in any PR:
  "The dashboard and the stages are real. Only the agents' terminal text comes from a stand-in script."

## Folders

- This folder: stack helpers (`lib.sh`, `api.sh`, `new-task.sh`, `seed-storefront.sh`, ...), `overlay.js`,
  `agents-grid.js`, the two browser configs, `storefront.natamux.json`.
- `standins/`: the stand-in `agent`, `natamux` and the node `wrapper`.
- `video/`: recording bodies (`b-*.js`), `prelude.js`, `mk.py` and the encode scripts.
- Env vars: `OUT` (default `./out`), `DEMO_CONTAINER` (default `natamux-demo-daemon-1`),
  `DEMO_PORT` (default `8090`), `PW_SESSION` (default `demo`).
- Run everything from this folder: `cd scripts/site-capture`.

## 1. Start a separate demo stack

- Name it `natamux-demo`, port 8090. Do not build. Do not touch the e2e stack (`natamux-e2e`, 8080):
  never `down` it, never remove its volumes.
- Use the attested images of `main`. Check them first:
  `gh attestation verify oci://<image> --signer-workflow <owner>/natamux/.github/workflows/e2e.yml --source-ref refs/heads/main`.
- From the repo root:

```sh
export DOCKER_DEFAULT_PLATFORM=linux/amd64 NATAMUX_DAEMON_IMAGE=<daemon>@sha256:... NATAMUX_WEB_IMAGE=<web>@sha256:... \
  NATAMUX_PORT=8090 NATAMUX_TOKEN=e2e-token NATAMUX_REPO=e2e-repo
docker compose -p natamux-demo -f compose.prod.yaml -f compose.e2e.yaml run --rm repo
docker compose -p natamux-demo -f compose.prod.yaml -f compose.e2e.yaml up -d --no-build --wait --wait-timeout 240 daemon web
```

- `NATAMUX_REPO=e2e-repo` is required: `compose.prod.yaml` refuses to start without it.
- Result: dashboard at http://localhost:8090, token `e2e-token`, container `natamux-demo-daemon-1`.

## 2. Browser session

- Headed Chrome (`channel: chrome`), 1440x900 at DPR 2, WebGL off. Config: `cli.stills.config.json`
  (`cli.video.config.json` is the same).
- Why WebGL off: with it on at DPR 2, terminals opened later draw text twice too big.
- Why headed: it keeps screencasts crisp at 2880x1800. A 2880x1800 window did not hold its viewport.

```sh
playwright-cli -s=demo open --config=cli.stills.config.json http://localhost:8090
playwright-cli -s=demo localstorage-set natamux.auth e2e-token
playwright-cli -s=demo reload
```

- `api.sh` and `lib.sh` call the API from this page, so keep the session open.
- Switch theme with `playwright-cli -s=demo set-color-scheme light|dark`.

## 3. Stand-ins and the storefront project

```sh
./install-standins.sh   # run again after ANY daemon restart; only new shells use it
./seed-storefront.sh    # once on a fresh stack: repo, workflow, five tasks
```

- Stand-ins live in `/tmp` of the container (tmpfs), so a restart removes them.
- A second run of `seed-storefront.sh` adds duplicate tasks.
- `new-task.sh bug|ask|slow|gate ["title"]` makes a fresh task for a live clip.
  `AGENT=` picks the Shell 1 agent, `PACE=` the speed (default 10; bigger is slower).
  - `bug`: stops at "Ready for review: Spec". After approval the whole loop takes about 90 s.
  - `ask`: the agent asks a question in about 5 s.
  - `slow`: returns when the side by side group runs.
  - `gate`: a Feature task waiting at Spec.
- The task title or description must contain "bug" to take the Bug fix path.
- Review sends back once with a finding. Set the text with `FINDING=` or `finding <id> "text"`.
  `touch /data/standin/<id>.clean` in the container makes Review find nothing.

## 4. Stills

- Take device screenshots (2880x1800):
  `playwright-cli -s=demo run-code "async page => { await page.screenshot({path: '$OUT/x.png', scale: 'device'}) }"`.
- Agents view with chosen tasks: edit `keep` in `agents-grid.js`, then
  `playwright-cli -s=demo run-code --filename=agents-grid.js`. A reload resets it.
- Take each still in both themes from the same page state.
- Convert with `cwebp -q 82 -m 6 -resize <w> <h>` (or `ffm.sh`). Homepage stills are 2000x1250 and 1000x625.
- Targets:
  - Homepage: `site/public/media/v2/<name>-<theme>-{2000,1000}.webp` and `og.jpg` (1200x630).
  - Docs: `site/public/screens/v2/<name>-<theme>.webp` (1600x1000).

## 5. Clips and film

- Add `overlay.js` (big cursor, click ripple, focus zoom, captions) once per session, before the first page load.
  A newer `overlay.js` is ignored in an old session: close and reopen it.
- Build a run script from a body, then run it. The dark session clicks for real (lead 1);
  record the light session at the same time with lead 0, so it only mirrors.

```sh
export OUT=./out
sed "s#__OVERLAY__#$PWD/overlay.js#; s#__PORT__#8090#; s#__TASK__#T-0001#" video/setup.js > "$OUT/setup.js"
playwright-cli -s=vd run-code --filename="$OUT/setup.js"
playwright-cli -s=vd run-code --filename="$(video/mk.py b-branch.js branch-dark-1.webm 1)"
playwright-cli -s=vl run-code --filename="$(video/mk.py b-branch.js branch-light-1.webm 0)"
```

- Bodies and their placeholders (`KEY=VALUE` after the lead flag):
  - `b-branch.js`: project settings, If node, both paths.
  - `b-live.js` (`TASK`): approve Spec, Took Bug fix, side by side, send back, Manual review. Use a fresh `new-task.sh bug` task.
  - `b-ch1.js` (`TASK`): film chapter 1, project settings from the sidebar.
  - `b-agents.js` (`IDS`, space separated): five providers in the Agents view.
  - `b-ask.js` (`TASK`, `FROM`): a question arrives and is answered. `FROM` is any task page to start on.
- Each run prints the time marks. Use them for the cuts.
- Encode with Docker ffmpeg (`linuxserver/ffmpeg`, no install):
  - `video/clip.sh <out.mp4> <src.webm> <start s> <frames> <still start s> <still end s> <crf>`: one clip.
  - `video/cut.sh`: keep ranges of a take, join, encode.
  - `video/film.sh <theme> <crf> s1..s6`: six chapters to fixed frame counts (see its header).
  - `video/frames.sh`: contact sheet to review a take.
  - `ffm.sh <in> <out> [ffmpeg options]`: any other ffmpeg call.
- Posters: `ffm.sh in.mp4 poster.webp -ss <s> -frames:v 1 -vf scale=2000:-1 -c:v libwebp -quality 80`.
- Output: H.264 High, yuv420p, `+faststart`, no audio, 30 fps.

## 6. Where files go

- Clips, film, posters: `site/public/media/v2/` (`clip-<name>-<theme>.mp4`, `film-<theme>.mp4`, posters as webp).
- Docs screenshots: `site/public/screens/v2/`.
- Film chapters and duration: `site/src/data/film.json`. Update the chapter start seconds when the cuts change.

## 7. Budgets

- Film: about 5 MB per theme, about 39 s, 1920x1200.
- Clip: 0.5 to 2 MB, 10 to 14 s, 1600x1000.
- Poster: under 200 KB, 2000x1250 webp.
- Still: 2000 wide under about 170 KB, 1000 wide under about 70 KB.
- Light and dark of one clip or film use the same cuts and frame counts.

## Traps

- The daemon allows 30 open terminals. Every stage shell counts. A new shell fails with
  "At most 30 terminals can be open" until old ones end. Finish old tasks (approve Manual review, skip Release) to free them.
- The project `repo` (id `default`) is added by the daemon and shows as an empty card at the top of the sidebar.
  Collapse it (`natamux.sidebar-folds` is set in `video/setup.js`). Removing it is a deletion: only with the owner's OK.
- Do not approve Release: it pushes to `origin`, which the demo repos do not have. Skip it.
- Generate (commit message) does not work with stand-ins: it prints terminal text. Type the message, and never say Generate wrote it.
- Side by side lasts only about a minute at pace 60. Shoot right away, or use `PACE=150`.
  Pace is read when an agent process starts, so changing it later affects only new shells.
- Dark and light shots must be a pair: same page state, same framing, same task.
  Live terminals move on, so a line or two of terminal text can differ.
- The Agents pane choice lives in memory. A reload resets it.
- Every page load logs 3 console errors (CSP blocks two `data:` fonts). Harmless.
- Old shells keep their old program. After `install-standins.sh`, make new tasks.
- `curl` to localhost may be denied. Use `api.sh`.
