# Plan: move Natamux to Rust

Status: approved by the owner, 2026-10-22, with the switch to GPL-3.0-or-later. It reverses the "stay on Electron" advice in `research-native-app.md`; `plan-tech-stack.md` section 6 points here.

## Goals

1. **Smooth.** The app never drops a frame that a person can see, even with 10 agents printing at once.
2. **Beautiful.** It looks at least as good as Zed.

License is not a constraint (owner, 2026-10-22). That lets Natamux reuse Zed's GPL-3.0 crates, and Natamux becomes GPL-3.0.

## What we learned

### From Zed (`zed-industries/zed`)

- **GPUI** is a hybrid immediate and retained mode UI on the GPU. It builds one scene per frame and draws it in batches: rects with SDFs, closed-form shadows, glyphs from an atlas, shaped text cached between frames. It targets 120 fps, which is 8.33 ms per frame ([videogame post](https://zed.dev/blog/videogame)).
- **Frame timing** on macOS comes from a `CVDisplayLink` per display, so ProMotion and variable refresh rates work.
- **Threads:** the main thread only builds frames. Everything slow (IO, parsing, diffing) runs on the `BackgroundExecutor` ([async post](https://zed.dev/blog/zed-decoded-async-rust)).
- **Backends:** Metal (macOS), DirectX 11 (Windows), wgpu (Linux). There is also a `gpui_web` crate; its maturity is not confirmed.
- **Terminal:** `terminal` and `terminal_view` on a fork of `alacritty_terminal`.
  - Same-style cells are merged into text runs, and same-colour backgrounds into rects.
  - Only the visible part is laid out.
  - PTY events are batched (at most 100, a 4 ms timer).
  - Scrollback is 10,000 lines by default.
- **Client and headless server:** `remote_server` owns files, terminals and tasks. The GUI is a thin client that reconnects. This is the same shape as Natamux's daemon and dashboard.
- **Looks:**
  - `ui` crate: about 25 components with spacing, typography, elevation and colour styles.
  - `theme` crate with a JSON theme schema (`https://zed.dev/schema/themes/v0.2.0.json`).
  - Fonts: IBM Plex Sans and Lilex. About 300 SVG icons.
  - An animation API that respects `reduce_motion`.
- **Tests:**
  - `#[gpui::test]` with `TestAppContext` and `VisualTestContext`: deterministic scheduling and simulated keys and clicks.
  - A macOS screenshot test runner that compares against baseline images.
- **Weak points:**
  - Pre-1.0 API that changes often; thin docs.
  - IME bugs are still open, including Vietnamese typing that swallows words in a text box ([#64707](https://github.com/zed-industries/zed/issues/64707)) and a misplaced IME window in the terminal ([#64850](https://github.com/zed-industries/zed/issues/64850)).
  - No screen reader on Windows ([#41138](https://github.com/zed-industries/zed/issues/41138)).

### From ArtCraft (`storytold/*`, 13 apps in Rust, egui and wgpu)

Copy these:
- **Engine first:** a pure data model, one registry of commands, and a thin UI. The UI, CLI, JSON control channel and MCP server all call the same commands.
- **Layers in the Cargo workspace,** checked by `cargo xtask layers`: nothing below the UI crate may use the UI framework.
- **An `AGENTS.md` with hard rules:**
  - No `unwrap`, `expect` or `panic` outside tests (clippy deny).
  - Every crash fix gets a regression test.
  - Widgets read theme tokens, never hard-coded colours.
  - Check screenshots at several window sizes and in every theme.
- **A loopback control channel,** so an agent can drive the UI and take screenshots.
- **Design rules:**
  - A `Tokens` struct; contrast 4.5:1 checked by unit tests.
  - Lucide icons (Natamux already uses `lucide-react`).
  - Long labels wrap or elide with a tooltip; they never overflow.
- **CI:** a nightly performance workflow.

Do not copy these:
- **egui:** IME is their weak spot. Every app has open Japanese, Korean and Chinese IME bugs, and they patch forks of winit and egui. Natamux is text-heavy and Vietnamese-first, so egui is ruled out.
- **Breadth before polish:** reviewers found the apps shallow and memory-heavy (5 GB in one review). Our phases ship fewer screens, finished.

## Target numbers

The targets are absolute; the Electron app is not measured (owner, 2026-10-22). Every phase must meet them on a 14" MacBook Pro (ProMotion):

| Measure | Target |
|---|---|
| Frame time while scrolling the task list or a terminal, 10 agents printing | p99 ≤ 1 refresh (8.33 ms at 120 Hz, start jitter allowed), p99.9 ≤ 2 refreshes, as the mean of at least 4 runs (owner, 2026-10-22; see `rust-phase1.md`, gate 2) |
| Key press to glyph on screen in a terminal | p95 ≤ 20 ms, of which the app's part p95 ≤ 17 ms (owner, 2026-10-22). At 120 Hz the measure always holds 8.33 to 16.7 ms of display wait, so 16 ms was below its floor; see `rust-phase1.md`, gate 2 |
| Heavy output: 10 terminals at 1 MB/s each | UI frames stay in the first row |
| Cold start to the first frame | ≤ 300 ms |
| Cold start to the dashboard with data | ≤ 1 s |
| Download size (macOS dmg) | ≤ 60 MB |
| Vietnamese input (Telex and VNI with the macOS input sources, plus EVKey/OpenKey if installed) | 100% of a fixed word list typed right, in a terminal and in every text box |

"Beautiful" is checked by a person and by tests:
- Side-by-side screenshots against Zed at the same size: spacing, type scale, borders, elevation, focus rings, empty states.
- Screenshot baseline tests for every screen, in light and dark themes.
- The contrast test.
- The owner signs off each screen before it ships.

## Architecture

Keep the split Natamux has today, as Zed does with `remote_server`: one headless daemon owns PTYs, git, agents and stages, and thin clients talk to it.

```
natamux-app (GPUI, desktop)   web dashboard (React, browser and phone)   natamux CLI
            \                          |                                   /
             +-------- HTTP + WebSocket API, unchanged (/api, /events, /tasks/:id/terminal) --------+
                                       |
                              natamux-daemon (Rust)
                 PTYs · screen state · tasks · stages · agents · git · GitHub
```

- **The HTTP and WebSocket contracts stay byte for byte.** The routes, the `/events` kinds, the terminal frames, close codes, `ticket`, `ack` and `INPUT_GAP_MS = 25` are listed in "Contracts to keep" below. Because of this, the React dashboard (browser, Codespaces, phone), the CLI and the agent hooks keep working through every phase.
- **On-disk data stays the same:** `tasks/<ID>.md`, `<ID>.events.jsonl`, the shell files, `repos.json`, and the `index.sqlite` cache. Users need no migration.
- **The web dashboard stays React.** GPUI does not run in a browser yet. The phone plan (PWA over Tailscale) depends on it.
- **The desktop terminal** receives the byte stream over the terminal WebSocket and feeds a local `alacritty_terminal::Term` drawn by Zed's `terminal_view`. Zed's `terminal` crate expects a local PTY, so it needs an adapter for a remote byte source. The phase 1 spike proves this.

### Cargo workspace

| Layer | Crate | Job | Uses |
|---|---|---|---|
| L0 | `natamux-core` | Task, event, stage and `.natamux.json` types; validation with the same error messages as the zod schemas | `serde`, `serde_json`, `schemars` |
| L0 | `natamux-proto` | HTTP bodies, `/events` kinds, terminal frames | `serde` |
| L1 | `natamux-store` | Markdown + YAML front matter files, JSONL logs, the SQLite index, atomic writes | `serde_yaml`, `rusqlite` |
| L1 | `natamux-git` | Worktrees, branches, changes, diffs, graph, commit, push, rollback; `gh` calls | `tokio::process` (runs `git` and `gh`, as today) |
| L1 | `natamux-pty` | Spawn, resize, kill the process tree, ConPTY; the Vietnamese input gap | `portable-pty` |
| L1 | `natamux-screen` | Headless screen per shell and a snapshot the xterm.js web client can replay | `alacritty_terminal`, `vte` |
| L1 | `natamux-agents` | How each agent starts (commands, permission flags, hook files, trust) and the skills it finds (part 3); adapters and hooks for Claude Code, Codex, Gemini CLI, Copilot, Cursor; usage and cost (part 4) | |
| L1 | `natamux-stages` | Stage logic with no daemon service: checks, branch conditions, prompts, stage files, the git and gh calls of stages, the CI wait trait. The runner that uses it stays in `natamux-daemon`, as it drives the daemon's services (part 3) | |
| L2 | `natamux-daemon` | HTTP and WebSocket server, auth (bearer, agent token, ticket), serves the web build | `tokio`, `axum`, `subtle`, `rust-embed`, `notify`, `tracing` |
| L3 | `natamux-client` | Typed API client with reconnect and a live cache from `/events` | `reqwest`, `async-tungstenite` |
| L4 | `natamux-app` | The desktop app | `gpui`, Zed's `ui`, `theme`, `workspace`, `terminal_view`, `editor`, `buffer_diff`, `git_ui`, `markdown`, `picker` |
| L4 | `natamux-cli` | `natamux` and `nmx`, including the fast hook forwarder | `clap`, `ureq` |
| L4 | `natamux-update` | Self-update with the pinned minisign key | `minisign-verify` |

- `cargo xtask layers` fails if a crate below L4 uses `gpui`.
- Every user action is a command in one registry (ArtCraft): menus, shortcuts, the palette, the CLI and a loopback control channel all call it. Agents and tests then drive the app the same way people do.
- Pin Zed crates and `gpui` to one Zed git rev. Upgrade on a schedule (once a month), in its own PR, with the screenshot tests as the gate.

### Smooth by construction

- The main thread only builds frames. PTY reads, parsing, diffing, git and file IO run on background executors or in the daemon.
- Keep the 4 ms output coalescing and the 64 KB ack window that the terminal protocol already has. Batch PTY events in the app as Zed does.
- Only draw what is on screen:
  - virtual lists (`uniform_list`) for tasks, events and files;
  - only visible terminals render;
  - hidden shells keep state but no layout.
- Cache shaped text, and build a frame only when something changed.
- A frame-time and input-latency overlay in debug builds (Zed has `input_latency_ui`), and a nightly perf job in CI that fails when the target numbers regress.

### Beautiful by construction

- **Design system: Zed's `ui` kit and `theme` crate** (owner decision, 2026-10-22). Do not mix in another kit such as gpui-component: two styles would show. A missing component is built in `natamux-app` from `ui` parts, in Zed's style.
- Write Natamux's brand into a Zed-format theme family, with light and dark:
  - navy `#1B2A4A` → `#0D1526`;
  - amber `#F2B544` for "Needs you";
  - mint `#58D3A6` and sky `#7FA8FF`.
- **Port the terminal palette** in `docs/design-terminal-look.md`.
- **Fonts:** keep Natamux's UI font (Inter) and terminal font (JetBrains Mono). Use Zed's type scale and spacing scale.
- **Icons:** Lucide. Natamux's own screens (stage cards, progress, Needs you strip) are built only from `ui` components and theme tokens.
- **Motion:** one moment per action (a stage moves, a card expands), using GPUI animations that respect `reduce_motion`.
- **Tests:** the screenshot baseline test for every screen and both themes, and a contrast test on every token pair.

## Phases

Each phase ends with a release, and the gate must pass before the next phase starts. Electron keeps shipping until phase 5; during the move it gets only bug fixes.

Branches (owner, 2026-10-22):
- `dev` holds the Rust work. Every Rust PR targets `dev`. It was made from `main` at e93dba6.
- `main` stays the Electron app and keeps its releases from `scripts/release.sh`.
- Merge `main` into `dev` after each Electron release, so fixes reach both.
- `dev` merges into `main` only at the phase 5 cut over.

### Phase 0: decide and prepare (1 week)

- Owner approves this plan and the GPL-3.0 switch.
  - Update `LICENSE`, README, site, `plan-tech-stack.md` and `research-native-app.md`.
  - `scripts/readme.test.py` checks for "MIT License" and must change with it.
- Shoot the UI reference: every screen and state of today's app, light and dark, in `docs/ui-reference/`. Each shot links to its `ui-spec.md` section and its React component. The native screens copy layout and behaviour from it; the looks come from Zed.
- Set up Rust safely (see "Safe local setup"):
  - `rust-toolchain.toml` pinned;
  - `cargo deny`, `cargo audit` and `cargo vet` in CI;
  - Dependabot for Cargo with a 7-day cooldown.
- Add `AGENTS.md` rules for Rust (from ArtCraft):
  - no panics outside tests;
  - regression test per bug;
  - tokens only;
  - screenshots in both themes;
  - `cargo xtask ci` = fmt, clippy, tests, layers.

### Phase 1: spike, go or no-go (2–3 weeks)

Closed 2026-10-22 (owner): every gate passed (`rust-phase1.md`, "Gate results"). Left from it: update wasmtime 48.0.7, rustls-webpki and webpki-roots on or after 2026-10-28, past their cooldown.

How to get the build, what the spike found and how to run the gates: `rust-phase1.md`.

- Build a GPUI app with a task list and one live agent terminal, against today's Node daemon, over the real WebSocket protocol (hello, snapshot, resume, ack).
- CI builds the app (see "Safe local setup"):
  - A job builds the macOS arm64 app on every PR into `dev`, uploads it as an artifact and attests it with `actions/attest-build-provenance`. The owner runs this build; nothing is built on the owner's Mac.
  - First job of the phase: check that GPUI draws on a GitHub macOS runner (Metal in the runner VM). If it does not, screenshot tests run on the Windows (DirectX through WARP) and Linux (wgpu on software Vulkan) runners, and macOS looks are checked by hand on the CI build.
- Gates 1, 2 and 5 need a person and a real display, so they run on the owner's Mac with the CI build. CI covers gate 4 and the automated tests.
- Gate, all must pass:
  1. Vietnamese Telex and VNI pass the word list in the terminal and in a text box on macOS. If not, fix it in a GPUI fork and upstream the fix; if that fails, stop the move.
  2. 10 terminals at 1 MB/s each meet the frame and latency targets, measured on the 14" MacBook Pro.
  3. The Zed `terminal_view` works on a remote byte stream.
  4. A Windows build starts and types.
  5. The owner uses it side by side with Electron for one day and says it is clearly better.

### Phase 2: CLI in Rust (2 weeks)

Closed 2026-10-22 (owner): the gate passed (`rust-phase2.md`, "Gate results").

How to get the build, what CI checks, where it differs and the gate results: `rust-phase2.md`.

- `natamux` / `nmx` with every command of `packages/cli`, the same output, and the hook forwarder (no token → no-op, never fails the agent, 3 s timeout).
- Gate: the CLI unit cases ported as Rust tests; agent hook E2E specs (`needs-input`, `shell-agent`, `ask-agent`) green with the Rust CLI on the agent PATH.

### Phase 3: desktop app on the Node daemon (8–12 weeks)

Started 2026-10-22 (T-0146). Decisions, details, the beta and the gate results of screen 1, screen 2 (T-0148), screen 3 (T-0149), screen 4 (T-0150), screen 5 (T-0151), screen 6 (T-0152) and screen 7 (T-0154): `rust-phase3.md`.

Decisions (owner, 2026-10-22):

- The first task is the shared foundation and screen 1 only. Screens 2 to 7 each get their own task and spec.
- Beta channel: a GitHub pre-release `native-beta.N` made by hand from a green run of `dev` (`.github/workflows/native-beta.yml`), with the attested CI build. macOS arm64, unsigned until the Apple account exists. `scripts/release.sh` and the releases of `main` do not change.
- Fonts (Inter, JetBrains Mono) and the Lucide icons that a screen uses live in `crates/natamux-app/assets`, with their licenses. No new crate.
- A screen's screenshot baselines come from a CI run's artifact, after the owner signs the screen off. The compare step only reports until then.

- Screens in this order. Each one ships when done, behind a "native app (beta)" download next to the Electron one:
  1. Shell, sidebar, task list and board.
  2. Workspace with agent terminals and shells.
  3. Needs you strip, stage card, banners and gate actions.
  4. Changes: diff (`editor`, `buffer_diff`), git graph, rollback, commit, push, PR.
  5. Stages: overview, progress, pipeline editor.
  6. Files and Docs.
  7. Settings, project settings, command palette, sign in.
- Gate per screen: target numbers met, screenshot baselines in both themes, owner sign-off, and every state in `docs/ui-reference/` covered with the same behaviour as the React screen (same commands, same API calls).

### Phase 4: daemon in Rust (10–14 weeks)

Part 1 (terminals and the contract suite, T-0153), part 2 (tasks, events, storage, index, T-0155) and part 3 (stage engine and agent start, T-0157): decisions, crates, what CI checks, where it differs, ported tests and gate results in `rust-phase4.md`.

- Same API, same files. Order by risk:
  1. Terminals: PTY, input gap, ring, snapshot, resume, backpressure, restore after restart.
  2. Tasks, events, storage, index.
  3. Stage engine.
  4. Agent adapters and hooks.
  5. Git, workspaces, fan-out, GitHub watcher, commit drafter.
- Port the daemon's unit tests (about 1,560) as Rust tests; they encode the edge cases. Start with `terminal-service`, `stage-runner`, `agent-hook-service`, and the signal tests.
- Add a black-box HTTP and WebSocket contract suite that runs against both daemons and compares answers.
- Gate:
  - the E2E suite green with the Rust daemon image swapped in (`NATAMUX_DAEMON_IMAGE`), mainly `terminal-protocol`, `terminal-cache`, `terminal-end`, `live-push`, `tasks`, `stages`, `branches`, `workflow`, `resume`, `needs-input`, `commit-draft`, `rollback`, `workspace`;
  - the contract suite equal on both daemons;
  - bench numbers better than Node.

### Phase 5: cut over (2–3 weeks)

- Ship two Rust binaries (app and daemon) with the web build embedded.
- Updater in Rust with the same pinned key (`RELEASE_SIGNER`) and trusted comment `natamux <tag>`.
- `release.yml` builds dmg (arm64, x64), Windows installer, AppImage, deb, tar.gz.
- `SHA256SUMS.txt`, owner signing with `sign-release.sh` and the `natamux-release` skill stay as they are.
- The Docker daemon image becomes the Rust binary.
- Remove Electron and the Node daemon in their own PR, after one release where both ran fine.

Rough total: 6–8 months for one developer with agents. These are estimates, not measurements; phase 1 sets the real pace.

## Contracts to keep

From `origin/main` (0.1.43). The Rust daemon and clients must keep all of these:

- **HTTP routes** in `packages/daemon/src/http/routes/*.ts`. The web app calls them under `/api`, and the daemon strips that prefix.
- **Auth:**
  - Bearer token compared in constant time; `?token=` on `/events`.
  - A one-time terminal `ticket` (30 s).
  - The agent token cannot send gate events (review.approved, stage.approved, stage.passed, stage.skipped, state.overridden).
  - Writes wait for the login PATH.
- **`/events` kinds:** `task.created`, `task.updated`, `task.changed`, `task.deleted`, `repos.changed`, `activity.changed`, `shells.changed`, `workspace.dirty`, `jobs.changed`, `workspace.preparing`, `usage.changed`, `resync`. Above 1 MB buffered, skip messages and send one `resync`.
- **Terminal socket:**
  - Query: `ticket`, `cols`, `rows`, `terminal` (1–999), `visible`, `shellId`, `offset`.
  - Client to daemon: `input`, `resize`, `visible`, `ack`, `ping`.
  - Daemon to client: binary output; `hello`, `snapshot`, `resume`, `replayed`, `pong`, `exit`, `error`.
  - Close codes 4401, 4409, 4410.
  - Limits from `terminal-limits.ts`: 30 sessions, 4 ms coalescing, 64 KB frames and ack, 1 MB ring, ping every 15 s with a close after 90 s of silence, `INPUT_GAP_MS = 25`, `ENTER_AFTER_PASTE_MS = 100`.
- **`.natamux.json`:**
  - The schema in `core/src/projects/project-settings.ts` and `core/src/stages/schema.ts`, with the cross-rules in `pipeline-schema.ts`.
  - The error texts the user sees.
  - The default pipeline.
- **Files** under `NATAMUX_DATA_DIR`, and `.natamux-task/` in each worktree.
- **Env vars:**
  - Daemon: `NATAMUX_PORT` (4317), `NATAMUX_HOST`, `NATAMUX_TOKEN`, `NATAMUX_DATA_DIR`, `NATAMUX_REPO_DIR`, `NATAMUX_WORKTREES_DIR`, `NATAMUX_WEB_DIR`, `NATAMUX_BIN_DIR`, `NATAMUX_<AGENT>_COMMAND` and `_ARGS`.
  - Set in task shells: `NATAMUX_TOKEN`, `NATAMUX_URL`, `NATAMUX_TASK_ID`, `NATAMUX_TERMINAL`.
- **Agent integration:**
  - Claude Code hooks through `--settings`, and the command-hook file for Copilot.
  - Codex `-c hooks.*` args; the Gemini settings file; the Cursor hook file.
  - The permission flags per agent.
  - The `plugins/natamux` skill.

## Safe local setup

Rule: **no `cargo` command ever runs in the macOS account that holds the signing keys.** Build scripts and proc macros run code at build time with the user's rights. In August 2026, `arrayref`, `internment` and `append-only-vec` shipped a build-time payload that stole browser credentials and installed a LaunchAgent on macOS ([JFrog](https://research.jfrog.com/post/arrayref-proc-macro1-crates-io/)).

The owner's Mac is a company Mac (owner, 2026-10-22): MDM profiles, a VPN and endpoint security apply to every user on it. A second macOS user keeps files and keychains apart, but shares the device profiles and the VPN, so a bad crate in it could reach the company network. So on this Mac:

- **Default: CI builds, the owner runs them.** CI builds, tests and attests; the owner downloads the artifact, runs `gh attestation verify`, then runs the app. No build script or proc macro runs on the Mac.
- Running the app still runs the runtime code of every crate in it, as the Electron app does with npm packages today. `cargo vet`, `cargo deny` and the 7-day cooldown lower that risk; they do not remove it.
- The app is unsigned until the Apple account exists. After `gh attestation verify`, clear the quarantine flag (`xattr -dr com.apple.quarantine Natamux.app`). If MDM blocks unsigned apps, ask IT before going further.
- Optional: Docker for headless crates, with the hardened command below. The run step has no network, so the VPN is out of reach.
- `natamux-dev` user: only on a personal Mac, or on the company Mac after IT says yes.

| Where | What runs there | Keys |
|---|---|---|
| Owner account (today's login) | 1Password, `release.sh`, `sign-release.sh`, `gh attestation verify`, running verified CI builds | minisign key, SSH tag key, 1Password |
| GitHub Actions | All builds and checks, the app artifact per PR (phase 1 on), release builds, `attest-build-provenance` | none of the owner's keys |
| Docker, `rust@sha256:<pinned>` (optional) | `cargo test` of headless crates (L0–L3) and `xtask` | none |
| `natamux-dev` macOS user, standard, not admin (personal Mac, or company Mac with IT's OK) | Xcode, rustup, `cargo build` / `cargo run` of the app with full Metal, Claude Code sessions that write Rust | none: no 1Password, no iCloud, no SSH signing key, own GitHub token with repo scope only |

Docker command. The repo is mounted read only: a build script that can write to it could plant a git hook or script that later runs in the owner account. Build output and the crate cache live in Docker volumes.

```sh
img=rust@sha256:4cd829461bd5c4d511c32e269da9cb8929223b666519d8004e35fc8d1d771ab7 # rust:1.98.1-slim
common=(--rm --user "$(id -u):$(id -g)" --cap-drop ALL --security-opt no-new-privileges
  -v "$PWD":/w:ro -w /w -v natamux-cargo-registry:/usr/local/cargo/registry -v natamux-rustup:/usr/local/rustup
  -v natamux-cargo-target:/target -e CARGO_TARGET_DIR=/target)
# Downloads only: crates, and the rustfmt and clippy components. Nothing is built while the network is on.
docker run "${common[@]}" "$img" cargo fetch --locked
# natamux-app needs a GPU and the GUI libraries: it builds in CI only.
docker run "${common[@]}" --network none "$img" cargo test --workspace --exclude natamux-app --locked --offline
```

Pin the image of the same Rust version as `rust-toolchain.toml`, or rustup downloads a whole toolchain. The slim image has no rustfmt or clippy; the fetch step adds them to the `natamux-rustup` volume, so the test step needs no network. Create the three volumes once and give them to the same uid (`docker run --rm -v natamux-cargo-registry:/r -v natamux-cargo-target:/t -v natamux-rustup:/u alpine chown -R "$(id -u):$(id -g)" /r /t /u`). colima mounts the home folder into its VM by default; a container escape would reach it, so this is a lower layer of safety than CI.

Setup steps (each needs the owner's OK, because they install software):

1. On the company Mac: nothing to install for the default path. Check how far it is managed (`profiles status -type enrollment`, `sudo profiles show -type configuration`, `scutil --nc list`) before any optional step.
2. Only where a `natamux-dev` user is allowed:
   - Create it as a standard user. Keep `chmod 700` on the owner's home, `~/.minisign` and `~/.ssh`: every local user is in the `staff` group. Do not share folders between the two users.
   - In `natamux-dev`: install Xcode (GPUI needs the Metal shader compiler from full Xcode, not only the Command Line Tools), then rustup. The repo's `rust-toolchain.toml` pins the version.
   - Turn the VPN off while it builds, if the VPN allows it.
3. Repo hygiene in CI:
   - commit `Cargo.lock`; build with `--locked`;
   - `deny.toml` with `unknown-registry = "deny"`, `unknown-git = "deny"`, and an allow list of crates.io and `github.com/zed-industries/zed`;
   - `cargo audit`;
   - `cargo vet` with audits imported from Mozilla, Google and the Bytecode Alliance.
4. Dependency updates: through Dependabot only, after a 7-day cooldown, one reviewed PR at a time. The 2026 bad versions were live for under two hours.
5. Releases: the owner downloads CI artifacts only, runs `gh attestation verify`, then signs as today. `sign-release.sh` already re-checks every file against the run's artifacts.
6. If a bad crate ever ran in the owner account: rotate the minisign key, the SSH tag key and 1Password items, and treat the Mac as compromised.

A macOS VM (Tart, UTM) gives stronger isolation, but GPU access in guests is limited and GPUI rendering there is not confirmed. Use it only after a test. A personal Mac without MDM and the company VPN is the clean long-term answer for UI work.

## Risks

| Risk | Plan |
|---|---|
| Vietnamese IME in GPUI | Phase 1 gate. Fix in a fork and upstream; stop the move if it cannot be fixed |
| GPUI and Zed crates change without notice | One pinned rev, monthly upgrade PR, screenshot tests as the gate |
| Zed's `terminal` assumes a local PTY | Phase 1 gate (item 3); adapter crate if needed |
| xterm.js snapshot compatibility (web clients replay a snapshot) | `natamux-screen` writes the same escape sequence format; `terminal-cache` E2E specs as the gate |
| Long feature freeze | Electron gets fixes only; the native app ships screen by screen as a beta |
| Two UIs to keep (GPUI desktop, React web) | Same commands and API in both; new features land in the API first; the web stays the phone and browser UI |
| Windows maturity (no screen reader in GPUI on Windows) | Windows build in every phase gate; keep Electron for Windows until parity |
| GPL-3.0 switch | Owner decision in phase 0 |
| Slow UI loop: push, wait for the CI build, download, run | Cache the cargo registry and `target/` in CI; build only macOS arm64 per PR. If it still slows phase 1 down, move UI work to a personal Mac with a `natamux-dev` user |
| Unsigned app blocked by the company MDM | Ask IT before phase 1; signing comes with the Apple account |

## Sources

- Zed: [videogame](https://zed.dev/blog/videogame), [async](https://zed.dev/blog/zed-decoded-async-rust), [Windows](https://zed.dev/blog/windows-progress-report), [remote development](https://zed.dev/docs/remote-development), [themes](https://zed.dev/docs/extensions/themes), [GPUI README](https://github.com/zed-industries/zed/blob/main/crates/gpui/README.md), [Cargo.toml](https://github.com/zed-industries/zed/blob/main/Cargo.toml), [awesome-gpui](https://github.com/zed-industries/awesome-gpui), [gpui-component](https://github.com/longbridge/gpui-component).
- ArtCraft: [apps](https://getartcraft.com/apps), [PhotoCraft](https://github.com/storytold/photocraft) (README, AGENTS.md, docs/ui-design.md, docs/releasing.md), [craft-libs](https://github.com/storytold/craft-libs), [OMG! Ubuntu review](https://www.omgubuntu.co.uk/2026/10/artcraft-free-photoshop-ai-clone), [HN](https://news.ycombinator.com/item?id=50004284).
- Supply chain: [rustdecimal](https://blog.rust-lang.org/2022/05/10/malicious-crate-rustdecimal/), [faster_log](https://blog.rust-lang.org/2025/09/24/crates.io-malicious-crates-fasterlog-and-asyncprintln), [arrayref 2026](https://research.jfrog.com/post/arrayref-proc-macro1-crates-io/), [cargo deny](https://embarkstudios.github.io/cargo-deny/checks/index.html), [cargo vet imports](https://mozilla.github.io/cargo-vet/importing-audits.html), [artifact attestations](https://docs.github.com/en/actions/concepts/security/artifact-attestations), [VM GPU limits](https://cua.ai/blog/gpu-passthrough-macos-vms), [Zed macOS build](https://github.com/zed-industries/zed/blob/main/docs/src/development/macos.md).
- Natamux facts: `origin/main` at 0.1.43, file paths as cited.
