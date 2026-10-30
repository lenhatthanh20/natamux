# Plan: move Natamux to the MIT license

Status: draft for the owner, 2026-10-30. Work goes into draft PR #542 (branch `feat/mit-license`, into `main`). The open decisions D1-D35 were decided on 2026-10-30 by an agent the owner asked to choose; D36 was added in the review round of the same day. All are in section 4. Section 13 says how each review point was handled.

Paths are relative to the repo root. `zed:` paths are in the Zed fork at rev 69c8133 (`github.com/lenhatthanh20/zed`, the rev in `Cargo.toml:54-71`). `app/...`, `terminals/...` and other short paths are under `crates/natamux-app/src/`.

## 0. Short answer

- Natamux can move to MIT. The owner is the only copyright holder (section 3.7). The blockers are 14 Zed crates the app uses directly, which are GPL, plus Zed's icon files.
- Zed's `gpui`, `gpui_platform` and `util` stay. They and everything they reach are Apache-2.0: 21 Zed crates, 0 GPL.
- There is no partial win: the license can change only when the last GPL crate leaves. So all the work lands as commits in PR #542, and the license switch is the last step.
- We write Natamux's own terminal view, text input, code view, diff view, Markdown view, UI kit and theme. `alacritty_terminal`, `tree-sitter`, `imara-diff`, `pulldown-cmark` and `palette` do the heavy work. Every crate we need is already in `Cargo.lock`.
- Proof:
  - Tests written on the old code first. They must pass on the old code and on the new code.
  - Golden tables made from the old code on CI.
  - An exact pixel compare of every shot after every UI step. The only planned pixel change is the icons (section 6.9, D2).
- Order: gate and tests, terminal, text input, code and Markdown and diff, UI kit, theme and settings, assets, license switch.

## 1. Goal and owner priorities

- Goal: Natamux depends on no GPL code, so it can switch from GPL-3.0-or-later to MIT.
- Priorities, in this order:
  1. The terminal has zero new bugs. This is the most important.
  2. The task stage workflow logic has zero new bugs.
  3. The UI looks exactly the same, to the pixel, and feels as smooth (UI/UX first).
- When a priority and speed of work conflict, the priority wins. When a test cannot prove equality, add a test or a shot state; do not guess.

## 2. Hard rules for every implementing agent

- Builds and tests run only on GitHub Actions CI. Never run `cargo`, `rustc`, `rustup`, `npm`, `pnpm`, `npx`, `node`, `make`, or any build, test or install command on this machine.
- Allowed locally:
  - reading files, `grep`/`find`/`wc`;
  - read-only `git` and `gh` commands, plus `git commit`/`git push` of this branch and of the `natamux-ci/*` branches named in this plan (pushes to `natamux-ci/*` only after D26 says yes);
  - `python3 -I` for small analysis scripts, `scripts/shots-exact.py`, `scripts/zed-lock-check.py`, `scripts/npm-licenses-check.py` (and their tests) and `scripts/readme.test.py`;
  - `rustfmt` inside a Docker container that already has it. No local image has it today (section 8.3). Never install anything, including `rustup component add` (D25).
- One PR: #542. All commits go on `feat/mit-license`. Merge style is a merge commit; never squash.
- No release notes and no changelog entry in this PR (`scripts/release.test.sh` fails CI). The license change goes into the next release PR.
- The new code is Natamux's own:
  - Read Zed code only to learn behaviour and exact values (sizes, colours, timings).
  - Never copy GPL code, comments, query files, icons or structure line by line.
  - For the terminal and the text input, write from this plan, the gpui API, alacritty and public specs, with the Zed files closed. The tests and golden tables prove equality.
  - Apache-2.0 or MIT code (gpui, alacritty, tree-sitter queries) may be used as a dependency or copied with its license notice kept.
- Comments: simple English, in the style of the code around them. Commit messages and the PR text: simple English, with no AI attribution line (owner rule for Natamux).
- Never use the two product names the owner banned for this work (the orchestrator's brief names them). `docs/plan-features.md:186-188` holds one of them: do not copy wording from there.
- File names: the local path rule blocks any path with "key" or "token" in it. Never create such a file (use names like `input_bytes.rs`, `press_table.json`, `shortcut_hint.rs`), and never read one. Zed's `zed:crates/terminal/src/mappings/keys.rs` and `zed:crates/ui/src/components/keybinding*.rs` stay unread. The golden tables cover their behaviour.
- Do not change other crates (`natamux-proto`, `-core`, `-stages`, `-daemon`, `-client`, `-store`, `-screen`, `-pty`, `-agents`, `-git`, `-contract`, `-cli`, `-update`). Do not change the workflow rules files listed in section 6.3.9. Check before each push: `git diff --name-only origin/main...HEAD -- crates/ ':!crates/natamux-app' ':!crates/natamux-ui' ':!crates/natamux-theme'` must print nothing. Section 6.3.9 names the exceptions.
- Deleting anything (files, lines, config entries, branches) needs the owner's yes first. The plan lists each planned removal so the owner can confirm it once per step (D33).
- Every commit leaves the branch compiling, formatted, clippy-clean (`-D warnings`) and green on CI. Remove unused imports and dependencies in the same commit.

## 3. License audit

### 3.1 Zed crates in `Cargo.lock` (106)

- 74 are `license = "GPL-3.0-or-later"`.
- 2 have no `license` field but only a `LICENSE-GPL` file: `grammars`, `language_core`. Treat them as GPL.
- 30 are `Apache-2.0`.
- Note: `text` is GPL at this rev (`zed:crates/text/Cargo.toml:6`), whatever older notes say.
- GPL list: agent_settings askpass assets breadcrumbs buffer_diff client clock component context_server credentials_provider dap db edit_prediction_types editor env_var extension feature_flags feature_flags_macros file_content file_icons fs fuzzy fuzzy_nucleo git git_hosting_providers icons language language_detection language_model language_model_core lsp markdown menu mermaid_render migrator multi_buffer net node_runtime paths prettier project proto release_channel remote rope rpc session settings settings_content settings_json settings_macros snippet snippet_provider sqlez sqlez_macros syntax_theme task telemetry telemetry_events terminal terminal_view text theme theme_settings ui ui_input ui_macros vim_mode_setting workspace worktree zed_actions zed_credentials_provider zed_env_vars zeta_prompt, plus grammars and language_core.

### 3.2 Natamux's direct Zed dependencies (`crates/natamux-app/Cargo.toml:8-33`, `Cargo.toml:54-71`)

| Goes (GPL) | Replaced by |
|---|---|
| `terminal`, `terminal_view` | own terminal, `terminals/term/` (6.2) |
| `editor`, `text`, `language`, `grammars`, `buffer_diff`, `markdown` | own text input, code view, highlighter, diff view, Markdown view (6.3-6.6) |
| `ui` | own UI kit (6.7) |
| `theme`, `theme_settings`, `settings`, `release_channel` | own theme and appearance (6.8) |
| `assets` | own asset table, fonts and icons (6.9) |

| Stays (Apache-2.0) | Note |
|---|---|
| `gpui` (dev: `test-support`) | |
| `gpui_platform` | keep features `["font-kit", "wayland", "x11"]` |
| `util` | used for `PathStyle`; drop the `debug-embed` feature once `grammars` is gone (its comment says it is for grammar queries, `Cargo.toml:70-71`) |

### 3.3 What stays from Zed

- The closure of `gpui` + `gpui_platform` + `util` is 21 crates in the lock: collections, derive_refineable, gpui, gpui_apple, gpui_linux, gpui_macos, gpui_macros, gpui_platform, gpui_shared_string, gpui_util, gpui_web, gpui_wgpu, gpui_windows, http_client, path, refineable, scheduler, sum_tree, util, ztracing, ztracing_macro.
- All are Apache-2.0. All dependency kinds were checked: normal, build, target, optional and dev. The dev-only and optional crates (bench_metrics, perf, util_macros, zlog, reqwest_client, http_client_tls) are Apache-2.0 too.
- Zed has no NOTICE file. `LICENSE-APACHE` starts with "Copyright 2022 - 2025 Zed Industries, Inc.".
- Fork changes, checked with `gh api repos/lenhatthanh20/zed/compare/v1.24.2...69c8133`:
  - Code changes are only in `crates/terminal/src/terminal.rs` and `crates/terminal_view/src/terminal_view.rs`. Both go.
  - Of the kept crates, only `crates/gpui_linux/Cargo.toml` changed: the `xim` git URL points to the owner's fork. `crates/gpui_web/examples/hello_web/Cargo.toml` changed the same way.
  - The source of every kept crate is unmodified. The notices say so (Apache-2.0 section 4(b)).

### 3.4 Non-Zed crates and forks

- Kept forks (license read from each fork's manifest):
  - alacritty_terminal @4c12966, Apache-2.0.
  - zed-font-kit, MIT OR Apache-2.0.
  - zed-scap, MIT.
  - xim crates, MIT.
  - calloop, MIT.
  - async-process and async-task, Apache-2.0 OR MIT.
  - async-tar, MIT/Apache-2.0.
  - notify, CC0-1.0.
  - proptest, MIT OR Apache-2.0.
  - wasm_thread, Apache-2.0 OR MIT.
- tree-sitter (fork @43623ec, MIT) and the grammar crates stay, because the own highlighter uses them directly (6.4). Every grammar repo in the lock is MIT. The markdown query files in the tree-sitter-markdown fork say they come from nvim-treesitter (Apache-2.0); ship that notice too.
- Forks used only by GPL crates leave `deny.toml` allow-git: dap-types, lsp-types, trash-rs. The grammar forks stay while the highlighter uses them.
- Registry crates: 754 remain after the switch. Their licenses cannot be listed locally. Proof on CI: remove `GPL-3.0-or-later` from `deny.toml` `[licenses].allow`; `cargo deny` in the `supply-chain` job then fails on any GPL registry crate left.
- `cargo deny` alone may not see the Zed crates:
  - `deny.toml:42-43` sets `[licenses.private] ignore = true`.
  - Every Zed crate is `publish = false` (`zed:Cargo.toml:274-275` `[workspace.package] publish = false`, inherited with `publish.workspace = true`, for example `zed:crates/editor/Cargo.toml:5`; `zed:crates/grammars/Cargo.toml:5` sets it directly). cargo-deny may treat them as private and skip them.
  - So two guards:
    - S11 sets `[licenses.private] ignore = false`. Natamux's own crates and `xtask` inherit `license.workspace` (MIT by then), so they pass.
    - `scripts/zed-lock-check.py` (from S0.2): reads `Cargo.lock` and fails if the set of packages whose `source` starts with `git+https://github.com/lenhatthanh20/zed?` (all 106 today use `?rev=69c8133...`) is not exactly the list in `scripts/zed-lock-crates.txt`. The list starts with today's 106 and shrinks in each removal step (S2.6, S7, S8.4, S9.2, S10) to the 21 of 3.3.
- MPL-2.0 is file-level copyleft and fine for an MIT app. Keep it in the allow list and ship its text.

### 3.5 Assets

| Asset | License | Action |
|---|---|---|
| Fonts IBM Plex Sans (4 TTF) and Lilex (4 TTF), now from Zed's `assets` crate | SIL OFL 1.1 | copy unchanged with `license.txt` and `OFL.txt` (6.9) |
| 73 Zed UI icons the app names, plus the ones Zed's widgets draw (Check, Dash, chevrons, Info, ArrowUpRight) | Zed's own drawings (`zed:crates/icons/README.md`: modified Lucide, some Phosphor, many from scratch); `zed:assets/icons/LICENSES` has only the Lucide text | never copy; replace (D2) |
| 46 Zed file icons and `folder.svg` | Zed's own redraws | never copy; replace (D3) |
| App themes `crates/natamux-app/assets/themes/*.json` | MIT upstream palettes; `natamux.json`, `ayu.json`, `gruvbox.json` derive from Zed's One/Ayu/Gruvbox JSON, which Zed ships with their MIT `LICENSE` files (`zed:assets/themes/{one,ayu,gruvbox}/LICENSE`) | keep (D20) |
| App icons `assets/icons/{lucide,states,motion,agents}` and their license files | ISC, own, MIT | keep |
| Zed default settings JSON and keymaps | GPL data | not used; the app writes its own values (`app/appearance_rules.rs`, `app/shortcuts.rs`) |
| The web client the daemon embeds (`crates/natamux-daemon/build.rs:22-52` embeds `packages/web/dist`): React, xterm.js, monaco-editor, lucide-react, material-icon-theme, react-markdown, remark-gfm, zustand and the Geist, Geist Mono, Inter and JetBrains Mono fonts (`packages/web/package.json:15-35`) | npm licenses, not checked today; the fonts are OFL | S11 item: list them on CI, fail on GPL, ship their notices (section 10 step 6) |

### 3.6 No partial license win

- Removing `assets` only leaves 105 Zed crates.
- Removing `editor` and `terminal_view` leaves 65, and 38 of them are still GPL. They each drag in project, workspace, lsp, client, rpc and more.
- Only removing every GPL direct dependency leaves 21 crates and 0 GPL.

### 3.7 Who can relicense

- `git shortlog`: only the owner (lenhatthanh20 / Le Nhat Thanh, the same email) and dependabot. Dependabot commits only bump versions.
- The `Co-authored-by` trailers (Cursor, Claude, Copilot, dependabot) are tools, not human authors.
- No Rust file says it was copied from Zed.
- The repo was MIT until `aa0723c8` (2026-10-22). The MIT text is in `git show ddfff51e:LICENSE` (21 lines).
- Releases that carried GPL code: `native-beta.1` and `v1.0.0` to the latest `v1.0.x`. They stay GPL for people who got them (D31).

## 4. Decisions taken

| Id | Decision | Reason |
|---|---|---|
| DT1 | All work in PR #542; the license switch is the last step | 3.6: no earlier step removes the GPL exposure |
| DT2 | Order: gate and tests (S0-S1), terminal (S2), text input (S3), code view (S4), Markdown (S5), diff (S6), drop the editor stack (S7), UI kit (S8), theme and settings (S9), assets (S10), switch (S11), final gate (S12) | Terminal first: it is priority 1 and the most work. While Zed's `ui` and `theme` are still linked, the new terminal, input and views use them through `ui::`/`theme::`, so they need no bridge. The theme moves only after the last Zed crate that reads Zed's theme globals (`ui`) is gone, so no two-theme bridge is needed. The icon change is the only planned pixel change, so it comes late and alone |
| DT3 | The pixel gate is an exact compare (every pixel, alpha ignored) of two CI artifacts, run outside CI by `scripts/shots-exact.py`. `REPORT_ONLY` stays `"1"` (`.github/workflows/rust.yml:374`) and no baseline PNGs are committed | The 426 committed baselines are stale (0 of them match today's run). `scripts/compare-shots.py` allows 0.5% of pixels to differ by 16 per channel (`scripts/compare-shots.py:15-17`), which can hide a lost line. 510 PNGs in git cannot be undone under merge commits |
| DT4 | Baseline: the harness commit of S0 is shot twice. Pixels that differ between those two runs form the moving mask | Determinism is not proven yet (traffic lights, caret, timing) |
| DT5 | Behaviour parity: tests written on the old code first, through the public surface (keystrokes, `simulate_input`, `EntityInputHandler` calls, mouse events). Where the data is big, golden JSON is made from the old code on CI and committed (D15) | The same test must pass on the old and the new code |
| DT6 | The two behaviour-heavy swaps (terminal, text input) are built behind cargo features `own-terminal` and `own-input` in `crates/natamux-app/Cargo.toml`. A temporary CI step runs clippy and the tests with the feature on. The feature and the step are removed when the swap is done | Many green commits while the new code grows, and the same suite runs on both. All inside the PR, so nothing GPL ships under MIT |
| DT7 | The terminal is a module `terminals/term/` in natamux-app (no new crate). It keeps the "screen = model + view" shape and the method names `RemoteTerminal` uses today | Only import changes in `RemoteTerminal`, `Parser` and their 14 gpui tests |
| DT8 | Terminal key and mouse encoding are ported from the alacritty app crate (`alacritty/src/input/keyboard.rs`, `alacritty/src/input/mod.rs`; Apache-2.0, notice kept). Zed's code is not used | Zed's `mappings/mouse.rs:3-4` itself says its code comes from Alacritty; the golden table T-K1 decides |
| DT9 | The text model is a plain-Rust module `text/` in natamux-app (no gpui, like a rules file). Storage is `String` plus a line-start index | `ropey` is not in the lock. The texts are small (a 5,000-line file is about 200 KB) |
| DT10 | No new crates.io crate. In the lock already: imara-diff 0.2.0, pulldown-cmark 0.13.5, tree-sitter 0.27.0 and the 17 grammar crates, unicode-segmentation 1.13.3, palette 0.7.6, strum 0.28.0, shlex 2.0.1, regex 1.12.3, smallvec 1.16.1, linkify 0.10.0 | Adding a crate needs a checksum (only cargo gives it), cargo-vet entries and the 7-day cooldown |
| DT11 | Diff hunks: `imara_diff` `Algorithm::Histogram` on lines plus `postprocess_lines`, the same as `zed:crates/buffer_diff/src/buffer_diff.rs:1252-1259` | Same algorithm gives the same hunks |
| DT12 | An own `Appearance` global holds fonts and sizes from S2.2 on. It is set by `app/appearance.rs` from the same inputs as the Zed settings JSON, and checked equal to Zed's values. In S9 it becomes the only source | The terminal and the inputs need fonts without Zed `settings` |
| DT13 | Never copy Zed icons (GPL art). Copy the OFL fonts unchanged | 3.5 |
| DT14 | Keep every quirk of today unless a decision in section 12 changes it | Priority 1 and 2: a "fix" by accident is a new behaviour |
| DT15 | `Cargo.lock` changes are made by a temporary CI workflow (`cargo metadata` without `--locked`), on a snapshot branch only (section 8.4). A hand edit is allowed only to add a dependency edge to a package already in the lock. Never `cargo update` or `generate-lockfile` | No cargo locally; every job uses `--locked` |
| DT16 | The font alias names `.ZedSans`/`.ZedMono` stay | They are strings in gpui (Apache, `zed:crates/gpui/src/text_system.rs:1426-1444`) |
| DT17 | The new terminal and the new text input tell their owners about changes only by events (`cx.emit`/`cx.subscribe`). They never call owner code inside their own update | 6.3.8: a synchronous callback panics in at least 7 places |

Decided on 2026-10-30 (the former open decisions of section 12). Each row: the options in one line, then the choice.

| Id | Decision | Reason |
|---|---|---|
| D1 | Options: two new crates `natamux-ui` and `natamux-theme` (MIT, L4, used through Cargo key renames) / modules in natamux-app with facades. **Chosen: the two crates.** Everything else stays as modules in natamux-app. The owner can still overrule this before S8 | About 13 call sites change instead of 198 `use ui`/`use theme` lines in 236 files (590 `ui::`/`theme::` uses in `crates/natamux-app/src`). S8.1 re-exports Zed with zero render change, so the first shot run proves the switch alone. Path crates need no checksum, vet entry or cooldown (DT10 is about crates.io). `docs/rust-phase3.md:10` asks to ask first; this row is that ask |
| D2 | Options: Lucide drawings at Zed's metrics with one owner-signed re-baseline / owner-drawn icons / another source. **Chosen: Lucide (ISC) at Zed's metrics (6.9)**, plus the key-hint glyphs of D4. One re-baseline in S10, signed by the owner | Zed sources most icons from Lucide and then adjusts them (`zed:crates/icons/README.md`, "Sourcing"), so Lucide is the nearest licensed shape. The 6.9 viewBox gives Zed's 12 px box and 1.2 px stroke (`zed:assets/icons/check.svg`: 16 box, `stroke-width="1.2"`). Phosphor is further from the look; owner-drawn icons cost the owner time and still differ. No option is pixel-identical, because every Zed file is Zed's GPL drawing |
| D3 | Options: Simple Icons (CC0) for logos + Lucide for generic icons / Material Icon Theme (MIT, multi-colour, flattened). **Chosen: Simple Icons + Lucide.** Java (gone from Simple Icons) uses Lucide `coffee`. One `LICENSE-simple-icons` ships | Zed's file icons are single-colour line or solid shapes in a 16 box (`zed:assets/icons/file_icons/rust.svg`, `docker.svg`), drawn as one tinted alpha mask at 12 px (`files/file_icon_view.rs:10-100`). Simple Icons are one path and tint cleanly. Material icons are multi-colour; flattened, their inner parts merge into a blob at 12 px. Logos are used only as file-type markers, as today |
| D4 | Options: own key hint matched to the B1 shots by measuring / reuse the app's `Kbd` chip (`components/chips.rs:141-171`) with a re-baseline. **Chosen: own `KeyBinding`, measured and matched.** Measure from the B1 shots (`add-task-dialog`, `send-back`, `account-menu`) and the S8.1 metrics golden (bounds of each hint, read with `debug_bounds`, never from Zed's source). If the macOS glyphs (⌘ ↵ ...) are icons (Zed ships `assets/icons/{command,return,shift,option,control,escape,backspace}.svg`), draw them through `IconName`: S8.2 then matches to the pixel while Zed's icon files are still served, and they change with the other icons in S10 (add them to the 6.9 Lucide map). Letter keys ("A" in `send-back`) are text. Fall back to `Kbd` only with the owner's sign-off | The hint is in the 8 workflow buttons (`components/button_style.rs:34-52`; callers such as `tasks/add_task_dialog.rs:335,339`, `tasks/edit_form.rs:163,204`). `Kbd` has its own box (18 px high, border, 11 px medium buffer font), so it would change all of them. The two Zed key files stay unread (path rule) |
| D5 | Options: keep (a plain click on an OSC 8 cell opens any scheme) / only http, https, mailto. **Chosen: only http/https/mailto.** The new terminal calls `links::open_link` (`app/links.rs:18-22`); other schemes do nothing, as with Cmd+click. A plain click on a safe OSC 8 link still opens it. T-L1 records the old behaviour on the old code and the new rule under `own-terminal` | The owner made this rule twice already: the fork's Cmd+click check (`zed:crates/terminal_view/src/terminal_view.rs:1332-1337`, "only web and mail links open") and the app's single link gate with the test `only_links_rs_calls_open_url` (`app/links_tests.rs:50-63`), which fails if the new terminal calls `open_url` itself. The gap is Zed code (`zed:crates/terminal/src/terminal.rs:2967-2982`), not a Natamux choice. Program output picks OSC 8 targets, so `file:`, `ssh:` or app schemes could start local programs |
| D6 | Options: keep the 3 dead terminal menu items / drop them. **Chosen: keep them** in the same order and look; a click closes the menu and does nothing. Drop them in a follow-up PR | Identical UI (priority 3) and DT14. The terminal menu is in the S8.3 shot list; dropping 3 rows and a separator changes its height |
| D7 | Options: keep the drift of find highlights on new output / search again on output. **Chosen: keep;** follow-up | Searching again means a regex over up to 100,000 history lines on output that wakes every 4 ms while focused: new load and new timing in the terminal (priority 1). Keeping is parity; `terminal-find` pins it |
| D8 | Options: keep the late query answers after a catch-up / drop answers made while a catch-up is parsed. **Chosen: keep.** Follow-up: match the web client, which never sends them (`docs/e2e.md:106`) | The queue lives in `natamux-client` (`protocol.rs:266-288,360-388`), a frozen crate. Dropping in the app needs new catch-up state in the terminal model (priority 1 risk), and a program still waiting for an answer (an agent's TUI that starts while the app connects) would lose it. Keeping is today's behaviour |
| D9 | Options: keep / fix (SGR 8 drawn visible, SGR 58 ignored, all underline kinds plain, scrollbar shown on every output while history grows). **Chosen: keep all four;** fixes are follow-ups | Each fix changes pixels in `terminal-look` or the existing terminal shots; DT14 |
| D10 | Options: keep whole-line Copy/Cut with no selection (Zed) / do nothing like a web textarea. **Chosen: keep** (6.3.5) | Parity in every workflow box (priority 2); T-CLIP-1 records it |
| D11 | Options: keep multi-cursor (Alt+click, Alt+Shift drag) / drop it. **Chosen: drop.** Alt is ignored on mouse down and drag: Alt+click places the caret like a plain click | No app code or binding uses it and no shot shows it. Multi-cursor puts many selections into every edit, IME and undo path of the one `TextInput` that all workflow boxes share; one selection (6.3.2) keeps that path small (priority 2). No test records Alt+click |
| D12 | Options: keep the other-match highlight, the bracket highlight and the fold markers / drop some. **Chosen: keep both highlights; drop the fold markers and folding.** Keep the gutter width Zed gives with folds on: right padding 4 × `ch` next to line numbers in a single-file editor (`zed:crates/editor/src/editor.rs:12389-12398`); T-LOOK records the px | Both highlights show in shots (the bracket at line 7 of `workspace-changes-diff`). Fold markers show only on hover. Folding needs display rows that differ from buffer rows inside the shared `TextInput`, which touches the caret, selection and IME code of the workflow boxes (priority 2). The kept padding keeps the editor tab pixel-identical |
| D13 | Options: add Mod+C copy of a mouse selection in the rendered Markdown / keep it look-only. **Chosen: keep it look-only;** a follow-up may add Copy | Zed's keymap is never loaded, so no key copies from it today; DT14. A new binding in the Docs context is new behaviour to test |
| D14 | Options: hold the editor caret solid in `--shot` / leave it blinking. **Chosen: hold it solid, only under `--shot`**, in S0.3 before B1, even if C0 shows no moving caret: a shot-only `"cursor_blink": false` next to the scrollbar line (`app/appearance_rules.rs:148-150`) and its test; the own input does the same (6.3.3) | A blinking caret can match in B1 and B1m by chance and then fail a later run. Real use keeps the 500 ms blink |
| D15 | Options: commit the golden JSON made by the GPL build / do not. **Chosen: commit it** under `crates/natamux-app/tests/golden/` | The files hold output facts about Natamux's own fixtures (bytes, numbers, ranges, colour values), not Zed code or text. They are the only way to run one check on the old and the new code after the Zed crates go (DT5) |
| D16 | Options: vendor the tree-sitter-javascript query files with the MIT notice / add the crate. **Chosen: vendor** `highlights.scm`, `highlights-jsx.scm`, `highlights-params.scm` and `injections.scm`, from the tree-sitter-javascript version the tree-sitter-typescript fork (@e2c5359) builds on. Fetch them with read-only `gh api` | DT10: no new crates.io crate (checksum, cargo-vet, 7-day cooldown) just for 4 text constants |
| D17 | Options: keep the possible drift of side-by-side diff with wrap on / fix it. **Chosen: keep;** the new view must draw what B1's `changes-diff-wrapped` shows. Fix later | Parity; the shot pins today's rows |
| D18 | Options: exact syntax colours for all 22 languages via T-HL-1 / owner-signed exceptions for the rare ones. **Chosen: exact for all 22:** T-HL-1 must match 100% and every `code-<lang>` shot exactly. An exception is allowed only where a match would need Zed's own query text; each lists the captures that differ and needs the owner's sign-off | 7.2 gives every language a `code-<lang>` shot, so all 22 are seen. jsdoc and regex show inside common JS/TS files, env in every `.env` file. The own `extra.scm` files come from each grammar's `node-types.json` |
| D19 | Options: the 15 other themes match to the pixel / not. **Chosen: yes.** The theme value test checks every field of all 17 themes in both modes; plus 3 shot states per theme (`tasks-table`, `workspace-changes-diff`, `workspace-terminal-running`), shot at 1000x680 only | Users pick these themes in Settings; a wrong fallback colour changes their whole app. One size keeps the added shot time to about 1-2 min per shard |
| D20 | Options: keep `natamux.json`, `ayu.json`, `gruvbox.json` / redraw them. **Chosen: keep;** `themes/LICENSES` and the notices add that their JSON layout comes from Zed's MIT theme files | Those Zed folders ship the MIT text (`zed:assets/themes/{one,ayu,gruvbox}/LICENSE`); `themes/LICENSES` already credits the palettes. Natamux Light/Dark is the default look, so a redraw would change every shot |
| D21 | Options: own small `build.rs` with the SDK rule / a constant measured from B1. **Chosen: own `build.rs` in `natamux-ui`, written fresh:** on macOS, read the major of `xcrun --sdk macosx --show-sdk-version`; 26 or more sets a cfg that gives 78, else 71 | The SDK differs by runner: app-macos and the shots build on `macos-15` (`.github/workflows/rust.yml:174,368`, so 71); releases build on `macos-26` and `macos-26-intel` (`.github/workflows/release.yml:68-69`, so 78). A constant would be wrong either in the shots or in the releases |
| D22 | Options: keep the `menu` action namespace / rename it. **Chosen: keep** | `components/input_rules.rs:138-153` and `components/menu.rs:107-109` dispatch by name; nothing and no test changes |
| D23 | Options: keep Zed's accessibility roles in the own widgets / drop them. **Chosen: keep** (6.7.1) | No pixel cost and no accessibility loss |
| D24 | Options: extend the colour-literal scan to the new crates with an allow list / leave it on natamux-app only. **Chosen: extend** `xtask colours` to `crates/natamux-ui/src` and `crates/natamux-theme/src`. Allow-list only files that must hold literals: the elevation shadows file in natamux-ui, the theme loader file (it calls `hsla(`), and `terminals/term/colors.rs` and `contrast.rs` in natamux-app (cube and gray math, the black/white fallback) | Keeps the rule that colours come from the theme (`xtask/src/main.rs:155-162`). The scan flags any `hsla(` call, so the new files need explicit entries |
| D25 | Options: let the local Docker format step download the rustfmt component once / no. **Chosen: no;** use the `fmt-patch` artifact (8.2 step 2) | The owner's rule for this work: locally only format code in a container, no package installs and no builds. A rustup component download is an install |
| D26 | Options: agents push `natamux-ci/mit`, `mit-check`, `mit-tools`, `perf-mit-before`, `perf-mit-after` / every loop runs on the PR branch. **Chosen: use the five refs.** The first push still needs the owner's explicit OK in chat (memory: a wip push needs an explicit OK); this row is not that OK | Shots of the exact commit (7.1 merge-ref trap), no cancelled PR runs, and a check-only loop of about 5 min |
| D27 | Options: merge `main` into `feat/mit-license` now (S0.1) / later. **Chosen: now, before the control run:** fetch, then merge `origin/main` with a merge commit. Today the branch lacks 2 commits of `main` (`545937ad` Release v1.0.9, `bb723790`). Later merges only at planned points, each with a new control run | The Settings shots print the version (`app/settings_view_render.rs:65`); C0, B1 and B1m must be built on the same `main` |
| D28 | Options: also run the exact compare inside CI as a temporary `rust.yml` step / only outside CI. **Chosen: only outside CI** | The outside compare is the step gate (7.1). A CI step needs B1 inside CI (artifact download across runs) and more workflow code to remove later |
| D29 | Options: add F1-F4 whole-app flows to `app-macos` / not. **Chosen: add them** (`--smoke --fake-daemon`) | Smoke sends `PlatformInput` through the real macOS window (`app/smoke.rs:47-60`): the only cover of the workflow glue on the real platform (priority 2). It adds seconds to a job that runs anyway |
| D30 | Options: `cargo about` in the tools workflow (installed on the CI runner only) / a `cargo deny list` license table plus one copy of each license text. **Chosen: `cargo about`,** installed in `Tool: notices` only (`cargo install --locked` at a pinned version). Commit `about.toml` and `about.hbs` so the output can be made again; make it in S11 from the final `Cargo.lock` and commit it as `THIRD-PARTY-RUST.md`, a generated file next to the hand-written `THIRD-PARTY-NOTICES.md` (section 10 step 6), so it never overwrites the hand entries | MIT, BSD and Apache-2.0 ask to keep each crate's copyright notice. `cargo deny list` gives only license ids, so the copyright lines would be missing |
| D31 | Options: say nothing on GPL-era releases / state their license. **Chosen: README says** `native-beta.1` and `v1.0.0` to the last `v1.0.x` were released under GPL-3.0-or-later and stay so; MIT from the first release after this PR | A license grant cannot be taken back from people who got those builds (3.7) |
| D32 | Options: licenses in the bundle's Resources only / also on a screen. **Chosen: Resources only,** plus the Windows and Linux packages (section 10 step 7). No screen | No UI change (priority 3). A file shipped with the program meets the notice terms of MIT and Apache-2.0 |
| D33 | Options: the owner confirms each planned removal once per step / per item. **Chosen: once per step, with every item listed.** Before each removal commit, the agent lists every removal of the step (dependencies, files, lines, config entries, the temporary CI step, branches) and waits for the owner's yes. No blanket yes for later steps. Local file removals use `trash`, never `rm` | The owner's rule: confirm before deleting anything, even when asked |
| D34 | Options: record Tab inside the inline Edit form (T19) and keep it / change it. **Chosen: keep what T19 records** | DT14; the Edit form is workflow (priority 2) |
| D35 | Options: the terminal test seam inside natamux-app / a test-only helper in `natamux-client`. **Chosen: inside natamux-app:** one free function `send_input(handle: &TerminalHandle, text: String)` in `terminals/remote_terminal.rs` that holds the `#[cfg(test)]` recorder and then calls `TerminalHandle::input`. All three paths that reach the shell call it: the input task (`terminals/remote_terminal.rs:436-440`, after `from_utf8_lossy`), `type_text` (`:258-263`; `paste_text` and the Files drop go through it) and the pending flush on Open (`terminals/remote_terminal_events.rs:54-56`). No other code calls `TerminalHandle::input` (a grep test checks it) | These are the only points where text leaves the app. A recorder in the input task alone misses `type_text`, the path drop of T-V1 and the pending text. No frozen crate changes. `TerminalHandle::detached` (`crates/natamux-client/src/terminals/session.rs:92-101`) already gives a handle with no socket |
| D36 | Options: keep Zed's drag-move of selected text in every text box (press in a selection, wait 300 ms, drag; Alt copies on macOS) / drop it. **Chosen: keep** (6.3.3); the owner may drop it in a follow-up | DT14 and priority 2: it is today's behaviour in every workflow box. T-MOUSE-1 records it on the old code |

Owner answers of 2026-10-30. The orchestrator listed every removal of this plan and the five CI refs in chat; the owner replied "ok đồng ý hết tất cả" (yes to all):

- D26: yes. Agents may push the five `natamux-ci/*` refs. The perf refs are `natamux-ci/perf-mit-before` and `natamux-ci/perf-mit-after`, because other tasks share `natamux-ci/perf-before`/`perf-after`.
- D33: yes to every removal this plan lists, for content in commits of this PR (Zed dependencies and lock entries, old code paths, the temporary features and CI steps, `deny.toml` and `.cargo/audit.toml` entries, unused `[patch.crates-io]` entries, Zed icons, the GPL license text and GPL mentions). Anything else (branches, worktrees, files outside the repo) still needs the owner's yes. A local file removal uses `trash`, never `rm`.
- D2, D3: the icon change goes ahead with the chosen sources.
- Owner sign-offs and manual gates (S0.4 new states, the IME and perf gates of 7.3 and 7.4, S10 icons, D18 exceptions): agents do not wait. They record each one in the progress file and the PR description; the owner does them on the final artifact, before the merge.
- A test that finds a real bug in today's code: record it, keep today's behaviour (DT14), list it for the owner.

## 5. Target shape

| Today (Zed) | New (Natamux) | Where |
|---|---|---|
| `terminal::Terminal` + `terminal_view::TerminalView` | `TermModel` + `TermView` + `TermElement` | `terminals/term/` |
| `editor::Editor` single line / auto height / multi line | `TextInput` behind the `TextBox` facade | `text/` (model, plain Rust), `text_view/` (gpui) |
| `editor::Editor` + `language::Buffer` for code (file tab, JSON tab, Docs edit and source) | `CodeView` = `TextInput` full mode + gutter + highlight | `text_view/code_view.rs` |
| `grammars` + `language::LanguageRegistry` | own language table and highlighter on tree-sitter | `git/languages.rs` (rewritten), `text_view/highlight.rs`, `assets/queries/<lang>/*.scm` |
| `editor` multibuffer + `buffer_diff` + `text::Point` | own diff model (plain Rust) + diff view on `uniform_list` | `git/diff_model.rs`, `git/diff_view.rs`, `git/diff_editors.rs` |
| `markdown::Markdown` | own Markdown view on pulldown-cmark | `workspaces/docs_markdown.rs` |
| `ui` | `natamux-ui` crate, renamed to the key `ui` in `crates/natamux-app/Cargo.toml` (D1) | `crates/natamux-ui/` |
| `theme`, `theme_settings`, `settings`, `release_channel` | `natamux-theme` crate, renamed to the key `theme` (D1) | `crates/natamux-theme/` |
| `assets` | own asset table with fonts, icons, file icons, licenses | `crates/natamux-ui/assets/` or `crates/natamux-app/assets/` (D1) |

If D1 says no new crates: the kit and theme become modules `components/kit/` and `app/look/`, and every `use ui::...`/`use theme::...` line (about 230 files) changes to `use crate::kit::...` in one mechanical commit before S8.

## 6. Designs

### 6.1 Appearance global (S2.2, completed in S9)

- `Appearance { ui_font: Font, ui_font_size: Pixels, buffer_font: Font, buffer_font_size: Pixels, buffer_line_height: f32, terminal_font: Option<(SharedString, Pixels)>, hide_scrollbars: bool }` as a gpui global, set by `app/appearance.rs` `apply` next to today's `SettingsStore` call (`app/appearance.rs:46-48`).
- Values (must equal Zed's results):
  - `ui_font`: family `.ZedSans`, features `calt=false`, weight 400, normal style, no fallbacks.
  - `buffer_font`: family `.ZedMono`, default features (ligatures on), weight 400.
  - `ui_font_size` = 16 × zoom, `buffer_font_size` = 13 × zoom, each rounded to 0.1 and clamped to 6..100. Zoom is 0.92 / 1.0 / 1.077 (`app/appearance_rules.rs:93-100`), which gives 14.7/16.0/17.2 and 12.0/13.0/14.0.
  - `buffer_line_height` 1.618 ("comfortable").
  - `hide_scrollbars` = `--shot`.
  - Terminal font: the user's family and size (8-32 in the UI, clamp 6-100, no zoom), else the buffer font and size.
- Parity test (gpui, while Zed settings are linked): for the 3 zooms × user font set or not, the own values equal `theme_settings::ThemeSettings` and `TerminalSettings` values.
- Dead code: in S2.2 only the parity test reads the fields, and clippy `-D warnings` would fail with "field is never read". Put `#[cfg_attr(not(test), allow(dead_code))]` on the struct, as `app/appearance_rules.rs:54` does, until S2.4 reads it in the own view. Remove the attribute in that step.
- Redraw on change:
  - The terminal view is drawn `.cached()` (`terminals/terminal_screen.rs:17`). A cached view paints again only when it is notified. Zed's `TerminalView` observes `SettingsStore` and notifies (`zed:crates/terminal_view/src/terminal_view.rs:286`).
  - So `TermView`, `TextInput` and `CodeView` each call `cx.observe_global::<Appearance>` and notify (the terminal also measures the cell again, so a new size reaches the shell).
  - `apply` (`app/appearance.rs`) sets the global with `cx.set_global`, which triggers those observers.
  - Test T-R2 (7.3): change the terminal font through `apply`; the cached view draws again with no new output, and the shell gets the new grid size. Record it on the old code first.

### 6.2 Terminal (priority 1)

#### 6.2.1 What stays

- Everything above Zed's types stays unchanged: panes, cache, size rules, parser lanes, `natamux-client` protocol, daemon, `natamux-screen`. Files: `terminals/terminal_pane.rs`, `terminal_cache.rs`, `terminal_size.rs`, `pane_*.rs`, `find_rules.rs`, `paste_rules.rs`, `terminal_font.rs`, `shell_panes*.rs`.
- Files that change:
  - `terminals/remote_terminal.rs`, `remote_terminal_events.rs`, `terminal_parser.rs`, `terminal_find.rs`, `terminal_screen.rs` (imports);
  - the tests `remote_terminal_tests.rs`, `terminal_screen_tests.rs`, `terminal_find_tests.rs`;
  - `app/shortcuts.rs:241-242`, where `terminal::Copy`/`Paste` become own actions `TerminalCopy`/`TerminalPaste` in context `Terminal`.

#### 6.2.2 Public shape (same contract as today)

| Today | New |
|---|---|
| `TerminalBuilder::new_display_only_with_bounds(cursor_shape, alternate_scroll, history, _, executor, _, bounds).subscribe(cx)` | `TermModel::new(history, bounds, cx)` (cursor Block, alternate scroll On, fixed) |
| `set_display_only_input(sink) -> Result<()>` | same name and semantics |
| `display_only_writer() -> Result<DisplayOnlyWriter>`, `write(&[u8])` | `TermWriter`: lock at most 16 KiB per lock (fair lock), `Processor::advance`, then one `Wakeup` tagged with the screen generation. A writer made before `replace_display_output` writes to the old, unseen screen |
| `set_wakeup_pace(Duration)` | same; default 4 ms |
| `replace_display_output(&[u8], cx) -> Result<()>` | same: a new alacritty `Term` at the same bounds, generation + 1, and selection, matches, hover, scroll accumulator and title reset; emits output-replaced |
| `last_content()`: `display_offset`, `mode`, `columns`, `screen_lines`, `terminal_bounds`, `cells` | same field names |
| `scroll_to_bottom`, `get_content`, `write_output` (tests), `matches`, `find_matches(Search, cx) -> Task<Vec<Range>>`, `activate_match(i)`, `Search::new(&str) -> Option<Search>` | same |
| `TerminalView::new(terminal, WeakEntity::new_invalid(), None, WeakEntity::new_invalid(), window, cx)`, `Focusable`, `Render`, drawn `.cached()` | `TermView::new(model, window, cx)`, same traits |

- Before the swap (S2.1), narrow `TerminalFind` to call `RemoteTerminal` methods (`find`, `activate_match`, `clear_matches`, `match_count`) instead of `terminal.matches` (`terminal_find.rs:111-196`).

#### 6.2.3 Files

- `terminals/term/` holds the files below. The pure ones take plain inputs, so the golden tests need no window.
  - `model.rs`, `writer.rs`, `event_loop.rs`;
  - `input_bytes.rs` (key encoding, alacritty notice; never name it `keys.rs`);
  - `mouse_report.rs` (alacritty notice), `colors.rs`, `contrast.rs`, `links.rs`;
  - `view.rs`, `element.rs`, `grid_layout.rs`, `ime.rs`, `scrollbar.rs`.

#### 6.2.4 Model

- State: `Arc<FairMutex<Term<Listener>>>`.
- Config:
  - `scrolling_history` = `history_lines(cols)` at the first width (`terminals/terminal_size.rs:12`, max 100,000);
  - default cursor Block, not blinking;
  - semantic escape chars = alacritty's `,│`|:"' ()[]{}<>\t` plus `─`;
  - OSC 52 disabled; kitty keyboard off; alternate scroll on.
- Listener: alacritty events go to a channel, tagged with the generation.
- Event loop task (behaviour of `zed:crates/terminal/src/terminal.rs:1460-1530`):
  1. Wait for an event. Handle it at once.
  2. Gather one pace window: until a `timer(pace)` fires, or more than 100 non-wake events wait, or the channel closes. Keep only the newest wake.
  3. If the window got no event at all (no wake and no other event): yield, and go back to step 1.
  4. Else apply the wake first, then the others in arrival order. Yield. Go back to step 2 (a new window with a new timer).
  - So a steady stream keeps the loop in step 2-4, one batch per pace. Only after one full empty window does the next event get handled at once.
  - Drop events whose generation is not current.
  - Lane paces stay in `terminal_parser.rs`: focused 4 ms, shown 16 ms, hidden 250 ms.
  - Tests: Zed's test build uses a random short delay instead of the timer when the pace is the default 4 ms (`terminal.rs:1470-1479`). T-E1 therefore sets a pace other than 4 ms, so both codes use a real timer on the test clock.
- Events:
  - `Wakeup`: emit Wakeup.
  - `PtyWrite`: sink.
  - `ColorRequest(i, fmt)`: sink(fmt(the term colour i, else the theme colour i, premultiplied by alpha to 8 bit)), in event order.
  - `TextAreaSizeRequest`: answer from the current bounds.
  - `CursorBlinkingChange`: emit BlinkChanged.
  - `Bell`: set a flag only (no sound, nothing shown).
  - `Title`/`ResetTitle`: store only.
  - Clipboard events: no-op (OSC 52 is off).
  - Others: nothing.
- `sync()` runs in prepaint, under the lock:
  - It takes the internal queue strictly first in, first out (`zed:crates/terminal/src/terminal.rs:2596-2602`). The kinds are resize, clear, scroll, scroll-to-point, set selection, update selection, copy, find link and open link; their order is the order they were queued, never sorted by kind. Example: scroll, then resize, then select runs in that order (T-E1 case).
  - Then it copies the visible cells into `Content`:
    - cells, mode, total lines, display offset, columns, screen lines;
    - selection range and text, cursor and the char under it, bounds;
    - hovered word, bottom row used, scrolled to top or bottom.
  - If the visible lines changed, it checks the hovered link again; if that queued new events, it emits `Wakeup` (`terminal.rs:2605-2614`).
- `Content.mode` is the mode copied at the last `sync()`, not the live grid mode. Key encoding and the bracketed paste check read it (`zed:terminal.rs:2564`; `terminals/remote_terminal.rs:266-275`).
- `input(bytes)`: queue scroll-to-bottom and clear-selection, then sink.
- These write to the sink directly, with no scroll and no selection clear: query answers, focus reports, mouse reports, wheel reports, alternate scroll.
- `paste(text)`:
  - bracketed mode on: `ESC[200~` + text with every ESC removed + `ESC[201~`;
  - off: `\r\n` and `\n` become `\r` (ESC kept).
- Search: alacritty `RegexIter` over the whole grid, on a background task, under the lock.
- `set_size` queues a resize only when cols, rows, cell width or line height change. A pixel-only change does nothing.

#### 6.2.5 Input encoding

- Keys:
  - Port alacritty's encoder and adapt it to gpui `Keystroke`.
  - Modes in play: APP_CURSOR, APP_KEYPAD, ALT_SCREEN. option_as_meta is false.
  - The encoder takes the mode from `Content.mode` (copied at the last `sync()`), not from the live `Term`. If a program turns APP_CURSOR on and no frame is drawn before the next key, the key still uses the old mode (T-K1 row "mode set, no frame").
  - Fix the port until the golden table T-K1 matches 100%.
  - Known values: Shift+Up = `CSI 1;2A`; AltGr characters are not turned into meta sequences; Ctrl+Q goes to the shell.
- `key_down`:
  - Skip when `prefer_character_input && key_char.is_some()`; the text comes through the IME handler.
  - Stop propagation only when bytes were sent.
  - Clear the bell flag, and hold the cursor solid (blink pause).
- Bytes go out through `String::from_utf8_lossy`. Bytes that are not UTF-8 become U+FFFD (`remote_terminal.rs:439`). This is a quirk; keep it.
- Mouse reports:
  - Mouse mode = mode 1000, 1002 or 1003 on, and no Shift.
  - Buttons: left 0, middle 1, right 2; release 3 (X10/UTF-8) or `m` (SGR). Modifiers: Shift +4, Alt +8, Ctrl +16.
  - SGR: `ESC[<b;col+1;line+1M` or `m`.
  - X10: `ESC[M` + 32+b, 33+col, 33+line, max 223.
  - UTF-8 encoding from 95, max 2015.
  - No report for lines above the screen.
  - Motion: mode 1003 reports every move (button 35 when none is down); mode 1002 reports only drags (32/33/34). Only when the cell or the side changed.
  - A middle or right release is reported only in mouse mode.
  - Linux/FreeBSD: a middle click outside mouse mode pastes the primary selection.

#### 6.2.6 View and element: exact values

- Key context: `Terminal`. Natamux binds only on `Terminal`. Keep the mode words only if a binding needs them.
  - Esc is `NoAction` in `Terminal`, so it reaches the shell.
  - macOS: Cmd chords reach the app.
  - Elsewhere: Ctrl chords without Shift go to the shell; function keys and Ctrl+Shift chords reach the app (`app/shortcuts.rs:215-226`, `app/shortcut_text.rs:84-110`).
  - No vi mode.
- Font:
  - The user's family, else `.ZedMono` (Lilex). Size per 6.1. Weight 400; bold cells weight 700, same colour. `calt` off. No fallbacks.
  - Line height = 1.3 × font px, not rounded (13 px gives 16.9).
  - Cell width = the advance of `m`.
- Layout:
  - The element paints `terminal_background` over all its bounds.
  - It sits in a container with `editor_background` and a stable scrollbar track. The track reserves 14 px (6 + 2 × 4) on the right whenever scrollbars are not hidden, and 0 in `--shot`.
  - Left gutter: one cell; text origin x = bounds.x + cell width. Minimum width: 2 cells.
  - Rows: `lh_dev = max(1, round(lh × scale))`, `avail_dev = floor(height × scale)`, `rows = max(1, avail_dev / lh_dev)`; height = rows × lh_dev / scale.
  - The spare pixels go on top when the alt screen is on, or when scrolled to the bottom and the bottom row is used; else they go at the bottom.
  - Origin x and y are snapped down to device pixels.
  - `num_lines = floor(next_up(height / lh))`, the same for columns. The quirk (very tall panes, about 170 rows at 13 px, can get one extra row) stays.
  - Default bounds before layout: 500×30 px, cell 5×5, which gives 100×6.
- Text runs:
  - Neighbouring cells of one line with the same font, colour, background, underline and strikethrough join one run. They must be adjacent columns.
  - Each run is shaped with `shape_line(text, size, [run], Some(cell_width))`. It is painted at (origin.x + col × cw, origin.y + line × lh), with height lh.
  - Skipped cells: wide-char spacers; a space right after a cell with zero-width extras; blank cells (space, default background, no link, no underline/inverse/strike).
  - Zero-width chars join the text but not the cell count. A wide char ends its run.
- Background rects:
  - Swap fg and bg first for inverse.
  - Cells whose bg is not `Named(Background)` get a rect. Same-colour neighbours merge, then equal spans merge across lines.
  - Each rect: x = floor(origin.x + col × cw), width = ceil(n × cw), height lh.
- Colours:
  - Named 16 → `terminal_ansi_*` / `terminal_ansi_bright_*`.
  - Foreground → `terminal_foreground`. Background → `terminal_ansi_background`. Cursor → `players().local().cursor`.
  - Dim* → `terminal_ansi_dim_*`. BrightForeground → `terminal_bright_foreground`. DimForeground → `terminal_dim_foreground`.
  - Indexed 0-15 like named. 16-231: cube, each `v == 0 ? 0 : v × 40 + 55`. 232-255: gray `(i − 232) × 10 + 8`. 256-268 per alacritty. Else black.
  - Spec RGB exact.
  - Natamux themes have no `terminal.ansi.background`. The fallback value must equal today's (T-C2 and the theme dump catch it).
- Minimum contrast APCA 45 on the fg:
  - Skip it when the fg is app-chosen (`Spec`, or `Indexed(16..=255)`), or the char is decorative: U+2500-257F, 2580-259F, 25A0-25FF, 1FB00-1FB3B, E0B0-E0BF, E0C0-E0CA, E0CC-E0D1, E0D2-E0D7.
  - Constants: exponent 2.4; coefficients 0.2126729/0.7151522/0.0721750; 0.56/0.57/0.62/0.65; black threshold 0.022 and 1.414; scale 1.14; offset 0.027; delta 0.0005; low clip 0.1.
  - Search: if |Lc| ≥ 45, keep the colour. Else binary-search the HSL lightness (20 steps, darker if the bg luminance > 0.5, stop within 1 of the target). Else repeat with saturation × 1.0, 0.8 ... 0.0. Else black or white, whichever has more contrast. Alpha kept. Use f32.
  - It must match T-C1 bit for bit after 8-bit quantization.
- Attributes:
  - Dim: fg alpha × 0.7, after contrast.
  - Italic: `FontStyle::Italic`.
  - Underline (any SGR 4 kind) or a link cell: 1 px in the fg colour; wavy only for undercurl. Strikethrough: 1 px in fg.
  - Hidden (SGR 8) is drawn visible. Underline colour (SGR 58) is ignored. Double, dotted and dashed draw as a plain line (D9).
- Block elements:
  - `▀▁…█▉…▏▐▔▕`, quadrants, sextants U+1FB00-1FB3B, and the shades `░▒▓` (fg at 0.25/0.5/0.75 alpha) draw as rects on an 8×24 subcell grid. They are merged and painted after the text.
  - Box drawing U+2500-257F is font glyphs.
- Cursor:
  - Shape from alacritty (DECSCUSR wins; Hidden when DECTCEM is off).
  - Default Block while focused; on focus out the default becomes Hollow.
  - Unfocused (exact `is_focused`; a focused context menu counts as unfocused): Block, Underline and Bar all draw Hollow.
  - Position (floor(col × cw), floor(line × lh)) after `display_offset`.
  - Width: the cell width for a whitespace char, else max(shaped width, cell width), then ceil.
  - Block: a filled quad in the cursor colour; the char under it is drawn again in `terminal_ansi_background`, normal weight.
  - Hollow: a 1 px outline. Bar: 2 px wide. Underline: 2 px high at the bottom.
  - Paint through `window.pixel_snap_bounds`.
  - Hidden while IME marked text shows.
- Blink:
  - It blinks only when the program asks (DECSCUSR blink style or mode 12).
  - 500 ms phases. A key press shows the cursor and restarts the phases after 500 ms.
  - On on focus in when blink applies; off on focus out.
  - Shown when: unfocused; focused in the alt screen; focused and the program did not ask for blink; else by phase.
- Selection:
  - Click count 1: simple selection; the side is right when past half a cell. 2: semantic (word). 3: lines. 0 or 4+: nothing.
  - Shift+click extends the selection, or anchors a new one. This is also how to select while the program tracks the mouse.
  - Drag starts after more than 2 px. Outside the element, it scrolls by ceil/floor(dist^1.1 / lh) lines, clamped to ±3, per mouse move (no timer), not in the alt screen. Drags run only while the view is focused and no gpui drag is active.
  - Right click (not mouse mode): select the word under the pointer if there is no selection, then open the menu.
  - Painted as paths in `players().local().selection` (same colour focused or not). Corner radius 0.15 × lh; curve width = min(r, span/2); quadratic curves; a line spans start col to end col + 1; clipped to the screen; two disjoint lines paint apart. Check against T-P1 and the shots.
  - Linux/FreeBSD: every change also writes the primary selection.
- Copy: Cmd+C (macOS) / Ctrl+Shift+C (others) in context `Terminal` keeps the selection. No selection: nothing happens and nothing reaches the shell. Typing scrolls to the bottom and clears the selection.
- Paste (Cmd+V / Ctrl+Shift+V):
  - If the first clipboard entry is a non-empty image, send `0x16`.
  - If it holds external file paths, send `" " + shlex-quote(path)` for each, then one trailing space, with the paste rule, and focus the view.
  - Else text with the paste rule.
  - The context menu's "Paste Text" pastes text only.
  - A drop from the Files tab keeps `paste_rules.rs` (ESC removed, no newline change, no scroll).
- Wheel:
  - `Started` resets the accumulator.
  - `Moved` adds `pixel_delta(lh).y × 1.0`; lines = trunc(new/lh) − trunc(old/lh); then accumulator %= the viewport height.
  - `Ended`/`Cancelled`: nothing.
  - Mouse mode and no Shift: wheel reports (64 up / 65 down plus modifiers), |lines| times.
  - Else alt screen + alternate scroll + no Shift: `ESC O A` / `ESC O B` × |lines|.
  - Else scroll the view (positive = into history).
  - The pane stops wheel propagation (`terminal_pane.rs:317-319`).
- Scrollbar (Zed's `ui` scrollbar, style Regular; `zed:crates/ui/src/components/scrollbar.rs`):
  - Thumb from total lines, viewport lines and display offset. Dragging sets the display offset at the next render.
  - Sizes: thumb 6 px wide (`:373-378`), padding 4 px on each side (`:27`), so the track is 14 px; minimum thumb length 25 px (`:877,893`); thumb corners fully round (`:1510`).
  - Thumb colour by state: idle `scrollbar_thumb_background`, hovered `scrollbar_thumb_hover_background`, dragged `scrollbar_thumb_active_background`. The colour is the track background (else `surface_background`) blended with that colour; unless hovered or the track is reserved, the base is capped at α 0.7 (`:1437-1462`). The auto-hide fade applies when not hovered.
  - Track background `editor_background` (the view's track config). If that config has a border, the border is 1 px (`:28`) on the inner side, in `border_variant` at 0.6 (`:1478-1495`); read which one the terminal uses from `zed:crates/terminal_view/src/terminal_view.rs` and confirm it with `terminal-scrollbar`.
  - Auto-hide: shows on scroll or geometry change (so also on output while history grows), hides after 1 s (`:22`) with a 400 ms fade (`:23`), shows in 50 ms (`:24`).
  - No horizontal bar.
  - No keyboard scroll keys (Zed's keymap was never loaded).
  - Every shot hides scrollbars (`app/appearance_rules.rs:148-150`), so the look is checked by the `terminal-scrollbar` shot state (7.2, `show: always`). Hover and drag colours are not in a shot; the own code gets a unit test that the thumb colour per state follows the rule above.
- Links:
  - Hover needs the secondary modifier (Cmd on macOS, Ctrl elsewhere) and is not active while selecting. Throttle: a move over 5 px or 100 ms. Check again on modifier change and after a scroll.
  - Target: the OSC 8 link under the pointer (its whole cell span). Else the URL regex on the logical line: `ipfs: ipns: magnet: mailto: gemini:// gopher:// https:// http:// news: file:// git:// ssh: ftp:// zed://` then non-space chars. Trim the end of `. , : ; (` and of an unbalanced `)`. Plain paths never match. `file://` does nothing.
  - Hovered: hand pointer; the range draws in `link_text_hover` with a 1 px underline of that colour; a tooltip shows the URL.
  - Cmd+press on a link stops selection and mouse reports; a release on the same link opens it if the scheme is http, https or mailto (case-insensitive).
  - OSC 8 cells always draw underlined. A plain click on an OSC 8 cell: D5. Its cell index uses `round`, not floor.
- Focus:
  - A click focuses the view (left; middle and right too in mouse mode).
  - Focus in: claim the shell's size; lane focused; `CSI I` if mode 1004; cursor solid; blink on if it applies; refresh the IME rect.
  - Focus out (also into a child such as the menu): lane shown or hidden; `CSI O` if mode 1004; hollow cursor; blink off.
  - On `Wakeup`: `cx.notify()`. `invalidate_character_coordinates()` only when focused, also on a selection change and on output replaced.
- IME handler (`EntityInputHandler` or an element `InputHandler`):
  - `selected_text_range` → `Some(0..0)`.
  - `marked_text_range` → `0..utf16_len(marked)` or None. `text_for_range` → None.
  - `replace_text_in_range` → clear the marked text, send the text as input, invalidate the coordinates.
  - `replace_and_mark_text_in_range` → set the marked text (empty clears it). `unmark_text` → clear.
  - `bounds_for_range` → the cursor bounds shifted right by `range.start × cw`.
  - `apple_press_and_hold_enabled` → false. `character_index_for_point` → None. Others default.
  - Marked text draws at the cursor: a `terminal_background` quad as wide as the shaped text and lh high, then the text in `terminal_foreground` with a 1 px underline of the same colour.
- Context menu (right click, not mouse mode), at the pointer: New Terminal, New Center Terminal, separator, Copy, Paste, Paste Text, Select All, Clear, separator, Close Terminal Tab (D6). "Clear" clears the local screen and history only. "Select All" selects the whole history and screen.
  - Key hints: Zed builds the menu with `menu.context(self.focus_handle)` (`zed:crates/terminal_view/src/terminal_view.rs:577`), so each row shows the key bound to its action in the terminal's context. Today Copy and Paste show ⌘C / ⌘V on macOS (Ctrl+Shift+C / Ctrl+Shift+V elsewhere), from `app/shortcuts.rs:236-243`. The other rows have no binding and no hint.
  - The own menu does the same: it passes the terminal's focus handle as the menu context, and Copy and Paste dispatch the bound action types, so the hints come from the same bindings.
  - Until S2.5 the bound types are Zed's `terminal::Copy` / `terminal::Paste`. The own `TermView` (behind `own-terminal`) handles those two types and puts them in its menu. S2.5 switches the bindings and the menu to the own `TerminalCopy` / `TerminalPaste` in one commit. The own actions must not use the names `terminal::Copy`/`terminal::Paste` while Zed's terminal crate is linked: gpui panics at start on two actions with one name (`zed:crates/gpui/src/action.rs:53,293-311`).
  - Shot state `terminal-context-menu` (7.2) pins the look.
- Query answers: DA, DSR, cursor position as `PtyWrite`; OSC 4/10/11/12 colour answers in event order; text area size. During a catch-up they queue and go out after it (`protocol.rs:277-280`; D8).
- Paint order:
  1. content mask;
  2. bg fill;
  3. mouse listeners;
  4. cursor style (hand on a Cmd-hovered link, else I-beam);
  5. `handle_input`;
  6. modifiers listener;
  7. bg rects;
  8. highlights (find matches in `search_match_background`, then selection);
  9. text runs;
  10. block rects;
  11. IME marked text;
  12. cursor (if visible and no marked text);
  13. hover tooltip.
  - Lay out only the visible rows (content mask).
- Find bar (Natamux code, `terminal_find.rs`):
  - Box 200×24, `editor_background`; count label XSmall; bar padding 4, gap 4, `elevated_surface_background`, elevation 2.
  - The active match becomes the selection and is scrolled into view. Matches are not re-run on new output (D7).

### 6.3 Text input (workflow surface)

#### 6.3.1 Facade (S3.1, still on Zed)

- `text_view/text_box.rs`: `TextBox` wraps `Entity<Editor>` first; later it wraps the own `TextInput`. All 48 files that touch text inputs move to it, including `components/form.rs` (`text_editor`, `input_box`) and `stages/stage_fields_view.rs` (`LiveField`).
- API:
  - Build: `TextBox::one_line(placeholder)`, `TextBox::area(placeholder, min, max)` (wrap on), `TextBox::plain(read_only)` (full mode, wrap, no gutter).
  - Read: `text() -> String` (with the IME marked text, `\n` endings), `focus_handle()`, `is_focused(window)`.
  - Write: `set_text`, `replace_minimal(text)` (minimal edits; caret, scroll and undo stay), `clear`, `insert_at_caret(text)` (replaces the selection, then focuses), `select_all`, `move_to_end`, `set_placeholder`, `set_masked(bool)`, `set_read_only(bool)`.
  - Events: `TextEvent::Edited` (after each change, as an effect, no debounce), `TextEvent::Blurred`.
  - Key context: `Editor` plus `mode = single_line | auto_height | full`. Rename it later only together with `text_boxes.rs`.
- Key bindings (`app/text_boxes.rs`):
  - Today it binds Zed's typed `editor::actions::*` (`app/text_boxes.rs:6,36-79`) and, by name, `editor::MoveUp`, `editor::MoveDown`, `editor::SelectToBeginningOfLine`, `editor::SelectToEndOfLine` (`:88-131`). `by_name` drops a name it cannot build through `filter_map`, with no error. Its 6 tests (`:175-233`) build `Editor::single_line`.
  - The own input cannot register actions named `editor::*` while Zed's `editor` crate is linked (until S7): gpui panics at start on two actions with one name (`zed:crates/gpui/src/action.rs:53,293-311`).
  - So the own input defines its own typed actions in its own namespace (for example `text_input::MoveUp`). From S3.3 `text_boxes.rs` binds each key twice in the same context: to Zed's action and to the own action. gpui tries every matching binding until one is handled (`zed:crates/gpui/src/window.rs:6062-6075`), so a Zed `Editor` (code and diff views until S6) takes Zed's action and a `TextInput` takes the own one; a key neither handles still propagates.
  - S7 removes the Zed half (owner confirms, D33) and the `by_name` list.
  - New test (S3.2, on the old code): every `by_name` entry builds a binding (the count of built bindings equals the list length), for `mac` true and false.
  - The 6 tests move to `TextBox` in S3.3 and run on both arms.

#### 6.3.2 Model (`text/`, plain Rust)

- A `String` plus line starts. Only `\n` is stored: `set_text` and paste normalise `\r\n` and `\r`.
- Offsets are bytes; there are conversions to UTF-16 (IME) and to graphemes (`unicode-segmentation`). How Backspace and Left act on NFD text follows the T-IME-7 record.
- One selection: `anchor`, `head`, `goal_x`.
- Undo transactions:
  - Typing merges when edits are closer than 300 ms and of the same kind.
  - One IME composition, from the first mark to the commit, is one step.
  - `set_text` and `replace_minimal` are their own undoable steps.
- Word motion: classes are word (alphanumeric or `_`), whitespace and punctuation. The previous word start skips whitespace, then one class run; the next word end is the mirror. Newlines stop it. Match the T-KEY records.
- `replace_minimal`: imara-diff on lines, then on chars inside the changed lines; apply back to front.
- Mask: the display string has one `*` per char.

#### 6.3.3 Element and view (`text_view/`)

- It is built on the gpui input API (`EntityInputHandler`, `ElementInputHandler`, `window.handle_input`). `zed:crates/gpui/examples/input.rs` (Apache-2.0) may be read, or copied with its notice.
- One-line: `shape_line`, with a horizontal scroll that keeps the caret in view.
- Area and full: `shape_text` with a wrap width. Full mode lays out only the visible rows and caches shaped lines per text + width.
- Paint order: background (full only), current line (if on), selection, whitespace dots inside the selection, text, IME underline, caret.
- Mouse (Zed's rules: `zed:crates/editor/src/element/mouse.rs:578-740`):
  - Mouse down does nothing if `window.default_prevented()` (`mouse.rs:583`). This is why a ButtonLike's `prevent_default` (6.7.2) keeps the caret where it is.
  - Down places the caret (Shift extends); drag selects, with autoscroll outside; double click selects a word, triple click a line.
  - A press on the gutter (code view and diff) acts as a triple click: it selects the line (`mouse.rs:609-610`).
  - A press inside the selection (click count 1, no Shift) does not move the caret yet (`mouse.rs:626-641`). Then (`mouse.rs:851-893,1080-1115`):
    - release with no move: the caret goes to the press point;
    - a move before 300 ms (`drag_and_drop_selection`, default `enabled: true, delay: 300`, `zed:assets/settings/default.json:433-438`): a normal drag selection starts at the press point;
    - a move after 300 ms: a drop caret follows the pointer; a release over text outside the selection moves the text there, or copies it with Alt on macOS / Ctrl elsewhere.
  - Kept by D36. Whether a read-only view moves text is recorded by T-MOUSE-1 on the old code, with every case above.
  - A click calls `window.focus(&handle)`. Never keep a private focused flag: `window.focus` calls `Window::refresh`, and that is why the cached `DetailsPanel` sees the new `not_editing` (`zed:crates/gpui/src/window.rs:2394`).
  - Alt+click: D11.
- Caret blink: its own timer task per focused input, and it notifies only this view. In `--shot` the caret stays solid (D14).
- Read-only: no edits, no IME; selection and copy work. The caret shows while focused, solid with no blink (`zed:crates/editor/src/editor.rs:9935-9938`: `read_only || blink visible`, and only when focused). The diff (6.5) and Docs source rely on it. Shot state `changes-diff-focused` pins it.

#### 6.3.4 Exact look values

- One-line and area:
  - UI font family, features and weight; size `rems(0.875)` (14 px at UI 16); colour `editor_foreground`; line height `relative(1.618)`; transparent background.
  - Height: one line, or clamp(lines, min, max) × line height, then it scrolls inside.
- Full:
  - Buffer font with default features (ligatures on: `=>` draws as an arrow in `workspace-editor-source`); size 12/13/14 by zoom; background `editor_background`.
  - The current line background `editor_active_line_background` is on in Docs edit, Docs source and the diff, and off in the file editor and the JSON tab (`files/file_editor_body.rs:38`, `projects/json_tab.rs:45`).
- Placeholder: `text_placeholder`, shown only while empty.
- Caret: a bar 2 px wide × line height in `players().local().cursor`. It blinks every 500 ms, stays solid for 500 ms after each input, and is hidden when unfocused. Read-only and focused: solid, no blink (6.3.3).
- Selection: `players().local().selection`, corner radius 0.15 × line height. The end of a selected line overshoots by 0.15 × line height. Whitespace inside a selection draws as dots in `editor_invisible`.
  - When the input is not focused, the selection colour is drawn at 0.5 opacity (`zed:crates/editor/src/element.rs:955-959`). Shot state `text-box-selection-unfocused` pins it.
- Masked: `*` per char. IME marked text: a 1 px underline in the text colour.
- `input_box` (`components/form.rs:24-35`): `px_2p5 py_1p5 rounded_md border_1`, border `border`, bg `editor_background`. It pushes the focus handle that really holds focus into `tab_order`.
- Dialog widths: 400/520/640, rem-scaled. JSON tab: fixed height 22 × 20 + 16 px (`projects/json_tab.rs:21`).
- T-LOOK records the exact line box heights, caret x and selection rects from Zed. The new input must hit them.

#### 6.3.5 Behaviour to keep

- Undo grouping is 300 ms.
- Copy or Cut with no selection takes the whole line plus `\n` (D10).
- What a one-line box does with pasted newlines follows the T-CLIP-1 record.
- No right-click menu.
- Tab inserts only in full mode. In one-line and area boxes, Tab and Shift+Tab propagate.
- Auto-close and auto-indent only in the code view of a language (6.4). Plain boxes have none.
- `accepts_text_input` is true unless read-only. While composing, every key goes to the IME first, Enter too.
- Keys (all from `app/text_boxes.rs`):
  - Any box: Backspace, Delete, Left, Right, Shift+Left/Right, Home/End (they stop at a soft wrap), word Left/Right (Alt on macOS, Ctrl elsewhere), word Shift+Left/Right, word Backspace/Delete, Mod+A/C/X/V/Z, Mod+Shift+Z, Shift+Home/End.
  - macOS also: Cmd+Left/Right, Cmd+Backspace, Cmd+Shift+Left/Right.
  - Many-line boxes: Enter (newline), Up/Down, Shift+Up/Down, PageUp/PageDown, to start/end (Cmd+Up/Down on macOS, Ctrl+Home/End elsewhere) and their Shift forms.
  - Full only: Tab, Shift+Tab.
  - Must propagate: Enter, Up and Down in one-line boxes; Esc; Mod+Enter; Tab outside full mode.

#### 6.3.6 IME contract

- `text_for_range(utf16)`: clip to char boundaries and report the adjusted range.
- `selected_text_range`: the selection in UTF-16, with `reversed`.
- `marked_text_range`: the marked range in UTF-16, or None.
- `replace_text_in_range(range, text)`:
  - With marked text: replace the marked range (ignore the given range).
  - Else, an empty or no range inserts at the caret.
  - Else replace that absolute range (EVKey, OpenKey, the Accessibility Keyboard).
  - Then clear the mark and end the IME transaction.
- `replace_and_mark_text_in_range(range, text, new_sel)`: the range is relative to the marked text when there is one. Mark the inserted text and set the selection at `new_sel`, relative to the insert start. Empty text unmarks. No auto-close while composing.
- `unmark_text`: keep the text and drop the mark. Blur with marked text: keep the text and unmark.
- `bounds_for_range`: the rect of the char at the range start, in window coordinates (the candidate window sits at the caret).
- `character_index_for_point`: implement it.

#### 6.3.7 Workflow contract

- Events reach owners only by `cx.emit` + `cx.subscribe`.
- "Edited" fires once per user edit, after the change, as an effect, with no debounce. Save (Mod+Enter) reads the draft, not the input (`projects/settings_writes.rs:25`), so a debounce would lose the last keys.
- `set_text` behaves the same every time (whether it emits or not). A same-text `set_text` breaks nothing.
- `LiveField::sync` (`stages/stage_fields_view.rs:74-83`) sets the text only when the box is not focused and the text differs. So the input needs a real `FocusHandle`.
- `StagePicker::name_edited` / `id_edited` (`stages/stage_picker.rs:112-123`): a programmatic `set_text` must not make `own_id` become `Some`.
- `Blurred`: the sidebar rename saves on it (`sidebar/tree_rename.rs:39`) and the font field applies on it (`app/settings_font.rs:52`).
- Single keys: `a` (approve), `r` (request changes), `e i c n j k / ?` must type into a focused box under `NatamuxList`/`NatamuxDetails`.
  - The only guard is the `not_editing` context, set from `focus_handle.is_focused(window)` (`tasks/details_panel.rs:338,367`; `sidebar/workspace_tree.rs:465`; `workspaces/docs_tab.rs:321`; `git/changes_view.rs:388`; `git/changes_rows.rs:36`).
  - If this breaks, typing "a" approves a review.
- Enter in a one-line box bubbles to the parent's `on_key_down` (`tasks/add_task_dialog.rs:319`, `tasks/add_subtask_dialog.rs:166`).
- Enter while composing commits the word and does not submit.
- Mod+Enter submits with the committed text. SendBack reads `text()` with the marked text included.

#### 6.3.8 Re-entrancy rules (double lease)

- The input never calls owner code synchronously; it only emits.
- Today these spots are safe only because events are effects:
  - `LiveField::new/sync` (`stages/stage_fields_view.rs:61,74`);
  - `SendBacksField` (`projects/send_backs_field.rs:56,71`) and `IdlePauseField` (`projects/idle_pause_field.rs:60,76`);
  - `StagePicker` (`stages/stage_picker.rs:112,123`);
  - `JsonTab` `follow_draft` (`projects/json_tab.rs:73`);
  - `DetailsPane::refresh` → `sync_stage`;
  - `DialogLayer` `tab_ring` / focus reads (`components/dialog.rs:195-235`).
- Owner closures given to widgets (`on_click`, menu handlers) update other entities only, or use `defer`.
- Keep the existing guards:
  - the Esc defer (`components/dialog.rs:209`); the Settings key defer (`app/settings_view.rs`);
  - the confirm defers (`projects/project_settings_page.rs:253`, `settings_writes.rs`);
  - `take_pending` (`tasks/details_panel.rs:285`); AddTask `start` (`tasks/add_task_dialog.rs:155`);
  - emit-then-run in `components/confirm.rs:99` and `projects/repo_trust_dialog.rs:151`.
- Every flow test runs inside `TestAppContext`; a double lease panics there and fails the test.

#### 6.3.9 Frozen files

- Do not change these workflow rules files:
  - `stages/`: `card_rules.rs`, `send_back_rules.rs`, `stage_view_rules.rs`, `overview_rules.rs`, `preview_rules.rs`, `progress_board_rules.rs`, `skill_rules.rs`, `editor_*.rs`;
  - `tasks/`: `task_rules.rs`, `details_rules.rs`, `edit_rules.rs`, `tries_rules.rs`, `task_attention.rs`, `usage_rules.rs`, `task_events_rules.rs`;
  - `projects/`: `repo_trust_rules.rs`, `settings_tab_rules.rs`, `settings_text.rs`;
  - `workspaces/new_workspace_rules.rs`;
  - and their `*_tests.rs`.
- Exceptions:
  - `app/appearance_rules.rs` and its tests in S9: the output type changes, the numbers in the tests stay.
  - A test-only seam in `natamux-client`, only if D35 allows it.

### 6.4 Highlighting and the code view (S4)

- Engine: `tree-sitter` directly, with no `tree-sitter-highlight`.
  - Parse on load. On each edit, `Tree::edit` and reparse with the old tree.
  - Budget on the main thread: 1 ms, the same budget Zed uses (`zed:crates/language/src/buffer.rs:1177-1180`). Mechanism: `Parser::parse_with_options(callback, Some(&old_tree), Some(ParseOptions { progress_callback }))`, where the progress callback returns `ControlFlow::Break` once 1 ms has passed (as `zed:crates/language/src/syntax_map.rs:1615-1660`). Do not use `set_timeout_micros` (deprecated; Zed at this rev also uses only `parse_with_options`).
  - A cancelled parse leaves state in the parser; call `parser.reset()` before the next parse (`zed:crates/language/src/language.rs:146-151`).
  - On a timeout: take a snapshot (`Arc<str>` of the text, the edited old tree, the language and an edit generation) and finish the parse on `cx.background_spawn` with its own parser. Meanwhile show the old colours shifted by the edit (no flash of uncoloured text). When it ends, the result is applied on the main thread only if the generation is still current; else it is dropped and a newer parse runs.
  - Query only the visible rows plus a margin, and cache by row.
- Language table (own Rust, facts only), the same 22 languages as `git/languages.rs:14-37`: bash c cpp css diff env gitcommit go gomod gowork javascript jsdoc json jsonc markdown markdown-inline python regex rust tsx typescript yaml.
  - The editor tab and the JSON tab pick the language by file extension today (`files/file_editor_body.rs:31,77-86`, `projects/json_tab.rs:102-108`), so each language's typing rules are live today and must stay.
  - Per language, the facts that drive typing (read from `zed:crates/grammars/src/<lang>/config.toml`, written as own Rust data):
    - suffixes and names, grammar, queries, tab size (4; JSON/JS/TS 2; Go hard tabs), line comment, block comment;
    - bracket pairs with their flags `close`, `newline`, `surround` and `not_in` scopes, including quote pairs (JSON closes `"` except inside a string, `json/config.toml:6-10`);
    - `autoclose_before` (the chars after the caret that still allow auto-close; JSON `,]}`);
    - newline expansion: Enter between a pair with `newline = true` puts the caret on an indented middle line and the close on the next line;
    - block comment continuation: `/*` closes with ` */` where the language has it;
    - Markdown lists: `unordered_list` (`- `, `* `, `+ `), `ordered_list` (`N. ` counts up), `task_list` (continues with `- [ ] `) (`markdown/config.toml`);
    - `word_characters` (`#` in JSON and CSS; `$` and `#` in JS/TS) and, inside overrides, `completion_query_characters`.
  - Indent: the rules the T-CODE-1 record shows (Zed uses `indents.scm` queries and `decrease_indent_patterns`; the own table reproduces the recorded results, never Zed's query text).
  - T-CODE-1 (S4.1, on the old code; gpui, through the editor tab and the JSON tab): per language, typing each opening bracket and quote (auto-close, and not inside a `not_in` scope), typing the closing char over an auto-closed one (skip-over), Backspace between an empty pair (pair delete), Enter between brackets, `/*` + Enter, Markdown list and task-list continuation and Enter on an empty item, and word motion over `a#b $c d-e.f:g`. The own code view must give the same text, caret and selection.
- Queries: upstream MIT `highlights.scm`/`injections.scm` from each grammar crate, vendored in `crates/natamux-app/assets/queries/<lang>/` with their notices.
  - The TypeScript query (35 lines) layers on the JavaScript one. Vendor tree-sitter-javascript's query files (MIT; D16).
  - The markdown queries also need the nvim-treesitter notice (Apache-2.0).
  - Write an own small query for env.
  - Own `extra.scm` per language for captures the shots need. Write it from the grammar's `node-types.json`, never from Zed's `.scm`. Never use Helix queries (MPL-2.0).
- Predicates (risk to check in S4.2: the markdown fork's query files are not local, so this is not confirmed yet):
  - The Rust `tree-sitter` crate evaluates only `#eq?`, `#match?`, `#any-of?` and their `#not-` forms. `#set!` and `#is?` come back as properties and are not evaluated.
  - nvim-treesitter files may hold nvim-only parts: `#lua-match?` (Lua patterns), `#offset!`, `#set! conceal`, `#set! priority`, and captures such as `@nospell` and `@conceal`.
  - For each vendored file: list what it uses. Turn `#lua-match?` into an equal `#match?` regex. Drop `#offset!`, conceal and `@nospell`/`@conceal` (they do not colour). Turn `priority` into the overlap rule below. Note each change in the file's header comment, with the notice kept.
- Capture → style:
  - A per-language rename table, upstream name → Natamux name (for example JSON `@string.special.key` → `property.json_key`, markdown `@text.title` → `title.markup`).
  - Then the longest theme syntax key that is a dot-prefix of the name.
  - The run gets the whole theme entry, not only the colour: colour, `font_weight` and `font_style` (`natamux.json`: `emphasis.strong` 700, `title` 400, `link_text` and `predictive` italic; 34 italic and 34 weight entries over all theme files).
  - Overlap rule per vendored file, not one rule for all: upstream grammar queries follow the tree-sitter CLI rule (the earlier pattern wins); nvim-derived queries follow the nvim rule (the later one wins, with `priority`). The table records the rule per file. T-HL-1 proves each file.
- Injections: markdown block → inline; fenced code → its language; JS/TS comments → jsdoc; regex literals → regex.
- `CodeView` = `TextInput` full mode plus:
  - a gutter: line numbers right-aligned in `editor_line_number`, the caret row in `editor_active_line_number`; width = max(widest number, 4 digits) + paddings in `ch` units (T-LOOK records the px);
  - highlight runs;
  - matching bracket background (innermost pair around an empty caret);
  - other-occurrence background `editor_document_highlight_bracket_background`, 100 ms debounce (D12);
  - a current-line option;
  - horizontal scroll when wrap is off;
  - auto-close, skip-over, pair delete, newline expansion, comment and list continuation, word chars and indent from the table, matching T-CODE-1;
  - scroll one page past the end; a vertical scroll margin of 3 lines; scroll sensitivity 1.0, fast 4.0 with Alt;
  - a scrollbar, Zed editor style (`zed:crates/editor/src/element.rs`):
    - width 15 px (`EDITOR_SCROLLBAR_WIDTH`, `zed:crates/ui/src/components/scrollbar.rs:26,373-378`), no padding, full track length; minimum thumb length 25 px (`element.rs:10395,10483`); square corners;
    - track `scrollbar_track_background`; the vertical track has a 1 px left border in `scrollbar_track_border`; the thumb is `scrollbar_thumb_background` / `scrollbar_thumb_hover_background` (hovered) / `scrollbar_thumb_active_background` (dragged), with the same left border in `scrollbar_thumb_border` (`element.rs:6090-6155`);
    - markers painted on the vertical track before the thumb: cursors, selected-text occurrences, symbol (document) highlights and git diff hunks (`default.json` `scrollbar`: `cursors`, `selected_text`, `selected_symbol`, `git_diff` all true; search results and diagnostics never occur in the app);
    - a horizontal bar when wrap is off and a line is wider than the view (both `axes` true);
    - visibility with `show: auto` (`element.rs:1511-1540`): shown while any of these holds, in a single-file buffer: diff hunks exist; selected-text highlights exist; symbol highlights exist; plus, in any buffer: a cursor is out of view, or less than 1 s since the last scroll (`zed:crates/editor/src/scroll.rs:28`). The file editor, JSON tab and Docs use a singleton buffer (`files/file_editor_body.rs:32`, `projects/json_tab.rs:40`, `Editor::multi_line` in `workspaces/docs_tab.rs:271` and `docs_view.rs:29`).
    - T-SB-1 (gpui, own code): a table test of the visibility rule with each case above. Shot states with `show: always` (7.2) pin the look.
- Users: `files/file_editor_body.rs` (editor tab, `set_disk_text` via `replace_minimal`), `projects/json_tab.rs` (JSON, no gutter, 2-way minimal sync, error border while it cannot parse), `workspaces/docs_tab.rs` (Docs edit, plain, Tab inserts), `workspaces/docs_view.rs` (Docs source, read-only).
- JSON tab: typed characters must survive an outside change (Reset, Import, Reload). Apply minimal edits against the current text, never "set the whole text".

### 6.5 Diff view (S6)

- Model (plain Rust, `git/diff_model.rs`):
  - Hunks from imara-diff Histogram + `postprocess_lines`. An empty buffer `"\n"` against a non-empty base is one hunk.
  - Word diff for hunks of at most 5 lines (`zed:crates/language/src/text_diff.rs:340-410`):
    - imara-diff `Algorithm::Histogram` over tokens;
    - a token is a run of chars of one kind (word, whitespace, punctuation); a punctuation run holds only one repeated char, so `==` and `::` are one token but `=>` is two;
    - the kind comes from the language's char classifier in the completion context: `word_characters` and `completion_query_characters` of that language count as word chars (6.4 table);
    - touching ranges merge.
  - Rows per side: `Line{number, text}`, `Deleted{number, text}` (inline only), `Spacer{rows}`, `Comment{id, rows}`, `Gap{hidden}`.
  - Natamux's `diff_rules.rs` (`spacers`, `end_spacer`, `HunkRows`, `line_change`) and `review_rules.rs` (`placement`, `block_rows`, `body_lines`) stay as they are.
- View:
  - Wrap off: `uniform_list` per side with one shared scroll offset. Wrap on: `list` with measured rows. Spacers count buffer rows as today (D17).
  - Read-only `TextInput` behaviour for the caret (solid while focused, 6.3.3), selection and copy. `diff_cursor.rs` keeps the signatures of `cursor_line`, `step_change` (Alt+F5, target at the top), `change_index`, `selection` (whole lines → `Quote`), `select_lines` and `go_to_line` (centre).
  - Mouse, besides 6.3.3: today each side is a non-singleton multibuffer (`MultiBuffer::new`, `git/diff_view.rs:286,300`). In a multibuffer:
    - a press on a line number stops propagation and does not move the caret (it asks to open the excerpt, which does nothing with no workspace; `zed:crates/editor/src/element/mouse.rs:689-726`);
    - a press elsewhere in the gutter selects the line (`mouse.rs:609-610`);
    - a double click selects a word (`double_click_in_multibuffer: "select"`, `zed:assets/settings/default.json:747`).
    - T-MOUSE-1 records these on the old diff view.
  - Unfocused selection at 0.5 opacity (6.3.4).
  - Scrollbar: the editor style of 6.4. Visibility differs: the sides are not singletons and do not allow diff markers, so hunks and selected text do not keep the bar shown (`zed:crates/editor/src/element.rs:1513-1530`); only a cursor out of view or a scroll in the last 1 s does. T-SB-1 has this case. Shot state `changes-diff-scrollbar` (`show: always`) pins the look, markers included.
  - Review comment blocks have a fixed height per body line, with a matching spacer on the other side.
  - `empty_rows` draws at full height (T-0230).
- Look:
  - Added rows: `editor_diff_hunk_added_hollow_background` + 1 px `editor_diff_hunk_added_hollow_border`. Deleted rows use the `_deleted_` versions and have no line number.
  - Word diff: `version_control_word_added/deleted`.
  - Gutter strip: width floor(0.275 × lh), in `version_control_added/modified/deleted`. A deleted-only hunk is a marker floor(0.35 × lh) wide, centred on the row boundary.
  - Context: 3 lines (`git/diff_editors.rs:21`). Side by side from 720 px (`git/diff_rules.rs:13`).
  - The look of gaps between collapsed parts comes from the new shot state of S0.4.

### 6.6 Markdown view (S5)

- pulldown-cmark 0.13 with Zed's option set (`zed:crates/markdown/src/parser.rs:13-23`): tables, footnotes, old footnotes, strikethrough, task lists, smart punctuation (curly quotes, en and em dashes), heading attributes, `+++` metadata blocks, GFM (blockquote alerts), superscript, subscript.
- Bare URLs outside links and code blocks become links, found with `linkify` `LinkFinder` set to `LinkKind::Url` (`parser.rs:612-620`). `linkify` 0.10.0 is already in the lock (`Cargo.lock:4877-4879`); add the edge (DT10).
- No HTML, no Mermaid, no images. Links open only http/https/mailto (`open_link`, `app/links.rs`).
- Values (the Preview style plus the app's sizes, `workspaces/docs_markdown.rs:62-86`):
  - Font sizes are rems (`rems_from_px`), so they scale with the UI zoom: body 13/16 = 0.8125 rem; code block 0.8125 rem; inline code 0.75 rem; h1-h6 = 1.25, 1.0, 0.9375, 0.8125, 0.8125, 0.75 rem (20, 16, 15, 13, 13, 12 px at UI size 16).
  - Body: UI font, line height 1.5, colour `text`. Paragraph spacing 16 px; list spacing 12 px (whether these are px or rem in the Preview style: T-MD-1 records them at two zooms).
  - Headings:
    - margin top 24, bottom 12; line height 1.25;
    - h1 gets `pb_2` and a bottom border, h2 `pb_1` and a bottom border, in `border_variant`;
    - h6 is `text_muted`;
    - the weight is Zed Preview's heading weight (read the value in `zed:crates/markdown/src/markdown.rs:186-410`).
  - Code block: buffer font 0.8125 rem; line height 1.618; padding 12; margin 16; radius 6; 1 px `border_variant` border; bg `editor_background`; scroll x; no syntax colour; no copy or wrap button.
  - Inline code: buffer font 0.75 rem, bg `editor_foreground` at 8%, colour `text`.
  - Link: `text_accent`, with a 1 px underline in `text_accent` at 50%.
  - Quote: `text_muted`, border `border`. Rule: a bottom border `border`, margin = paragraph spacing.
  - Table cell padding 10 × 4. Bullets `•`; ordered `N.`; task lists draw a checkbox.
  - Mouse selection in `element_selection_background`, look only (D13). The same text twice does not reparse.
- The 2 tests in `workspaces/docs_markdown.rs:111-147` stay, as value checks.

### 6.7 UI kit (`natamux-ui`, S8)

#### 6.7.1 Surface

- Build only what the app uses:
  - `prelude`: the gpui names, `h_flex`, `v_flex`, `rems_from_px` (= px/16), `Color`, `Label`, `LabelCommon`, `LabelSize`, `LineHeightStyle`, `Icon`, `IconName`, `IconSize`, `IconPosition`, `Button`, `ButtonCommon`, `ButtonSize`, `ButtonStyle`, `IconButton`, `SelectableButton`, `Clickable`, `Disableable`, `Toggleable`, `FixedWidth`, `VisibleOnHover`, `StyledExt`, `Headline`, `HeadlineSize`, `DynamicSpacing`, `ActiveTheme`.
  - Top level: `ButtonLike`, `IconButtonShape`, `KeyBinding` + `KeybindingPosition` (D4), `ToggleButtonGroup`(+`Style`), `ToggleButtonSimple`, `ToggleButtonWithIcon`, `Checkbox`, `Switch`, `ToggleState`, `Tab`, `TabPosition`, `Disclosure`, `Chip`, `Indicator`, `ListItem`, `ListItemSpacing`, `ContextMenu`, `ContextMenuEntry`, `Tooltip`, `tooltip_container`, `TextSize`, `utils::TRAFFIC_LIGHT_PADDING`.
  - Internal: `ListSubHeader`, `ListSeparator`, `List`, a rem-size override for menus drawn deferred, control-character replacement.
- Menu actions: own `actions!` with the namespace string `menu` (D22). The app dispatches `menu::SelectNext` etc. by name (`components/input_rules.rs:138-153`, `components/menu.rs:107-109`). The key context stays `"menu"`.
  - Timing: Zed's `menu` crate stays linked while Zed's `ui` is (`zed:crates/ui/Cargo.toml:24`), and two actions with one name panic at start (`zed:crates/gpui/src/action.rs:53,293-311`). So in S8.3 the own `ContextMenu` handles Zed's `menu::*` types, re-exported by `natamux-ui` from Zed's `menu` crate. S8.4, which drops Zed `ui` and `menu`, defines the own `actions!(menu, ...)` with the same names in the same commit.
- Traits while Zed widgets remain (S8.2-S8.3):
  - Zed widgets that stay until S8.3/S8.4 (`Checkbox`, `Switch`, `Tab`, `ListItem`, `Disclosure`, `ToggleButtonGroup`, `ContextMenu`) implement Zed's `Clickable`, `Disableable`, `Toggleable`, `FixedWidth` and use Zed's `StyledExt`. An own trait of the same name in the prelude would leave `.disabled()` or `.toggle_state()` on them unresolved, and two `StyledExt` traits make calls on `Div` ambiguous.
  - So until S8.4 the prelude keeps exporting Zed's traits, and the own widgets implement Zed's traits (allowed: the types are local). Own `elevation_2`/`elevation_3` come from Zed's `StyledExt` until then (same values).
  - S8.4 switches the prelude to own traits and the own `StyledExt` in one commit, when no Zed widget is left.
- Accessibility roles as today (D23): `Role::Button`, `aria_label`, `aria_toggled`, `Role::Switch`, `Role::Menu`/`MenuItem`/`MenuItemCheckBox`, `aria_active_descendant`, `aria_expanded`.
- Not built (unused): Divider, Avatar, Banner, Callout, Modal, Popover, PopoverMenu, RightClickMenu, TabBar, DataTable, ProgressBar, DropdownMenu, SplitButton, the component preview system, `ElevationIndex` as public API, `StyledTypography` helpers, animations.

#### 6.7.2 Exact values (px at UI size 16; rem items scale with zoom)

- Text: Large 16 (1.0 rem), Default 14 (0.875), Small 12 (0.75), XSmall 10 (0.625). `TextSize::*.pixels(cx)` returns fixed px 16/14/12/10 and does not scale (`tasks/state_filter.rs:72` relies on it).
- Label line height: gpui's default 1.618 × size; `LineHeightStyle::UiLabel` = `relative(1.)`.
- Headline (rem): XSmall 0.88, Small 1.0, Medium 1.125, Large 1.27, XLarge 1.43. Line height 1.6 rem. It ignores `.color()` (always `text`) and uses the UI font.
- Icon sizes: Indicator 10, XSmall 12, Small 14, Medium 16 (default), XLarge 48.
- `DynamicSpacing::BaseNN` = NN px; `.rems` = NN/16 rem; `.px` = ui_font_size × NN/16.
- Radii (gpui): xs 2, sm 4, md 6, lg 8, xl 12.
- `elevation_2`/`elevation_3`: `bg(elevated_surface_background)`, `rounded_lg`, `border_1`, `border_variant`, plus black shadows:
  - elevation 2: (0,2) blur 3 α 0.12; (0,1) blur 0 α 0.03 light / 0.06 dark.
  - elevation 3: (0,2) blur 3 α 0.06/0.12; (0,3) blur 6 α 0.06/0.08; (0,6) blur 12 α 0.04; (0,1) blur 0 α 0.04/0.12 (light/dark).
- ButtonLike:
  - Heights: Large 32, Medium 28, Default 22, Compact 18, None 16.
  - Padding x: Large/Medium 8, Default/Compact 4, None 1 px. Gap 4. Corners 4.
  - `border_1` only for Outlined. `w(width) + justify_center` when a width is set.
  - Cursor: hand; not-allowed when disabled.
  - On hover the bg changes. Outlined focus-visible sets `border_focused`; the other styles use the hover bg. Active sets the active bg.
  - Disabled: no hover, no active, no click, and the enabled bg and border stay.
- ButtonStyle colours:

| State | Outlined bg / border | Subtle bg / border | Tinted(Accent), only in ToggleButtonGroup |
|---|---|---|---|
| enabled | element_background / border_variant | ghost_element_background / transparent | status.info_background / status.info_border |
| hover | ghost_element_hover / border | ghost_element_hover / transparent | info_background darkened: lightness − 0.05 light, − 0.2 dark, clamp 0 |
| active | element_active / border_variant | ghost_element_active / transparent | as enabled |

- Button:
  - Label colour: `Disabled` when disabled; when toggled, `selected_label_color` or `Color::Selected` (= `text_accent`); else the set colour or `Default`.
  - Layout: h_flex gap 4 of [start icon][h_flex gap 6, `justify_between`, `flex_row_reverse` with `KeybindingPosition::Start`, of the label and the key hint][end icon].
  - The default label size is 14. `truncate` adds `min_w_0 + overflow_hidden` on both rows.
- IconButton:
  - The default shape is Wide (width = icon + 2 × padding).
  - Square: padding 0 for Indicator, 2 for XSmall/Small/Medium/XLarge; for `Custom(r)` the padding equals the size.
  - Icon colour: disabled → Disabled; selected → `selected_icon_color` or `Selected`; else the icon colour.
- ToggleButtonGroup:
  - `rounded_md`, `overflow_hidden`. Outlined: `border_1`, `border` at opacity 0.6, cells split by `border_r_1` in the same colour.
  - A selected cell is `Tinted(Accent)`.
  - Cell content: `px_2 gap_1p5 justify_center`, plus `py_2` with an icon. The icon is XSmall, Accent when selected, else Muted. The label is Small, Accent when selected.
  - Width is `relative(1/COLS)` unless `auto_width`.
- Checkbox:
  - Container 20×20; box 16 with `m_1`, `rounded_xs`, `border_1`; bg `ghost_element_background`; border `border`.
  - Disabled: border `border_variant`, bg `element_disabled` at 0.6, cursor not-allowed.
  - Hover: the border at α 0.7.
  - Selected: Check Small in `Selected`. Indeterminate: Dash.
  - Row gap 6. The label is 14 Muted.
  - `on_click` passes the inverse state. `visualization_only` turns off the hover border and uses the default cursor.
- Switch:
  - Wrapper `p(1px) border_2`, transparent; `border_focused` on focus-visible; `rounded_full`.
  - Track 32×20, `px 2`, `border_1`. Thumb 12, `rounded_full`, colour `text`, opacity 0.2 disabled / 1.0 on / 0.5 off.
  - Off: bg `element_disabled`, border `border`. On: bg `status.info` at 0.4, border `text_accent` at 0.2.
  - Hover: the bg blended with `text` at 0.16 × r (on) or 0.05 × r (off), where r = 1.5 light, 1.0 dark.
  - Row gap 6; the label is Small Default; the whole row clicks.
- Tab:
  - Height 32 (content 31). bg `tab_active_background` / `tab_inactive_background`; text `text` / `text_muted`; border `border`.
  - Borders by position:
    - First selected: `pl_px border_r_1 pb_px`. First unselected: `pl_px pr_px border_b_1`.
    - Last selected: `border_l_1 border_r_1 pb_px`. Last unselected: `pl_px border_b_1 border_r_1`.
    - Middle(Equal): `border_l_1 border_r_1 pb_px`. Middle(Less): `border_l_1 pr_px border_b_1`. Middle(Greater): `border_r_1 pl_px border_b_1`.
  - Inner row `px 4 gap 4`; start slot 12×12 px; end slot 14×14 px.
  - `Tab::container_height(cx)` = `Base32.px(cx)`.
- Disclosure: an IconButton with ChevronRight (closed) or ChevronDown (open), Small, Muted.
- Chip: `h_flex gap_0p5 px_1 border_1 rounded_sm`, border `border`, bg `element_background`, `overflow_hidden`. Label in the buffer font, XSmall, Default.
- Indicator dot: 6×6, `rounded_full`.
- Lists:
  - ListItem:
    - Inset `ml(indent × 12)` + `px 4`; inner `rounded_sm px 6 gap_1`.
    - Spacing: Dense none, ExtraDense `py_neg_px`, Sparse `py_1`.
    - Selectable: hover `ghost_element_hover`, active `ghost_element_active`, selected `ghost_element_selected`. Focused adds `border_1` (`border_focused` when focused and enabled).
  - ListSubHeader: `pb 4 px 2`, inner `h_5`, `px_2` when inset, label Small Muted.
  - ListSeparator: 1 px, `my 6`, `border_variant`.
  - List: `py 4`.
- ContextMenu:
  - Wrapper with a rem override = UI font size; `occlude`; UI font; line height 1.618; `elevation_2`.
  - Inner: `max_h` 75% of the viewport; `min_w` 200 (or a fixed width + `overflow_x_hidden`); `overflow_y_scroll`; key context `menu`.
  - Rows:
    - Inset ListItem, Dense by default. The label is 14 Default (Disabled when disabled), truncated.
    - The icon is Small with gap 6. `ContextMenuEntry::new` puts the icon at the start; `ContextMenu::entry` puts it at the end.
    - A toggled row shows a Check (or the entry icon), Small, in `icon_color` or Accent; it is invisible when off.
    - The key hint sits `ml_4` after the label.
  - Submenu:
    - Opens on hover or Right/Enter. It flips left when the right edge + 200 px passes the window.
    - `anchored` with `snap_to_window_with_margin(8)`, offset −2.
    - Safety zone: 50 px padding; the pointer threshold is x − 50 on hover and x − 100 on move.
  - Timings: a key-dispatched action selects the row, waits 50 ms, then cancels and dispatches. The parent ignores blur for 200 ms after a submenu cancels, and for 150 ms after a keyboard open.
  - Keys:
    - `confirm` with no selection dismisses.
    - Up/Down skip rows that cannot be selected, and wrap. Home/End go to the ends.
    - With a11y on, the checked row (else the first) is selected on focus.
  - It dismisses on blur unless a submenu holds focus. `DismissEvent`, `Focusable` and the `Entity<ContextMenu>` type stay.
- Tooltip: outer `pl_2 pt_2p5`; card `elevation_2`, UI font, 14, `text`, `py_1 px_2`; title max width 288; meta label Small Muted.
- `TRAFFIC_LIGHT_PADDING`: 78 when the macOS SDK major is 26 or more at build time, else 71 (D21).
- Gotchas to keep:
  - `Label::buffer_font` sets the weight to the buffer font weight. The last of `.weight()` and `.buffer_font()` wins, so the Kbd chip (`components/chips.rs:151-157`) renders at 400.
  - `truncate` = `min_w_0 + overflow_x_hidden + whitespace_nowrap + text_ellipsis`.
  - `single_line` turns `\n` into U+23CE, DEL into U+2421, and other ASCII controls into U+2400+n.
  - Alpha = `color.fade_out(1 - alpha)`.
  - ButtonLike `on_mouse_down(Left)` calls `window.prevent_default()` (a click does not steal focus from a text box). `on_click` calls `cx.stop_propagation()` first. Disabled attaches neither and sets `tab_stop(false)`. Bubble listeners run in reverse order (`zed:crates/gpui/src/window.rs:5868`).
- Colour literals: D24.

### 6.8 Theme and settings (`natamux-theme`, S9)

- Same names as Zed, so callers do not change:
  - `Appearance {Light, Dark}` (`is_light`, `From<WindowAppearance>`: Dark/VibrantDark → Dark).
  - `SystemAppearance` global; `Theme {id, name, appearance, styles}`.
  - `colors()`, `status()`, `accents()`, `players()`, `syntax()`, `darken(Hsla, light, dark)`.
  - `ThemeColors` (all 143 field names), `StatusColors` (42), `AccentColors` (`color_for_index(i) = accents[i % len]`), `PlayerColors`.
  - `SyntaxTheme` with the longest-dot-prefix lookup. Each entry is a whole highlight style: colour, `font_weight`, `font_style` (and any other field the theme JSON sets), not only a colour.
  - `trait ActiveTheme`; `GlobalTheme` (observable: `cx.observe_global::<GlobalTheme>` in `git/languages.rs:66` keeps working).
  - `ThemeRegistry`; `init(assets, cx)`, `reload_theme(cx)`, `init_for_tests(cx)` (loads Natamux Dark); `setup_ui_font(window, cx)` (sets the rem size = UI font size, returns the UI font); `theme_settings(cx)` with `ui_font`, `buffer_font`, `ui_font_size`, `buffer_font_size`; `set_appearance(cx, f)`.
- Loader:
  - It reads the existing JSON files unchanged, with dotted keys → fields (`.` → `_`).
  - Colour path, kept bit-identical: `gpui::Rgba::try_from(str)` → `palette::Srgba` → `palette::Hsla` → `hsla(hue.into_positive_degrees() / 360, s, l, a)`, with palette 0.7.6 (`zed:crates/theme/src/schema.rs:18-27`). Plain `hue / 360` is wrong: palette gives negative hues for blues and purples.
  - Missing keys: every field a theme JSON does not set takes its value only from the S9.1 dump data (the final value per bundled theme × mode, stored as data). No derivation code: Zed derives many fields (`zed:crates/theme_settings/src/schema.rs:254-840`, for example `editor_document_highlight_bracket_background`, the diff hunk fill, hollow and border colours, panel, tab and minimap fallbacks), and a partial list would miss some. The app ships only its bundled themes (`app/theme_catalog.rs:19-47`), so the dump covers every case. Never copy `default_colors.rs`. Natamux Light lacks `version_control.conflict_marker.ours/theirs`.
  - The dump (7.6) includes the `syntax` entries with their font fields.
- Light or dark:
  - The setting `{mode: system|light|dark, light, dark}`; the `--theme` flag overrides the mode.
  - System follows `SystemAppearance`, kept current by `app/window_watch.rs:48-53`.
  - The theme reloads only when the picked name changes.
  - Menu order and defaults stay in `app/theme_catalog.rs:19-47`.
- `appearance_rules::user_settings` returns typed values instead of JSON. Its tests keep the same numbers.
- Call sites that change:
  - `main.rs:389-406`: remove `release_channel::init` and `settings::init`; call `theme::init`.
  - `app/test_views.rs:11-17`: use `init_for_tests`. It loads the bundled themes and sets Natamux Dark, which the tests already use since S1.1 (7.5 H1), so no test sees a new theme in S9.
  - `crates/natamux-ui/Cargo.toml`: its `theme` key (Zed's crate, added in S8) becomes `theme = { package = "natamux-theme", path = "../natamux-theme" }`, and its prelude `ActiveTheme` comes from it. Else its widgets read a `GlobalTheme` nobody sets and panic at run time.
  - `app/appearance.rs:6,46-48`, `app/window_render.rs:230`, `app/window_watch.rs:50-52`.
  - `app/assets.rs:6`; `app/asset_table.rs:93` (`keeps_zed_path` goes).

### 6.9 Assets (S10)

- Fonts, unchanged bytes. Check the sha256 prefixes:
  - IBM Plex Sans 3.005: Regular `975dcda37d80f038`, Italic `a9c6ef9942c49e49`, SemiBold `a20caf8286023a6a`, SemiBoldItalic `147106b619423a82`, with `license.txt`.
  - Lilex 2.601: Regular `8a467ad5a1941dc7`, Bold `3369214e2ecddb45`, Italic `a6e52046b91ed12a`, BoldItalic `9629bd65ee737687`, with `OFL.txt`.
  - Paths stay `fonts/ibm-plex-sans/*`, `fonts/lilex/*`. `load_fonts` adds them before the first frame.
- UI icons (D2):
  - Lucide (ISC) drawings at Zed's metrics: `width="16" height="16" viewBox="-1.333 -1.333 26.667 26.667"`, `stroke-width="2"`, `fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round"`. That gives a 12 px content box and a 1.2 px stroke at 16 px. Filled icons add `fill="currentColor"`.
  - Paths stay `icons/<snake_case>.svg`, so `IconName::path()` does not change.
  - Mapping:
    - Arrows → arrow-down/left/up/up-right; Bell/BellOff → bell/bell-off; Book → book-open; CaseSensitive → case-sensitive; Check → check.
    - Chevrons → chevron-*; ChevronDownUp/UpDown → chevrons-down-up/up-down; Close → x; Copy → copy; Dash → minus.
    - Debug → bug; DebugPause → circle-pause; DiffSplit → columns-2; DiffUnified → rows-2; Download → download; Ellipsis → ellipsis.
    - ExpandVertical → unfold-vertical; FoldVertical → fold-vertical; Eye/EyeOff → eye/eye-off.
    - File, FileCode, FileDiff, FileMarkdown, FileTextOutlined → file, file-code, file-diff, file-text, file-text; Filter → list-filter.
    - Folder/FolderAdd → folder/folder-plus; FontSize → a-large-small; GitBranch/GitGraph → git-branch/git-graph.
    - Image, Info, Keyboard, ListTodo, ListTree → same names; LoadCircle → loader-circle; MagnifyingGlass → search; Menu → menu.
    - Maximize/Minimize → maximize-2/minimize-2; Paperclip; Pencil; Person → user; Pin/Unpin → pin/pin-off.
    - PlayFilled → play (filled); PlayOutlined → circle-play; Plus; PullRequest → git-pull-request; Regex → regex.
    - RotateCcw/RotateCw; Settings; Sparkle → sparkles; Split → square-split-horizontal; Table; Terminal → terminal; TerminalAlt → square-terminal.
    - TextWrap → wrap-text; ToolHammer → hammer; Trash → trash-2; Undo → undo-2; Warning → triangle-alert; XCircle → circle-x.
  - Check each name against the Lucide release used.
- File icons (D3): 46 stems (`files/file_icon_view.rs:10-100`) plus `folder.svg`. Generic ones from Lucide; language and tool logos from Simple Icons (CC0-1.0). Single colour, tinted by `Color::Default` at 12 px.
- The license files ship in the asset table and its test (`app/asset_table.rs:128-143`): Lucide, lobe-icons, material-icon-theme, themes `LICENSES`, plus the font texts and `LICENSE-simple-icons`. The test that asserts no fonts (`app/asset_table.rs:117-120`) changes.
- Drop the `assets` crate and `keeps_zed_path`.

## 7. Verification gate

### 7.1 Pixel gate

- Tool: `scripts/shots-exact.py` (S0.2), stdlib only. Write it from this spec:
  - `shots-exact.py <base_dir> <new_dir> [--control <dir>] [--out <diff_dir>] [--only <prefix>...] [--strict]`.
  - It reads PNG (8-bit RGB/RGBA, not interlaced) with the stdlib (`zlib`, `struct`) and compares every pixel, alpha ignored.
  - Noise rule (orchestrator decision of 2026-10-30, from B0, C0, C0p and C0b): a pixel whose largest channel difference is 1 (`max(|dr|,|dg|,|db|) <= 1`) counts as equal. Unchanged code gives such isolated pixels from run to run; a lost line, a moved glyph or a colour change differs by far more. The output counts them (`n px within 1`), so they are not hidden. `--strict` turns the rule off: any difference fails. The rule does not cover carets or late highlights; S0.3 fixes those.
  - With `--control` (a second run of the base code), pixels that differ by more than 1 between base and control are moving and may differ.
  - It skips `window-*` (the OS-drawn window shots). It never skips by any other name prefix: state names may start with anything. `NO BASE` fails. `NO NEW` (a base shot missing from the new run, as after a shard that stopped early) fails. A size change fails.
  - `--out` writes `diff-<name>.png` (differing pixels in red over a dimmed copy). The `--out` dir must not be inside `base_dir`, `new_dir` or the control dir (the tool exits with an error if it is), so its own output is never read as a shot.
  - Test case: a state named `diff-x` in both dirs is compared, not skipped.
  - Exit 0 only when all match.
  - `scripts/shots-exact.test.py`: equal images pass; one changed pixel fails; a size change fails; the mask allows a moving pixel; a difference of 1 passes, a difference of 2 fails, and `--strict` makes a difference of 1 fail. CI runs it next to `compare-shots.test.py` (`.github/workflows/rust.yml:157`).
- Baselines:
  - B0 = run 40985373281, already downloaded to the session scratchpad `mit-shots/base-40985373281` (510 states + 4 `window-*`).
  - C0 = the control run of S0.2.
  - C1 = a control run of the S0.3 commit, only if S0.3 ran (its shot-only `cursor_blink: false` changes carets, so C0 no longer fits).
  - B1 and B1m = two runs of the S0.4 harness commit. B1 is the base of every later compare; B1 vs B1m is the moving mask.
  - B2 = the run of S10 after the owner signs the icon change; it is the base for S11-S12.
- Rule per UI step: download `shots-macos` into a new empty folder, then run `python3 -I scripts/shots-exact.py <B1> <run> --control <B1m> --out <diffs>`. The step is done only with 0 differing pixels outside the mask and the noise rule, on every shot.
  - Any difference: look at the diff images with the Read tool, fix, and push again.
  - Never accept a difference without the owner's sign-off.
- Merge-ref trap:
  - A PR run builds the PR merged with the current `main`. The Settings dialog prints the version (`app/settings_view_render.rs:65`), so a `main` release bump changes those shots.
  - Shoot gate runs on `natamux-ci/mit` (it builds the exact commit; D26).
  - If PR runs must be used: merge `main` into the branch at planned points only, and shoot a new control after each merge.
  - Record which commit each run built.
- Do not push to a ref while a wanted shots run is in progress on it: a new push cancels the run, and a cancelled run has no `shots-macos` artifact.
- Windows: `app-windows` uploads `shots-windows` (`.github/workflows/rust.yml:618`). If two runs of B1 match exactly there too, compare it the same way. Else record that Windows has no pixel gate.
- After the PR merges (follow-up, not in this PR): the owner may commit fresh baselines and set `REPORT_ONLY` to empty.

### 7.2 New shot states (S0.4, harness only, on unchanged product code)

- Each one is added to `STATES` (and to `DOCKED` when it is docked) in `.github/workflows/rust.yml:397-443`, to `Shot` in `app/options.rs:41`, and to the fake data. Only states that the old code can draw belong here. What a shot cannot show is covered by tests.
- Terminal (scripted fake screens like `app/fake_terminals.rs`, no PTY):
  - `terminal-look`: SGR 16 colours fg and bg, a 256 sample, a truecolour row; bold, dim, italic, underline (single, double, curly), strikethrough, inverse, hidden; wide CJK, emoji with VS16, Vietnamese with combining marks; box drawing, block elements, quadrants, sextants, shades, Powerline; low-contrast pairs; an OSC 8 link.
  - `terminal-cursors`: steady block, underline and bar (`ESC [ 2 q`, `4 q`, `6 q`) in focused panes, plus one unfocused hollow, and a wide char under the cursor.
  - `terminal-selection`: a selection over two lines plus find matches. The selection is set through the model API in the hook.
  - `terminal-alt-screen`: a full-screen coloured grid. `terminal-scrolled`: history above, shot scrolled up, with "Jump to latest" visible.
  - `terminal-context-menu`: the menu opened by a right click on a word, so the word is selected and Copy is live; it shows the ⌘C/⌘V hints (6.2.6).
  - `terminal-scrollbar`: history above, scrollbars forced on.
- Scrollbars: every shot sets `scrollbar.show = never` today (`app/appearance_rules.rs:148-150`). Add a shot-only `show: always` for the states that end in `-scrollbar` (the `shot` input of `user_settings` carries the state name or a flag for it; its tests cover both). Never in real use.
- Text and code:
  - `text-box-selection`: a focused one-line box with a selection.
  - `text-box-selection-unfocused`: the same box with its selection, focus moved elsewhere (selection at 0.5 opacity, 6.3.4).
  - `workspace-editor-scrollbar`: the editor tab on a long file, scrolled, with a selection (markers on the track), scrollbars forced on; wrap off with a long line, so the horizontal bar shows.
  - `edit-form`: the edit form open.
  - `tree-rename`: a rename in progress.
  - `settings-font-error`.
  - `send-back-long`: an area grown to its max and scrolled.
  - `workspace-docs-source`: the Docs Source view.
  - `workspace-docs-source-focused`: the same, focused, so the read-only caret shows solid (6.3.3).
  - `workspace-docs-rich`: a doc with h1-h6, lists, a task list, a table, a code block, a quote, a link and a rule, plus straight quotes and `--`/`---` (smart punctuation), a bare URL, a footnote, a GFM alert (`> [!NOTE]`), superscript and subscript.
  - `workspace-editor-long`: a 4-digit gutter.
  - `workspace-editor-wrapped`: a `.md` file.
  - `code-<lang>` for each of the 22 languages: one fixture file each, docked only.
- Diff (names start with `changes-diff-`, never `diff-`, so they cannot be mistaken for the compare tool's `diff-<name>.png` files): `changes-diff-side-by-side` (main area ≥ 720 px), `changes-diff-collapsed` (a long file with gaps), `changes-diff-wrapped`, `changes-diff-deleted-only`, `changes-diff-focused` (the diff focused with a caret and a selection, 6.3.3), `changes-diff-scrollbar` (scrollbars forced on, hunk markers on the track).
- Kit:
  - `menu-keyboard`: a menu with the keyboard highlight.
  - `button-focus`: a focused button with its focus ring.
  - `button-disabled-tooltip`.
  - `checkbox-states`: on, off, disabled.
- Themes: a `--theme-name <name>` shot option. States `tasks-table`, `workspace-changes-diff`, `workspace-terminal-running` are shot for each of the 15 other themes (D19).
- Proof that the harness changed nothing else: every old state in B1 equals C1 exactly (outside the mask), or C0 when S0.3 did not run.

### 7.3 Terminal test matrix (S2.1 on the old code; must stay green through S2)

| Id | Kind | What | Runs on both by |
|---|---|---|---|
| T-K1 | golden `press_table.json` | Every key (letters, digits, punctuation, Space, Enter, Tab, Backspace, Esc, arrows, Home/End/PgUp/PgDn/Insert/Delete, F1-F20, keypad) × modifiers (none, Shift, Ctrl, Alt, Ctrl+Shift, Ctrl+Alt, Shift+Alt; Cmd on macOS) × modes (normal, APP_CURSOR, APP_KEYPAD, alt screen) → bytes or "propagates". Plus the row "mode set, no frame": output turns APP_CURSOR on, no frame is drawn, then Up is pressed; it must send the normal-mode bytes (6.2.5). One golden file per OS (7.6) | keystrokes on a focused terminal view; read the recording seam |
| T-K2 | gpui | App chords reach the app: Cmd+C/V, Cmd+F, Mod+Shift+Enter. Esc reaches the shell. On Linux/Windows, Ctrl+B etc. reach the shell | actions + seam |
| T-I1 | gpui | Committed text ("người", "日本語") is sent once through `simulate_input`; a key with `key_char` is not sent twice | both |
| T-I2 | gpui, own code only | Marked text sends nothing; unmark clears; `bounds_for_range` = cursor rect + n × cw; `selected_text_range` = 0..0 | the old handler is private (`zed:crates/terminal_view/src/terminal_element.rs:1771`), so on the old code this is the manual IME gate |
| T-P1 | golden | Draw lists for the `terminal-look` fixture screens: text runs (text, start, cells, RGBA8, weight, style, underline, strike), bg rects, block rects. Scrollbar quads (track, thumb idle, thumb hovered) are taken from the shot `terminal-scrollbar` instead, because the old element paints them in a private path | old: the public `TerminalElement::layout_grid` (`zed:.../terminal_element.rs:438`) on `last_content().cells`; new: `grid_layout` |
| T-C1 | golden | Contrast: 16 named fg × theme bg for all 17 themes, plus 20,000 seeded random pairs; exact after 8-bit quantization | old: `ui::utils::ensure_minimum_contrast` (`zed:crates/ui/src/utils/apca_contrast.rs:147`) |
| T-C2 | golden | Colour conversion: all named, all 256 indexed, sample Spec, per theme | old: public `convert_color` (`zed:.../terminal_element.rs:1972`) |
| T-G1 | gpui | Grid size for pane sizes × scale 1 and 2 × font sizes 12/13/14/20 × scrollbar on/off; top or bottom anchoring | `RemoteTerminal` grid size after layout |
| T-M1 | golden | Mouse reports: press, release, move, drag × 3 buttons × modifiers × SGR/UTF-8/X10 × modes 1000/1002/1003, with columns ≥ 95 and ≥ 223 | mouse events; seam |
| T-W1 | gpui | Wheel: pixel and line deltas, touch phases; reports in mouse mode; `ESC O A/B`; local scroll; Shift; the pane does not bubble | scroll events; seam + `display_offset` |
| T-S1 | gpui | Selection: click; drag under and over 2 px; double click (stops at `─` and `│`); triple; Shift+click extend and anchor in mouse mode; right-click word; copy text, selection kept; typing clears it and scrolls to the bottom | mouse + keys; test clipboard; `selection_text` |
| T-V1 | gpui | Cmd+V: text with bracketed on/off; image → `0x16`; file paths → quoted list; path drop (through `type_text`); text typed before the socket opens is sent once on Open (the pending flush) | test clipboard; seam (D35: all three input paths) |
| T-L1 | gpui | Links: Cmd-hover target (regex, trimming); Cmd+click opens only http/https/mailto; OSC 8 plain click (records today, D5); `file://` does nothing | a recorder for `open_url` |
| T-F1 | gpui | `CSI I`/`CSI O` only with mode 1004; size claim on focus; hollow cursor when unfocused or the menu is focused | seam + cursor shape |
| T-Q1 | gpui | DA, DSR 6, OSC 10/11/4 with the theme colours, text area size; order with other writes | feed output; seam |
| T-B1 | gpui | Blink: off by default; on after DECSCUSR blinking; 500 ms phases; a key shows the cursor; the alt screen never blinks | test clock |
| T-R1 | gpui | Resize and reflow keep text; history limit from the first width; a pixel-only change sends nothing | existing size rules + `get_content` |
| T-R2 | gpui | Change the terminal font through `app/appearance.rs` `apply`: the cached view draws again with no new output, and the shell gets the new grid size (6.1) | `RemoteTerminal` grid size; a draw count |
| T-E1 | gpui | Event loop, with a pace other than 4 ms (6.2.4): a burst within the pace wakes once after the pace; a steady stream gives one batch per pace window; the first event after quiet wakes at once, where quiet is one full pace window with no event; old-generation events are dropped after a replace. Internal queue order: queue scroll, then resize, then select before one frame; the result equals running them in that order | `RemoteTerminal` level; test clock |
| T-SH | shots | `terminal-*` states of 7.2 plus all existing terminal states | exact gate |
| existing | | `remote_terminal_tests.rs` (12), `terminal_parser.rs`, `terminal_screen_tests.rs:50`, `terminal_find_tests.rs:27`, `terminal_size.rs`, `terminal_cache.rs`, `terminal_pane.rs`, `find_rules.rs`, `paste_rules.rs`, `terminal_font_tests.rs`, `app/shortcuts.rs:291-324`, smoke wheel check `app/smoke.rs:97-130` | unchanged |

- Manual gates before the terminal flip (S2.5), by the owner, on the `natamux-app-macos-arm64` artifact:
  - IME gate 1: Telex, VNI, EVKey or OpenKey, Japanese Kotoeri (`docs/rust-phase1.md:124`).
  - Gate 2 run 4 times within its targets: frames over 1 refresh ≤ 1%, over 2 ≤ 0.1%, key to glyph p95 ≤ 20 ms (`docs/rust-phase1.md:135`).
  - Gate 3 hand checks: history, typing, paste, Ctrl+C, Vietnamese, resize, daemon restart with no repeated or lost line (`docs/rust-phase1.md:185`).
- Perf (the rule for every perf proof in this plan):
  - Push the old tip to `natamux-ci/perf-mit-before` and the new one to `natamux-ci/perf-mit-after` (`perf.yml`; each run already runs the flows 5 times, `.github/workflows/perf.yml:56-61`). Run each side 3 times (3 workflow runs, a new empty commit each time, never `gh run rerun`).
  - Compare per metric the median of the 3 run values. Pass: after ≤ before × 1.10 + 1 ms (CPU %, RSS and draw counts: after ≤ before × 1.10). The frame targets of the manual gates hold in every run.
  - Many metrics have no baseline value (`null` in `scripts/perf-baseline.json`, for example `diff_click_p95_ms` and `palette_key_p95_ms`), so the before runs are the only reference. `scripts/perf-check.py` still runs for the metrics it knows.
  - A failure needs a fix, or the owner's sign-off with the numbers.

### 7.4 Text input tests (S3.2 on the old code, through `TextBox`)

- T-KEY-1..n: each binding of 6.3.5 (word moves over `foo_bar  baz.qux`, Home/End on a wrapped row, Cmd+Backspace, PageUp/Down, Mod+A then type), plus the 6 tests in `app/text_boxes.rs:179-233`. Each runs twice, after `text_boxes::bind(cx, true)` and after `bind(cx, false)` (`app/text_boxes.rs:16`), so the macOS bindings are tested on Linux too; and the whole set also runs on `app-macos-tests` (7.6).
- T-BIND-1: every `by_name` binding of `app/text_boxes.rs` builds, for `mac` true and false (6.3.1).
- T-MOUSE-1 (gpui, on the old code; for the inputs, the code view and the diff): press inside a selection then release with no move; move before 300 ms; move after 300 ms and release outside the selection (move; Alt on macOS / Ctrl elsewhere copies) on an editable box and on a read-only view; a press on the gutter (line select); a press on a diff line number (no caret move, propagation stopped); a double click in the diff (word); a mouse down after a listener called `prevent_default` (nothing happens).
- T-KEY-P: Enter, Up, Down, Tab, Esc and Mod+Enter propagate from a one-line box to the parent `on_key_down` and the dialog layer.
- T-CLIP-1: copy and cut with an empty selection; paste `a\r\nb` into a one-line box and into an area (record the result); paste replaces the selection.
- T-UNDO-1: `abc` typed fast is one step; a pause over 300 ms splits it; `set_text` can be undone; redo.
- T-EV-1: `Edited` count and order for typing, paste, undo, `set_text` (same and different text) and `replace_minimal`; `Blurred` on focus loss; events arrive as effects (the handler updates the owner, as `LiveField` does).
- T-MIN-1: `replace_minimal` keeps the caret and the scroll row; the JSON tab follows a Reset; the file editor's `set_disk_text`.
- T-MASK-1: `*` per char; `text()` returns the real text; toggling keeps the caret. Never name the file after "token".
- T-WS-1: LiveField writes on each edit; `sync` does not overwrite a focused field; the "Done when" placeholder follows the stage.
- T-IME-1..10, by calling the `EntityInputHandler` methods on the entity (Zed's `Editor` implements it publicly, `zed:crates/editor/src/input.rs:2798`):
  1. Telex `việt` as mark steps, then a commit: no mark left, one undo step.
  2. A relative replace inside the mark.
  3. An EVKey absolute replace.
  4. A fast Backspace + text, 50 words.
  5. An empty range inserts at the caret.
  6. Enter while composing commits and does not reach the parent; Mod+Enter submits the committed text.
  7. NFD Backspace and Left (record them).
  8. UTF-16 ranges with `đ`, emoji and CJK.
  9. Blur keeps the text and unmarks.
  10. `bounds_for_range` in one-line (scrolled), area (row 3) and full modes.
  - Plus a table test over the 100 words of `crates/natamux-app/assets/ime-words.txt` with the Telex sequence of each.
- T-LOOK-1: line box height, caret x after `abc`, the selection rect of `bc`, the wrap row count at a fixed width, and the gutter width. Recorded from Zed with `debug_bounds`/`bounds_for_range`.
- Manual before the flip (S3.4): the owner runs Tools > IME Check (`ime/ime_check.rs`) with the 100 words in Telex, VNI, EVKey and OpenKey, plus Japanese.
- Perf (rule of 7.3): `palette_key_p95_ms` (42-46 ms on CI today, `docs/perf.md:25,107`) at S3.4; typing in a 24-line area and in the JSON tab with no frame over 1 refresh; at S4.3 the new flow `editor_type_5000` (S1.4).

### 7.5 Workflow tests (S1, on the old code; they must stay green to the end)

- Harness:
  - H1: `app/test_views.rs` `init_app` also does `shortcuts::bind`, `DialogLayer::init`, `MenuLayer::init`, a `Dashboard` with `Source::FakeDaemon`, and a root that renders both layers.
  - H1 also changes the test theme. Today `init` loads `theme::LoadThemes::JustBase` (`app/test_views.rs:16`), Zed's built-in fallback theme, which S9 cannot keep. From S1.1 `init` loads `LoadThemes::All(Box::new(NatamuxAssets))` (as `main.rs:405-407` does) and sets Natamux Dark through the same settings JSON `app/appearance_rules.rs` writes. All colour records (T-Q1 OSC 10/11 answers, T-C2, T-P1, the S8.1 metrics golden, the S9.1 dump) are made under it, and S9's `init_for_tests` gives the same theme. All existing tests must stay green with it on the old code.
  - H2: an input driver (`type_text`, `press`, `paste`, `text_of`, `is_focused`) that uses only keystrokes and `simulate_input`.
  - H4: every flow test runs in `TestAppContext`, so a double lease fails it.
- Tests:
  - T1: one-line and many-line editing keys.
  - T2: one-line propagation of Enter, Tab, Esc, Mod+Enter, Up and Down.
  - T3: single keys `a r e i c n j k / ?` type into every box under `NatamuxDetails` and `NatamuxList`, with no daemon call; with focus outside a box, `a` sends `review.approved` once.
  - T4: paste of `\r\n`, `\r`, a trailing newline, tab, 100 KB, emoji, NFC and NFD Vietnamese.
  - T5: events as effects with no timer.
  - T6: focus. A click on a Button keeps focus in the text box; the Tab ring of each dialog; `window.focus` flips `not_editing` in the cached `DetailsPanel`.
  - T7: IME commit then Enter in AddTask makes one task.
  - T8: `debug_bounds` of each input kind (one-line height; area min lines 4, 6, 8, 9, 3 and growth to max; padding).
  - T10 SendBack:
    - default target; Mod+Enter sends `Reopen` with the trimmed reason, and an empty one sends `None`;
    - `busy` blocks a second send and Esc; an error keeps the dialog;
    - Esc closes and focus returns; the picker changes the target.
  - T11 StagePicker:
    - search; the name fills the id until the id is typed by hand;
    - `own_id` stays None after a programmatic fill;
    - errors show after the first try; adding selects the stage; closing returns focus to Add stage.
  - T12 LiveField/StageFields: a write per key; Use default; a stage switch closes the skills menu; insert chip and insert skill.
  - T13 SendBacksField and IdlePauseField: valid and invalid values; the draft is unchanged on error; outside changes show only when unfocused.
  - T14 JsonTab:
    - valid and invalid JSON;
    - an outside `set_text` keeps the caret and undo;
    - typing during the async apply keeps the typed chars;
    - a late language does not change the text.
  - T15 ProjectSettingsPage:
    - save with the version; conflict → Reload or Overwrite (merge);
    - a broken file asks first; Discard, Reset and Import ask; Export;
    - Mod+Enter is ignored while saving; Esc order; Tab ring.
  - T16 FanOut: defaults (2, Start on); Mod+Enter calls once; Esc and Cancel do not.
  - T17 AddTask: Enter adds once; Mod+Enter or the Start button opens Start a task with the title (via defer, no panic); menus.
  - T18 AddSubtask: Enter and Mod+Enter add once; the old selection comes back; an error keeps the title.
  - T19 EditForm: Mod+Enter saves from every field; a blank title blocks; the title is selected on open; `editing()` is true; Tab behaviour as it is today.
  - T20 StartTask: trust dialog → Not now continues; Approve continues; 409 shows the change; closing while the upload is read still opens the task; dynamic Tab ring.
  - T21 Repo trust: an open dialog + Not now with a `then` that updates the opener; Approve calls `then` once; Esc once.
  - T22 DialogLayer: Esc; busy; Mod+Enter from every field; the Tab ring wraps; a confirm over a dialog; the opener's focus comes back; a scrim click closes only the top one.
  - T23 Menus: each `ContextMenu` user; Up/Down/Home/End/Enter/Space/Esc/Tab/Left/Right; the handler runs once; dismiss restores focus.
  - T24 Stage card actions: each button sends the right `StageAction`; all disabled while busy; a double click sends one request; `a` and the button act the same.
  - T25 Checkbox rows: click, Enter, Space; disabled does nothing; `change()` writes only on a difference.
  - T26 Stage list: Alt+Up/Down keep focus; drag and drop; menu items; turning a stage on and off.
  - T27 Sidebar rename: Enter saves, Esc drops, blur saves, an unchanged title sends nothing.
- F1-F4 (D29), in `--smoke --fake-daemon` on `app-macos` (real macOS input):
  - F1: Start a task.
  - F2: `a` on a task in review.
  - F3: Send back.
  - F4: edit a prompt + Mod+Enter.
- Unchanged and still green: about 1,200 daemon tests (293 in `crates/natamux-daemon/tests/stage_runner/`), 244 proto, 216 core, 95 stages, 175 client, all app rules tests, `live-daemon`, and the e2e groups (they test the daemon and the web client).

### 7.6 Golden fixtures

- Generators are `#[ignore]` tests named `golden_*` that write JSON to `$NATAMUX_GOLDEN_OUT` when it is set.
- They run in the temporary tools workflow (8.4). Commit the JSON under `crates/natamux-app/tests/golden/` (D15). The normal tests compare the current code with the committed JSON.
- Generators that call Zed code go away with their Zed crate; the JSON stays.
- Both OSes:
  - `cargo xtask ci` runs only in `check` (ubuntu). `app-macos` builds and smokes but runs no `cargo test` (`.github/workflows/rust.yml:169-312`). So `cfg(target_os = "macos")` code (key encoding, Cmd-hover links, the macOS text-box bindings) and the macOS rows of T-K1/T-K2 would never run.
  - `Tool: golden` runs on `ubuntu-latest` and on `macos-15` (8.4). Golden files live per OS: `tests/golden/<set>/linux/` and `tests/golden/<set>/macos/`. A test reads the file of the OS it runs on.
  - For the length of this PR, a job `app-macos-tests` (macOS, needs only `changes`) runs `cargo test -p natamux-app --locked` (S0.2). At least the terminal, input and workflow tests must run there; running the whole crate is simpler. It is its own job so the shots do not wait for it (a cold test build takes about 8.5 min) and a failed test does not skip them. The job is removed in S12 with the owner's confirm (D33), or kept if the owner wants it.
- Sets:
  - terminal: T-K1, T-P1, T-C1, T-C2, T-M1 (S2.1);
  - highlight captures T-HL-1: `(byte range, theme key, resolved style)` per fixture file per language, where the style is colour, font weight and font style (6.4), plus the shot files `src/checkout.ts`, `.natamux.json`, `README.md` (S4.1);
  - code-view typing T-CODE-1 (6.4; S4.1);
  - hunks and rows T-DIFF-1: the 6 shapes of `git/diff_view_tests.rs:32-88` plus 20 seeded random edits, plus word-diff fixtures in JS, CSS and JSON with `$ # - . :` and runs such as `==`, `::`, `=>` (S6.1);
  - Markdown blocks T-MD-1: kinds, sizes and margins for the rich doc at zoom 1.0 and 1.077, including smart quotes and dashes, a bare URL, a footnote and a GFM alert (S5.1);
  - theme values: every field of all 17 themes in both modes, the missing-key values, and every `syntax` entry with its colour, `font_weight` and `font_style` (S9.1);
  - widget metrics: bounds and resolved style per widget × state (default, hover, active, disabled, selected, focused) × theme × zoom (S8.1).

### 7.7 Final gate (S12)

- `[ci: all]` on the final tip: every job green (check, app-macos, app-windows, app-shots, live-daemon, cli, e2e groups, daemon-os, supply-chain).
- Exact compare of all shots against B2 (B1 for states with no icon). Differences may only be the version label of the Settings shots, and only if `main` moved; name them in the PR description.
- The terminal matrix (7.3), the input tests (7.4) and the workflow tests (7.5) all green. The manual gates (7.3, 7.4) are signed by the owner on the final artifact.
- `cargo deny` passes with no GPL in the allow list and `[licenses.private] ignore = false`. `scripts/zed-lock-check.py` passes with exactly the 21 kept Zed packages.
- `Tool: npm-licenses` on the final tip shows no GPL in the web bundle (section 10 step 6).

## 8. CI loop for agents

### 8.1 Tags in the head commit message

- On a PR only two things are read, both from the head commit: `[ci: all]` and the shot tags (`scripts/ci-changes.py:242-260`).
  - `[ci: shots]`: 4 shards, every state.
  - A line `Shots: a b c`: 1 shard with only those states. An unknown name fails the step.
  - `[ci: all]`: every job plus 4 shards.
- Once a commit of the PR touches `Cargo.toml`, `Cargo.lock`, `rust.yml` or `xtask/`, every later run runs all jobs (`ALL_PATHS`, `scripts/ci-changes.py:80-89`).
- On `natamux-ci/*` the whole message is read. `app-windows` runs there only with `[ci: all]`.

### 8.2 Loop per commit

1. Write code in rustfmt style: 4 spaces, trailing commas, width 100, edition 2024.
2. Push to `natamux-ci/mit-check` (only `changes`, `check`, `rust-ok`; about 5 min; `--all-errors`; a `fmt-patch` artifact when formatting differs). Download: `gh run download <id> -R lenhatthanh20/natamux -n fmt-patch -D <new empty dir>`, then `git apply <dir>/fmt.patch`. Repeat until `check` is green.
3. For a UI or app change, push to `natamux-ci/mit` with `[ci: shots]` or `[ci: all]` (about 13-16 min). Wait. Download `shots-macos` and run the exact compare.
4. Push the same commit to `feat/mit-license` at milestones (end of each step), with the tag the step names. Do not push again until that run ends.
- `gh` needs `-R lenhatthanh20/natamux` (the shell folder resets). Wait with the Monitor tool or a background loop; plain `sleep` is blocked.
- Never `gh run rerun` a run that uploaded artifacts; make a new commit instead.
- At most one full run at a time: the account has 5 macOS runners, and a full run uses up to 6 macOS jobs (`app-macos`, `app-macos-tests`, 4 shot shards).
- Code under `cfg(target_os)` is compiled only by `app-macos`/`app-windows`, not by `check` (Linux). Keep it small and read it twice. From S0.2 `app-macos-tests` runs `cargo test -p natamux-app` (7.6), so the macOS branches are tested there; a check-only push to `natamux-ci/mit-check` does not run it.

### 8.3 Formatting

- The owner allows only formatting in a container locally, and no installs.
  - `rust:1.98.1-slim` has no rustfmt.
  - The local untagged arm64 `rust` image has only the rustup proxy.
  - So local rustfmt needs a component download, which is not allowed unless D25 says yes.
- Default: the `fmt-patch` artifact (8.2 step 2).
- If D25 allows the download, the recipe: a `natamux-rustup` volume, `--network none`, `--cap-drop ALL`, the repo mounted, `rustfmt --edition 2024` on every `.rs` under `crates` and `xtask`.

### 8.4 Temporary tools workflow (never on the PR branch)

- `.github/workflows/mit-tools.yml` lives only in a commit on top of the PR tip, pushed to `natamux-ci/mit-tools` (the token has the `workflow` scope; D26). Jobs, chosen by a line in the commit message:
  - `Tool: lock`: `cargo metadata --format-version 1 > /dev/null` without `--locked`, then upload `Cargo.lock`. It keeps the existing pins.
  - `Tool: golden`: `cargo test -p natamux-app --locked -- --ignored golden_` with `NATAMUX_GOLDEN_OUT=golden`, as a matrix on `ubuntu-latest` and `macos-15`; upload `golden-linux/` and `golden-macos/` (7.6).
  - `Tool: notices`: the third-party notices generator (D30), then upload.
  - `Tool: npm-licenses`: `pnpm install --frozen-lockfile` then `pnpm licenses list --prod --json` in `packages/web`, then `python3 scripts/npm-licenses-check.py` on the output (fails on any license id containing `GPL`, `AGPL` or `SSPL`, unless the package is in an owner-signed list in the script), then upload the JSON. CI only; never on this machine.
  - `Tool: vet`: `cargo vet prune`, then upload `supply-chain/`.
- Copy the downloaded files into the PR branch commit. The workflow file itself never goes to `feat/mit-license`.
- New dependency edges use the exact versions already in the lock (DT10). Pin with `=x.y.z` only if `cargo metadata` would pick another version.

### 8.5 Temporary feature step (DT6)

- In S2.2 and S3.3, add one step to the `check` job after `cargo xtask ci`:
  - `cargo clippy -p natamux-app --all-targets --locked --features own-terminal -- -D warnings`;
  - `cargo test -p natamux-app --locked --features own-terminal`.
- `own-input` is added in S3.3.
- The step is removed in S2.6 and S3.5.
- `xtask layers` and `xtask colours` rules apply to the new code. `natamux-ui` and `natamux-theme` go into `UI_CRATES` (`xtask/src/main.rs:7`).

## 9. Steps

Each step lists: Goal, Files, Tests first, Tag (the head commit message of the push that proves it), Proof (CI jobs and shot states), Risks. "All shots" means every state of `STATES` plus the 7.2 states, exact against B1 with the B1m mask.

### S0. Gate and baseline (no product change)

S0.1 Merge `main` (D27)
- Goal: the branch is on `main` bb723790 (v1.0.9), so later runs and the version label match.
- Files: none (merge commit).
- Tag: none. S0.2 follows at once.
- Risk: none.

S0.2 Exact compare script and control run
- Goal: the gate tool, and C0 to learn which pixels move with unchanged code.
- Files:
  - `scripts/shots-exact.py`, `scripts/shots-exact.test.py`;
  - `scripts/zed-lock-check.py`, `scripts/zed-lock-check.test.py`, `scripts/zed-lock-crates.txt` (today's 106 names; 3.4);
  - `.github/workflows/rust.yml`: next to line 157, run both tests and `python3 scripts/zed-lock-check.py`; a job `app-macos-tests` with `cargo test -p natamux-app --locked` (7.6).
- Tests first: `shots-exact.test.py`, `zed-lock-check.test.py` (a lock with an extra Zed package fails, a missing one fails, an exact one passes).
- Tag: `[ci: all]` (the `app-macos-tests` job must run).
- Proof: `check`, `app-macos` and `app-macos-tests` green; C0 downloaded and compared with B0. Report each moving shot and box to the orchestrator.
- Risks:
  - The traffic lights or the caret move (B0 has a red light in 510 of 510 shots; the old baselines had grey ones).
  - Slow states get shot early (`sleep 3` per shot, `.github/workflows/rust.yml:479`).

S0.3 Harness fixes (only if C0 shows moving pixels; D14)
- Goal: deterministic shots.
- Files:
  - `app/appearance_rules.rs:148-150` (shot-only `"cursor_blink": false` next to the scrollbar line) and its tests;
  - `app/shots.rs` (a longer settle for slow states: `SETTLE` 800 ms at `app/shots.rs:28`, `PANES_DRAWN` 1200 ms at `:53`).
  - Done in S0.3: the controls showed one late state, not a slow one. `workspace-changes-diff` lost the bracket box behind the `}` of line 7 in one run (C0b), because Zed looks for brackets only when the cursor moves and the step to the change came before the TypeScript parse ended. `app/shots.rs` now waits until `DiffView::parsed` (`git/diff_view.rs`, through `ChangesView::shot_parsed`) says the file is drawn with its language parsed, checked every 50 ms for at most 5 s, before the step. `SETTLE` and `PANES_DRAWN` stay. The first S0.3 run (41018471510) also caught three slow starts (`workspace-terminal-needs-input` with no tasks yet, `load-error` before the error showed, `project-settings-problems` before its scroll): the CI loop slept a fixed 3 s from the process start. Now the app prints `shot: ready` when its steps are done plus 500 ms, and no earlier than 2.5 s after the window opens; the `app-shots` loop waits for that line (at most 20 s, else the job fails) and then shoots. One run of the S0.3 tree had `details-more-menu-dark-1024x720` at 1024x682 instead of 1024x684: macOS keeps the window above the Dock, and the Dock's height moved. So `app-shots` hides the Dock before the shots (the 1024x720 shots now come out 1024x720, every one of them changed, before B1), and the loop fails a shot whose size is not the one it asked for.
- Tag: `[ci: shots]`.
- Proof: two runs differ only by the OS-drawn traffic lights, or not at all. One of them is C1, the new control for S0.4.
- Risk: a fix changes product behaviour outside `--shot`. Gate every change on `shot`.

S0.4 New shot states and theme option (7.2)
- Goal: a baseline for every state the later steps need.
- Files: `app/options.rs`, `app/shots.rs`, `app/fake_terminals.rs`, `app/fake_dashboard.json` and `fake_files.json` (fake data), `app/appearance_rules.rs` and its tests (the shot-only `show: always` for `*-scrollbar` states), `.github/workflows/rust.yml` `STATES`/`DOCKED`.
- Tag: `[ci: shots]`. Shoot the same commit twice (B1, B1m), on two `natamux-ci` refs or an empty second commit.
- Proof:
  - The old states in B1 equal C1 exactly (C0 if S0.3 did not run).
  - The owner looks at the new states.
  - B1 vs B1m gives the mask.
- Risk: hooks that need GPL internals (terminal marked text): leave them out (T-I2 covers them).

### S1. Workflow safety net (tests only, old code)

S1.1 Harness H1, H2, H4
- Files: `app/test_views.rs` (also the Natamux test theme, 7.5 H1) and the new helpers in `app/test_driver.rs`.
- Tag: none (`check` only). Push to `natamux-ci/mit-check`, then the PR.
- Proof: all existing tests green under Natamux Dark.

S1.2 Workflow tests T1-T27 (7.5)
- Files: new `*_tests.rs` next to each view (`stages/send_back_dialog_tests.rs`, `stages/stage_picker_tests.rs`, `projects/json_tab_tests.rs`, `projects/project_settings_page_tests.rs`, `tasks/fan_out_dialog_tests.rs`, `tasks/add_task_dialog_tests.rs`, `tasks/add_subtask_dialog_tests.rs`, `tasks/edit_form_tests.rs`, `workspaces/new_workspace_dialog_tests.rs`, `components/dialog_tests.rs`, `components/menu_tests.rs`, `stages/stage_actions_tests.rs`, `sidebar/tree_rename_tests.rs`, `components/form_tests.rs`). Several commits by area.
- Tag: none.
- Proof: green on the old code.
- Risk: a test that records today's behaviour finds a real bug. Record it as it is, tell the owner, and do not fix it in this PR unless the owner says so.

S1.3 Whole-app flows F1-F4 (D29)
- Files: `app/smoke.rs` (or a new `app/flow_smoke.rs`), the `app-macos` steps in `rust.yml`.
- Tag: `[ci: all]`.
- Proof: `app-macos` green.

S1.4 Perf flows for the big views (old code)
- Goal: a "before" number for the speed claims of S4.3 and S6.2.
- Files: `app/flow_bench.rs` (two flows in the existing `--bench-flows` set): `editor_type_5000` (type 20 chars into a 5,000-line file in the editor tab; time per key to the drawn frame) and `diff_open_10000` (open a 10,000-line diff; time from the texts arriving to the first drawn frame), with the fake data they need.
- Tag: none; then `natamux-ci/perf-mit-before` 3 times (7.3 rule).
- Proof: both metrics appear in `perf/flows.txt`.

### S2. Terminal (priority 1)

S2.1 Seams and terminal tests on the old code
- Goal: the full matrix of 7.3, green on Zed's terminal.
- Files:
  - `terminals/terminal_find.rs` (calls through `RemoteTerminal`);
  - `terminals/remote_terminal.rs` and `terminals/remote_terminal_events.rs` (D35: the three calls of `TerminalHandle::input` go through one `send_input`, which holds the `#[cfg(test)]` recorder; a grep test checks no other call exists);
  - new `terminals/term_behaviour_tests.rs`, `terminals/term_golden_tests.rs`;
  - fixtures `crates/natamux-app/tests/golden/terminal/{linux,macos}/*.json` (from `Tool: golden` on both OSes).
- Tests first: T-K1, T-K2, T-I1, T-P1, T-C1, T-C2, T-G1, T-M1, T-W1, T-S1, T-V1, T-L1, T-F1, T-Q1, T-B1, T-R1, T-R2, T-E1.
- Tag: `[ci: all]`.
- Proof: `check` and `app-macos-tests` green; all shots exact (no visible change).
- Risks:
  - `send_input` must keep every path's behaviour: same text, same order, same `from_utf8_lossy` in the input task only.
  - The test platform's key path differs from macOS (`prefer_character_input`). The table records the test platform; the manual IME gate covers macOS.
  - The seam must not change runtime behaviour: it is `cfg(test)` only.

S2.2 Feature, Appearance global, pure modules
- Goal: own colours, contrast, input bytes, mouse reports, grid math and the link regex pass the goldens.
- Files:
  - `crates/natamux-app/Cargo.toml` (`[features] own-terminal = []`, edges `regex`, `shlex`, `unicode-width`, `alacritty_terminal` if missing; lock via `Tool: lock`);
  - `terminals/term/{colors,contrast,input_bytes,mouse_report,grid_layout,links}.rs`;
  - `app/appearance.rs` (the 6.1 global, with `#[cfg_attr(not(test), allow(dead_code))]` until S2.4);
  - `.github/workflows/rust.yml` (8.5 step);
  - `THIRD-PARTY-NOTICES.md` (start it: alacritty Apache-2.0 for the ported code). This file is hand-written and is never overwritten by a generator (section 10 step 6).
- Tests first: the golden tests pointed at the own modules under the feature; the 6.1 parity test.
- Tag: `[ci: all]`.
- Proof: `check` with and without the feature.
- Risk: clippy dead code under the feature. Keep every module reachable from a test or `#[cfg(feature)]` code; the `Appearance` fields are read only by the parity test until S2.4 (6.1).

S2.3 Model, writer, event loop
- Files: `terminals/term/{model,writer,event_loop}.rs`.
- Tests: T-E1, T-Q1, T-R1, T-F1, and the 12 `remote_terminal_tests.rs` under the feature.
- Tag: `[ci: all]`.
- Proof: `check` both ways.
- Risk: races (a writer against a replace, the generation filter, fair against unfair lock). Keep the same design; T-E1 and the swap tests.

S2.4 View, element, scrollbar, IME, menu
- Files: `terminals/term/{view,element,scrollbar,ime}.rs`; the facade in `terminals/remote_terminal.rs` picks the implementation by feature; `app/appearance.rs` (drop the `dead_code` attribute of 6.1, the view now reads the global).
- The view observes the `Appearance` global (6.1). Its menu uses the terminal focus handle as context and handles Zed's `terminal::Copy`/`terminal::Paste` until S2.5 (6.2.6).
- Tests: the whole matrix under the feature, plus T-I2 and the scrollbar thumb-colour test.
- Tag: `[ci: all]`.
- Proof: `check` both ways; `app-macos-tests` both ways.
- Risks: pixel details (selection curves, rect rounding, row snapping, the 14 px strip, theme fallbacks).

S2.5 Flip
- Goal: the own terminal is the default.
- Files: `crates/natamux-app/Cargo.toml` (`default = ["own-terminal"]`), `app/shortcuts.rs:241-242` (own actions `TerminalCopy`/`TerminalPaste`), and the own menu's Copy/Paste rows switched to them in the same commit.
- Tag: `[ci: all]`. Also `natamux-ci/perf-mit-before` (S2.4 tip) and `natamux-ci/perf-mit-after` (S2.5 tip), 3 runs each (7.3 rule).
- Proof:
  - all shots exact, with the `terminal-*` states (including `terminal-context-menu` with its ⌘C/⌘V hints and `terminal-scrollbar`) and every existing terminal state;
  - `--smoke --fake-daemon`; `app-windows` smoke;
  - perf medians within the 7.3 rule;
  - the owner's manual gates of 7.3 on the `natamux-app-macos-arm64` artifact.
- Risk: what the gates find. Fix, then repeat the gates.

S2.6 Remove the old terminal
- Goal: no Zed `terminal`/`terminal_view` in the app.
- Files:
  - remove the dependencies in `crates/natamux-app/Cargo.toml` and in the workspace `Cargo.toml:65-66` (owner confirms, D33);
  - remove the feature, the 8.5 step and the generators that call Zed;
  - update `Cargo.lock` (`Tool: lock`) and shrink `scripts/zed-lock-crates.txt` to the new lock's Zed set.
- Tag: `[ci: all]`.
- Proof: all green (`zed-lock-check.py` included); all shots exact; the goldens still pass.
- Risk: `editor` and `language` remain in the lock (other users); that is expected.

### S3. Text input

S3.1 `TextBox` facade
- Goal: one place builds text inputs; zero behaviour change.
- Files: the 48 files of the inventory, among them:
  - `components/form.rs`, `stages/stage_fields_view.rs`, `stages/stage_picker.rs`, `stages/send_back_dialog.rs`, `stages/prompt_field.rs`, `stages/stage_details_fields.rs`, `stages/branch_details*.rs`;
  - `tasks/add_task_dialog.rs`, `tasks/add_subtask_dialog.rs`, `tasks/edit_form.rs`, `tasks/details_panel.rs`, `tasks/details_run.rs`;
  - `projects/projects_dialog.rs`, `projects/projects_browse.rs`, `projects/send_backs_field.rs`, `projects/idle_pause_field.rs`;
  - `workspaces/new_workspace_*.rs`;
  - `sidebar/sidebar.rs`, `sidebar/tree_rename.rs`, `sidebar/workspace_row.rs`;
  - `app/settings_font.rs`, `app/settings_view.rs`, `app/shortcut_sheet.rs`, `app/sign_in.rs`, `app/window.rs`, `app/smoke.rs`;
  - `palette/palette_view.rs`, `git/changes_boxes.rs`, `git/review_comments.rs`, `terminals/terminal_find.rs`, `ime/ime_check.rs`.
- Tag: `[ci: shots]`.
- Proof: all tests; all shots exact.
- Risk: a wrapper that pushes the wrong focus handle into `tab_order` breaks the Tab rings (T6 catches it).

S3.2 Input tests on the old code (7.4)
- Files: `text_view/text_box_tests.rs`, `text_view/ime_tests.rs`, `text_view/mouse_tests.rs` (T-MOUSE-1 for the boxes), T-BIND-1 in `app/text_boxes.rs`.
- Tag: none.
- Proof: green on `Editor` (Linux and `app-macos-tests`).

S3.3 Own input behind `own-input`
- Files: `text/` (model), `text_view/{text_input,element,ime,blink}.rs`, the feature, the 8.5 step; `app/text_boxes.rs` (own typed actions in their own namespace, bound next to Zed's in the same context, 6.3.1; its 6 tests move to `TextBox`).
- The input observes the `Appearance` global (6.1).
- Tests: the whole 7.4 set and 7.5 set with the feature, including T-BIND-1 and T-MOUSE-1.
- Tag: `[ci: all]`.
- Proof: `check` and `app-macos-tests` both ways.
- Risks:
  - IME (Zed issue #64707 was this class).
  - Typing latency: shape only changed or visible lines; notify only the input.

S3.4 Flip
- Files: `Cargo.toml` default features.
- Tag: `[ci: all]`.
- Proof:
  - all shots exact (D14 holds the caret);
  - F1-F4; `palette_key_p95_ms` on perf (3 runs each side, 7.3 rule);
  - the owner's IME check (7.4).

S3.5 Remove the old input path
- Files: the facade drops the `Editor` arm; remove the feature and the 8.5 step. `app/text_boxes.rs` keeps the Zed half of its bindings: the code view and the diff still use Zed's `Editor` until S4.3 and S6.2. The Zed half and `by_name` go in S7.
- Tag: `[ci: all]`.
- Proof: all green and exact.
- Risk: `editor` stays for the code and diff views until S7.

### S4. Highlighting and code view

S4.1 Goldens on the old code
- Files: `git/languages_golden_tests.rs` (T-HL-1 generator, with resolved styles), `text_view/code_typing_tests.rs` (T-CODE-1), `text_view/code_mouse_tests.rs` (T-MOUSE-1 gutter cases), fixtures under `crates/natamux-app/tests/golden/highlight/{linux,macos}/` and `golden/code_typing/{linux,macos}/`, the fixture source files under `crates/natamux-app/tests/fixtures/code/`.
- Tag: none. Run `Tool: golden`.

S4.2 Own highlighter
- Files:
  - `git/languages.rs` (rewritten; the same 22 names and lookup);
  - `text_view/highlight.rs`;
  - `crates/natamux-app/assets/queries/<lang>/{highlights,injections,extra}.scm` with notices;
  - `Cargo.toml` edges to tree-sitter and the 17 grammar crates (`Tool: lock`).
- Tests: T-HL-1 must match (colour, weight and style).
- Tag: none (the highlighter is not used yet).
- Risks:
  - overlap rules and capture renames (6.4, D18);
  - nvim-only predicates and directives in the markdown fork's queries (6.4): check each file first, before relying on it.

S4.3 Code view and its users
- Files: `text_view/code_view.rs`, `files/file_editor_body.rs`, `projects/json_tab.rs`, `workspaces/docs_tab.rs`, `workspaces/docs_view.rs`, `git/diff_editors.rs` (`plain_switches` goes once unused).
- Tests: T-MIN-1, T14, T-LOOK gutter, T-CODE-1, T-MOUSE-1 (gutter), T-SB-1.
- The code view observes the `Appearance` global (6.1).
- Tag: `[ci: shots]`, then `natamux-ci/perf-mit-after` 3 times.
- Proof:
  - all shots exact, including `workspace-editor*` (with `workspace-editor-scrollbar`), `workspace-docs-edit`, `workspace-docs-source`, `workspace-docs-source-focused`, `project-settings-tab-json`, `project-settings-json-error`, `details-files` and `code-<lang>`;
  - T14 and T-CODE-1 green;
  - `editor_type_5000` within the 7.3 rule against the S1.4 before runs.
- Risks: syntax colours; ligatures; the JSON tab losing typed chars.

### S5. Markdown view

S5.1 T-MD-1 golden on the old code
- Tag: none.

S5.2 Own view
- Files: `workspaces/docs_markdown.rs` (only this file in the app); the `pulldown-cmark` and `linkify` edges.
- Tag: `Shots: workspace-docs workspace-docs-rich` on snapshots, then `[ci: shots]` on the PR.
- Proof: exact shots; T-MD-1; the 2 existing tests.

### S6. Diff view

S6.1 T-DIFF-1 golden and diff tests on the old code
- T-DIFF-1 with the JS, CSS and JSON word-diff fixtures (7.6); T-MOUSE-1 diff cases (line number, gutter, double click, drag in a read-only view).
- Tag: none.

S6.2 Own diff model and view
- Files: `git/diff_model.rs`, `git/diff_view.rs`, `git/diff_editors.rs`, `git/diff_cursor.rs`, `git/review_blocks.rs`, `git/changes_view.rs`, `git/changes_boxes.rs`; port `git/diff_view_tests.rs` (7 tests); the `imara-diff` edge.
- Tests: T-DIFF-1, T-MOUSE-1 (diff), T-SB-1 (diff case).
- Tag: `[ci: shots]`, then `natamux-ci/perf-mit-after` 3 times.
- Proof:
  - exact shots: `workspace-changes-*`, `changes-over-main`, `image-diff`, `rollback`, `workspace-history`, `history-over-main`, `changes-diff-*`;
  - `diff_open_10000` and `diff_click_p95_ms` within the 7.3 rule against the before runs (S1.4).
- Risks: spacers, comment blocks, scroll sync, gaps; selection feel.

### S7. Remove the editor stack

- Goal: no `editor`, `text`, `language`, `grammars`, `buffer_diff`, `markdown`.
- Files:
  - `crates/natamux-app/Cargo.toml`, the workspace `Cargo.toml` (and drop `util` `debug-embed`, `Cargo.toml:70-71`);
  - `main.rs:408` (`editor::init`), `app/test_views.rs:17`;
  - `git/languages.rs` (old registry gone);
  - `app/text_boxes.rs`: the Zed half of the bindings and `by_name` (6.3.1); T-BIND-1 then checks the own bindings only;
  - `Cargo.lock` (`Tool: lock`) and `scripts/zed-lock-crates.txt`.
  - Owner confirms the removals (D33).
- Tag: `[ci: all]`.
- Proof: all green; all shots exact.

### S8. UI kit

S8.1 `natamux-ui` as a pure re-export (D1)
- Files:
  - `crates/natamux-ui/{Cargo.toml,src/lib.rs}` (`license.workspace = true`; `pub use zed_ui::*` with `zed_ui = { package = "ui", ... }`);
  - `crates/natamux-app/Cargo.toml` (`ui = { package = "natamux-ui", path = "../natamux-ui" }`);
  - `xtask/src/main.rs:7`;
  - the widget-metrics generator (7.6).
- Tag: `[ci: shots]`.
- Proof: all shots exact (nothing renders differently; any difference is a build mistake).

S8.2 Atoms, buttons, tooltip, key hint
- Goal: own `Label`, `Headline`, `Icon`, `h_flex`/`v_flex`, spacing, `ButtonLike`, `Button`, `IconButton`, `Tooltip`, `tooltip_container`, `KeyBinding` (D4).
- Keep re-exporting Zed's plain enums (`Color`, `LabelSize`, `IconSize`, `IconName`, `IconPosition`, `ButtonStyle`, `ButtonSize`) until S8.4, because Zed widgets not yet replaced take them.
- Keep exporting Zed's traits (`Clickable`, `Disableable`, `Toggleable`, `FixedWidth`, `StyledExt` with the elevation helpers) in the prelude until S8.4; the own widgets implement Zed's traits (6.7.1). Own traits come in S8.4.
- Tests: the metrics golden (7.6) for every widget and state; T6, T24.
- Tag: `[ci: shots]`.
- Proof: all shots exact.
- Risk: hover, active and focus states are not in the shots. The metrics golden covers them.

S8.3 ContextMenu and menu actions
- Files: `natamux-ui` `context_menu.rs`, `list*.rs`, and a `menu` dependency on Zed's `menu` crate (already in the graph through Zed `ui`) to re-export its action types; `components/menu.rs` if names change (D22).
- The own `ContextMenu` handles Zed's `menu::*` action types in this step; the own `actions!(menu, ...)` come in S8.4 (6.7.1: two actions with one name panic at start).
- The menu context (`.context(focus_handle)`) gives key hints from the bindings in that context, as Zed's does (the terminal menu relies on it, 6.2.6).
- Tests: T23 and the menu metrics; port the behaviours of the Zed menu tests (focus selection with and without a11y, confirm with no selection dismisses, back navigation over headers).
- Tag: `[ci: shots]`.
- Proof: exact shots including `task-context-menu`, `add-menu`, `account-menu`, `details-more-menu`, `start-task-branch-menu`, `menu-keyboard` and `terminal-context-menu`.

S8.4 Form and list widgets, own enums, drop Zed `ui`
- Goal: own `Checkbox`, `Switch`, `ToggleButtonGroup` family, `Tab`, `Disclosure`, `Chip`, `Indicator`, `ListItem`, `TextSize`, `TRAFFIC_LIGHT_PADDING` (D21), and the own enums with the same variants.
- Also in this step, in one commit: the own traits (`Clickable`, `Disableable`, `Toggleable`, `FixedWidth`, `VisibleOnHover`, `StyledExt`) replace Zed's in the prelude; the own `actions!(menu, ...)` with the same names replace Zed's `menu` types.
- `natamux-ui` no longer depends on Zed `ui` or `menu`. It depends on Zed's `theme` under the key `theme` for colours, and its prelude re-exports `theme::ActiveTheme`; S9.2 re-points that key. `IconName::path()` = `"icons/" + snake_case(name) + ".svg"` (strum).
- Files also: `Cargo.lock` and `scripts/zed-lock-crates.txt` (whatever leaves the lock).
- Tag: `[ci: all]`.
- Proof: all green; all shots exact.

### S9. Theme and settings

S9.1 Theme value dump on the old code
- Files: `crates/natamux-app/tests/golden/theme/*.json` via `Tool: golden` (every field, the missing-key values and the `syntax` font fields, 7.6).
- Tag: none.

S9.2 `natamux-theme`
- Goal: own theme and appearance; no `theme`, `theme_settings`, `settings`, `release_channel`.
- Files: `crates/natamux-theme/` (MIT, L4); the key rename `theme = { package = "natamux-theme", path = "../natamux-theme" }` in both `crates/natamux-app/Cargo.toml` and `crates/natamux-ui/Cargo.toml`, and the `natamux-ui` prelude `ActiveTheme`; the call sites of 6.8; `appearance_rules.rs` and its tests (same numbers); `xtask/src/main.rs:7`; `Cargo.lock` and `scripts/zed-lock-crates.txt`.
- Tests:
  - every field of the dump for all 17 themes × both modes;
  - sizes at 3 zooms;
  - `components/theme_contrast.rs` tests.
- Tag: `[ci: all]`.
- Proof: all green; all shots exact, including the 15-theme sets; a gpui test that a `natamux-ui` widget renders under `natamux-theme` (no `GlobalTheme` panic).
- Risks:
  - a missing fallback changes one colour (the dump test finds it);
  - a crate still on Zed's `theme` reads an unset `GlobalTheme` and panics at run time; `zed-lock-check.py` fails while Zed's `theme` is in the lock.

### S10. Assets (the one planned pixel change)

- Goal: own fonts, icons, file icons and license files; no `assets` crate.
- Files: `crates/natamux-app/assets/fonts/`, `assets/icons/*.svg`, `assets/icons/file_icons/`, `app/asset_table.rs` + tests, `app/assets.rs`, `Cargo.toml`, `Cargo.lock`, `scripts/zed-lock-crates.txt` (down to the 21 of 3.3).
- Tests:
  - a font hash test;
  - every `IconName` path exists in the table;
  - the license files ship.
- Tag: `[ci: shots]` (then `[ci: all]` for the lock).
- Proof:
  - The exact compare against B1 differs only inside icon boxes; the agent lists every differing shot with its boxes.
  - The owner signs off (D2, D3). The run becomes B2.
  - Fonts give 0 differences.
- Risk: a box outside an icon differs (a layout change). That fails the step.

### S11. License switch

- Goal: MIT.
- Files and edits (section 10). Each removal needs the owner's confirm (D33).
- Files also: `scripts/npm-licenses-check.py` and its test (stdlib; reads the `pnpm licenses list --json` output), `about.toml`, `about.hbs`, `THIRD-PARTY-RUST.md`.
- Tag: `[ci: all]`. Before it: `Tool: notices` and `Tool: npm-licenses` on `natamux-ci/mit-tools`.
- Proof:
  - `supply-chain` green with no GPL allowed and `[licenses.private] ignore = false`; `scripts/advisory-dates.test.py` and `advisory-dates.py` green in CI (`.github/workflows/rust.yml:1005-1006`);
  - `zed-lock-check.py` green with the 21 kept crates;
  - `Tool: npm-licenses` green;
  - `python3 -I scripts/readme.test.py` green (it is not in CI); the site build (`site.yml`) green.

### S12. Final gate

- 7.7. The PR description lists the steps, the gate results, the owner's sign-offs and the B1/B2 run ids. No release notes.
- Ask the owner whether the `app-macos-tests` job (S0.2) stays. If not, remove it after the final gate runs (D33 confirm).

## 10. Final switch checklist (S11)

1. `LICENSE`: the MIT text from `git show ddfff51e:LICENSE`, holder line "Copyright (c) 2026 lenhatthanh20 and the Natamux contributors".
2. `Cargo.toml:7`: `license = "MIT"`. All 15 crates and the 2 new ones inherit it (`license.workspace = true`).
3. `deny.toml`:
   - remove `"GPL-3.0-or-later"` (line 25);
   - set `[licenses.private] ignore = false` (lines 42-43; 3.4);
   - remove the unused allow-git entries (dap-types, lsp-types, trash-rs; the grammar forks stay while used);
   - advisories: check every `ignore` entry (`deny.toml:6-21`) and `.cargo/audit.toml:3-11` against the final `Cargo.lock`. Most say "a Zed dependency" or "Zed's ... crates": rsa (RUSTSEC-2023-0071), quick-xml (2026-0194/0195), zstd and zstd-safe via `rust-embed` and `rpc` (2026-0391/0398/0403/0410), async-std (2025-0052), paste (2024-0436), rustybuzz (2026-0206), ttf-parser (2026-0192), zbus_names, zvariant, zvariant_derive (2026-0370/0371/0372). For each: is the crate still in the lock, and through what? Remove the entries whose crate is gone, in both files (owner confirms the list, D33). Reword the reasons of the kept ones to name the real path (for example gpui). Read `scripts/advisory-dates.py` and its test first; they run in CI (`.github/workflows/rust.yml:1005-1006`).
4. `[patch.crates-io]` (`Cargo.toml:80-87`): check each entry (async-process, async-task, calloop, notify, notify-types, tree-sitter-language) against the final lock: an entry stays only while a crate of the graph uses that package (the comment at `Cargo.toml:78-79` says so). Keep `tree-sitter-language` while tree-sitter stays. Remove the entries no crate uses, with their `deny.toml` allow-git lines, only with the owner's confirm (D33).
5. `supply-chain/`: `Tool: vet` prune, then commit.
6. Notices, two files at the repo root, so a generator never overwrites hand-written text:
   - `THIRD-PARTY-NOTICES.md`, hand-written (started in S2.2), for what `cargo about` does not see:
     - Zed Industries, Apache-2.0, the 21 kept crates, unmodified source (3.3);
     - the ported alacritty input code (Apache-2.0);
     - vendored tree-sitter-javascript queries (MIT); nvim-treesitter markdown queries (Apache-2.0), with the list of changes made to them (6.4);
     - gpui `examples/input.rs` if any code was copied;
     - fonts (OFL, with the `OFL.txt`/`license.txt` texts); icons (Lucide ISC, Simple Icons CC0, lobe-icons MIT, material-icon-theme MIT); themes (`themes/LICENSES`);
     - the web bundle the daemon embeds (3.5): each npm package of `Tool: npm-licenses` with its license and copyright line, and the Geist, Geist Mono, Inter and JetBrains Mono fonts with the OFL text. `scripts/npm-licenses-check.py` fails on GPL (8.4).
   - `THIRD-PARTY-RUST.md`, generated by `cargo about` (D30) from the final `Cargo.lock` with the committed `about.toml`/`about.hbs`: every Rust crate (alacritty_terminal, tree-sitter and the grammars, imara-diff, pulldown-cmark, linkify, palette and the registry crates). Never edited by hand; made again with `Tool: notices`.
7. Ship `LICENSE` and both notice files in `Natamux.app/Contents/Resources/` (`scripts/bundle-rust-app.sh:21-28`) and in the Windows and Linux packages (`.github/workflows/release.yml`, `rust.yml`) (D32).
8. `README.md:30`: badge alt "MIT license". `README.md:224-229`: MIT text, plus the note on GPL-era releases (D31).
9. `scripts/readme.test.py:166-170`: assert MIT (it is run by hand, not by CI).
10. Site: `site/src/content/docs/docs/help/faq.md:26`, `site/src/pages/index.astro:375,803,905`.
11. Docs:
    - `docs/plan-rust-migration.md:3,10,157-159,361`: say the license moved to MIT, with a dated decision line;
    - `docs/research-native-app.md:48`;
    - `docs/rust-phase1.md:116`;
    - Do not repeat other products' names from `docs/plan-features.md:186-188`.
12. `app/asset_table.rs` license test lists the new files.
13. No release notes or changelog entry.

## 11. Not done, and why

- No local build, test or format: owner rule. Every proof is a CI run.
- Two Zed files were not read because of the path rule: `zed:crates/terminal/src/mappings/keys.rs` and `zed:crates/ui/src/components/keybinding*.rs`. T-K1 and the S8 metrics golden (with D4) stand in for them.
- No pixel gate on Linux (no GUI CI). Windows only if 7.1 shows its shots are stable.
- Real-input behaviour (IME candidates, trackpad feel, live output) cannot be shot. It is covered by tests, F1-F4 and the owner's manual gates.
- Not ported because they are unused or unreachable:
  - the terminal vi mode;
  - Zed's terminal path-like targets and workspace items;
  - multi-buffer, LSP, completions, folds (D12) and multi-cursor (D11);
  - the Zed widgets listed in 6.7.1.
- Fixes of today's quirks (D5-D9, D17): only when the owner says so. Else follow-ups.
- Committing fresh baseline PNGs and turning `REPORT_ONLY` off: a follow-up after the merge (7.1).
- A Zed (gpui) upgrade: its own PR later. Each upgrade must check again that the kept crates are still Apache-2.0.
- A permanent npm license guard in `rust.yml`: not added. `Tool: npm-licenses` checks the web bundle in S11 and S12 only; a later dependency change in `packages/web` is not checked by CI.
- The registry license list could not be made locally (no registry, no builds); D30 makes it on CI.
- How cargo-deny treats `publish = false` git crates could not be run locally; 3.4 adds `zed-lock-check.py` and `[licenses.private] ignore = false`, so the result does not depend on it.

## 12. Open decisions

None. D1-D36 were decided on 2026-10-30 and are in section 4 with their options, choice and reason.

## 13. Review notes (review round of 2026-10-30)

Each point was checked against the code. Where it went:

| # | Point | Result |
|---|---|---|
| 1 | Sync queue order | Accepted: FIFO in 6.2.4; T-E1 case scroll → resize → select |
| 2 | Event loop repeat | Accepted: steps 1-4 in 6.2.4; "quiet" defined in T-E1. Added: T-E1 needs a non-default pace, because Zed's test build uses a random delay at 4 ms (`zed:crates/terminal/src/terminal.rs:1470-1479`) |
| 3 | Keys read the last frame's mode | Accepted: 6.2.4, 6.2.5; T-K1 row "mode set, no frame" |
| 4 | Seam misses two input paths | Accepted: D35 now one `send_input` for all three paths; T-V1 covers the pending flush |
| 5 | Tests only on Linux | Accepted: `Tool: golden` on both OSes with per-OS files (7.6, 8.4); `app-macos-tests` runs `cargo test -p natamux-app` from S0.2; T-KEY runs `bind(cx, true)` and `bind(cx, false)` |
| 6 | License gate blind to Zed crates | Accepted: `[licenses.private] ignore = false` in S11; `scripts/zed-lock-check.py` from S0.2 (S0.2 instead of S2.6, so a new edge in S2.2 that pulls a Zed crate is caught too) |
| 7 | npm code in the daemon | Accepted: 3.5 row, `Tool: npm-licenses`, notices in section 10 step 6 |
| 8 | Exact compare skips `diff-*` | Accepted: states renamed `changes-diff-*`; the tool skips only `window-*` and refuses an `--out` dir inside the input dirs |
| 9 | No terminal menu shot | Accepted: `terminal-context-menu` in 7.2 |
| 10 | Terminal menu key hints | Accepted: 6.2.6, S2.4, S2.5, S8.3. Own actions keep the names `TerminalCopy`/`TerminalPaste`, because an own `terminal::Copy` would panic next to Zed's (gpui forbids two actions with one name) |
| 11 | Scrollbars unseen; code-view rule wrong | Accepted with one correction: the rule and values are in 6.2.6, 6.4, 6.5; shot states with `show: always`; T-SB-1. Rejected part: "diff hunks keep the bar shown (the diff view)". The diff sides are non-singleton multibuffers (`git/diff_view.rs:286,300`) and do not set `allow_git_diff_scrollbar_markers`, so in the diff only an off-screen cursor or a recent scroll shows the bar (`zed:crates/editor/src/element.rs:1513-1530`). The hunk and selected-text cases apply to the single-file views |
| 12 | Read-only views show a caret | Accepted: 6.3.3, 6.3.4, 6.5; shots `changes-diff-focused`, `workspace-docs-source-focused` |
| 13 | Unfocused selection at 0.5 | Accepted: 6.3.4, 6.5; shot `text-box-selection-unfocused` |
| 14 | Missing mouse behaviours | Accepted: 6.3.3, 6.5, T-MOUSE-1. Drag-move kept as D36 (parity) |
| 15 | Language table too thin | Accepted: 6.4 table facts and T-CODE-1 |
| 16 | Word-diff tokenizer | Accepted: 6.5; JS/CSS/JSON fixtures in T-DIFF-1 |
| 17 | Syntax weight and italics | Accepted: 6.4, 6.8, T-HL-1, theme dump |
| 18 | `into_positive_degrees()` | Accepted: 6.8 |
| 19 | Derivation list incomplete | Accepted: the partial list is gone; missing keys come only from the S9.1 dump |
| 20 | Test theme changes in S9 | Accepted: S1.1 makes `test_views::init` load the bundled themes and set Natamux Dark (7.5 H1), before any colour record |
| 21 | `natamux-ui` still on Zed `theme` | Accepted (the crate is created in S8; its `theme` key is added in S8.4): S8.4 and S9.2 |
| 22 | Mixed traits in S8.2 | Accepted: 6.7.1, S8.2, S8.4 |
| 23 | Text-box keys bind Zed action types | Accepted, with a different fix: own actions cannot reuse the `editor::` names while Zed's `editor` is linked (gpui panics on a duplicate name, `zed:crates/gpui/src/action.rs:53,293-311`), so the own input uses its own namespace and `text_boxes.rs` binds both until S7 (6.3.1). T-BIND-1 added |
| 24 | Markdown options | Accepted: 6.6, DT10 (`linkify` 0.10.0), `workspace-docs-rich`, T-MD-1 |
| 25 | Markdown sizes are rems | Accepted: 6.6 |
| 26 | nvim-only query parts | Accepted as a risk to check (the fork's files are not local): 6.4, S4.2 |
| 27 | Background reparse API | Accepted: 6.4 (`parse_with_options`, progress callback, `parser.reset()`, snapshot and generation) |
| 28 | Cached views miss appearance changes | Accepted: 6.1 observers; T-R2 |
| 29 | S2.2 clippy dead code | Accepted: 6.1, S2.2, S2.4 |
| 30 | Baseline proof after S0.3 | Accepted: C1 (7.1, S0.3, S0.4, 7.2) |
| 31 | Weak perf proofs | Accepted: 7.3 rule (3 runs per side, medians, tolerance); S1.4 adds `editor_type_5000` and `diff_open_10000` on the old code; S3.4, S4.3, S6.2 use them |
| 32 | Notices overwritten | Accepted: hand file `THIRD-PARTY-NOTICES.md` and generated `THIRD-PARTY-RUST.md` (D30, section 10 step 6) |
| 33 | S11 cleanup incomplete | Accepted: section 10 steps 3-4 check every advisory ignore and patch entry against the final lock |
| extra | Own `menu::*` actions in S8.3 | Found while checking point 23: Zed's `menu` crate stays linked with Zed `ui` until S8.4, so own actions with the same names would panic at start. S8.3 handles Zed's `menu` types; S8.4 defines the own ones (6.7.1) |
