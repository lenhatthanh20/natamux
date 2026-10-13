# Research: a native desktop app

Status: research, 2026-10-13. It proposes; it does not change the decision in `plan-tech-stack.md` section 6. Every outside source was checked on 2026-10-13. Facts about Natamux cite the repo file.

## Question

- Which stack should Natamux use to become a "native app" on macOS and Windows (Linux nice to have)?
- Which other ADEs and IDEs already ship native apps, and with which stack?
- Is a move worth it compared with the current Electron shell, and what does it cost?

## Options A/B/C

The desktop app is already an Electron shell (`packages/desktop`, about 2,100 lines by `wc -l`). It starts the Node daemon in a utility process and shows the dashboard the daemon serves (`packages/desktop/README.md`). "Native" can mean three depths:

| Option | What changes | What stays |
|---|---|---|
| **A. Native shell, web UI** | Electron is replaced by a shell over the system webview: Tauri 2, Electrobun, Deno Desktop, Wails, or a thin Swift / WinUI host | Daemon (Node), dashboard (React, xterm.js), browser tab |
| **B. Native UI, same daemon** | The dashboard (`packages/web`, about 26,600 lines in 160 files) is rewritten in native UI: SwiftUI/AppKit + WinUI 3, or one toolkit such as GPUI or Flutter. It talks to the daemon over the same HTTP and WebSocket API | Daemon, CLI |
| **C. Full native rewrite** | Daemon (`packages/daemon`, about 27,200 lines) and UI in a compiled language (Rust, Swift, Go). Terminal through libghostty or a Rust PTY and renderer | Task file format, docs |

- Tauri 2 is judged at depth A (Node daemon kept as a sidecar). Tauri with a Rust daemon is depth C, and costs what C costs.
- GPUI and Swift + WinUI are judged at depth B. Moving the daemon too makes them C.
- libghostty is a terminal engine, not a shell. It plugs into B or C only. Under A the terminal stays xterm.js.

## Needs

Every option must meet these, or say what it gives up:

1. Platforms: macOS arm64 and x64, Windows x64 and arm64. Linux is nice to have.
2. Terminals for agent CLIs: a PTY on macOS and Linux, ConPTY on Windows, full xterm-compatible rendering, Vietnamese IME input, large scrollback.
3. The dashboard still works in a plain browser tab and in Codespaces (`plan-tech-stack.md` section 1).
4. One developer can maintain it, with few dependencies (`CLAUDE.md`).
5. Signing, notarization, auto-update.
6. Installer size, memory, start time.

## Comparison table

### Candidates

| Candidate | Option | Language | Maturity | Licence | macOS | Windows | Linux | Code kept |
|---|---|---|---|---|---|---|---|---|
| Electron 44 (today) | baseline | TypeScript | Stable | MIT | arm64, x64 | x64, arm64 (Chromium) | x64 (Chromium); Natamux's Linux release job is paused (`.github/workflows/release.yml`) | All |
| Tauri 2 (v2.12.3) | A | Rust shell + Node sidecar | Stable | Apache-2.0 (GitHub) | arm64, x64 (WKWebView) | x64, arm64 (WebView2) | Yes (WebKitGTK) | Daemon, web, CLI; shell rewritten in Rust |
| Electrobun (v2.0.3-beta.23) | A | TypeScript on Bun | Pre-release | MIT | arm64 only | x64; arm64 in beta | x64, arm64 (WebKitGTK 4.1 or CEF) | Web; daemon only if node-pty runs on Bun (not checked) |
| Deno Desktop (Deno 2.9) | A | TypeScript on Deno | Experimental since 2026-06-25 | MIT | arm64, x64 | x64 only | x64, arm64 (WebKit) | Web; daemon only if node-pty loads (not confirmed) |
| Wails v3 (v3.0.0-beta.28) | A | Go | Beta; v2 is the stable line | MIT | arm64, x64 (not checked per arch) | x64 (arm64 not checked) | Unknown: not checked, because Wails was not on the shortlist | Web; daemon rewritten in Go or kept as a sidecar |
| Swift/AppKit + WinUI 3 / C# | B | Swift, C# | Stable platforms | Swift Apache-2.0, Windows App SDK MIT | arm64, x64 | x64, arm64 | No: needs a third UI (Ghostty uses GTK4 for this) | Daemon, CLI |
| Rust + GPUI (0.2.2) | B | Rust | Pre-1.0, "frequent breaking changes" | `gpui` crate Apache-2.0; Zed editor GPL | arm64, x64 (Metal) | x64 (Direct3D 11) | Yes (wgpu on Vulkan; Wayland or X11) | Daemon, CLI |
| Flutter | B | Dart | Stable | BSD-3-Clause | arm64, x64 | x64 | Yes ([supported platforms](https://docs.flutter.dev/reference/supported-platforms)) | Daemon, CLI |
| libghostty (terminal engine for B or C) | B, C | Zig, C API | `libghostty-vt` usable; API still changing; no tagged release | MIT | Ghostty app ships here | Parser only; no Windows renderer or app | Ghostty app ships here (GTK4) | Not a shell; replaces xterm.js only |

### Needs per candidate

Size, memory and start time come from the benchmark below, unless marked otherwise. Unknown means no benchmark with the same method was found and no prototype exists to measure.

| Candidate | Terminal | Browser tab | Upkeep (one developer) | Signing and update | Size | Memory (idle) | Start time |
|---|---|---|---|---|---|---|---|
| Electron 44 (today) | xterm.js + WebGL in Chromium; node-pty in the utility process. Works today | Kept: same web UI | Done; one language | Own updater (`docs/design-desktop-update.md`); signing waits for accounts | 327 MB on disk (Natamux, measured); 323 MB in the benchmark | 128 MB | 273 ms |
| Tauri 2 | xterm.js in WKWebView / WebView2; node-pty in a packaged Node sidecar is not proven | Kept: same web UI | Adds Rust and sidecar packaging | Tauri updater plugin with its own signing key; OS signing still needed | 57 MB, plus a Node runtime for the daemon (not measured) | 109 MB, without the Node sidecar | 311 ms, without the Node sidecar |
| Electrobun | System webview or bundled CEF; node-pty on Bun not checked | Kept | Adds Bun; pre-release | Not checked, because it was cut from the shortlist | 418 MB with CEF | 208 MB | 773 ms |
| Deno Desktop | WebView2 / WebKit; `deno compile` loads Node-API addons by extracting them to a temp file | Kept | Adds Deno; experimental | Blog says nothing about signing | 111 MB | 98 MB | 242 ms |
| Wails v3 | System webview; PTY must come from Go or a Node sidecar | Kept | Adds Go; beta | Not checked, because it was cut from the shortlist | Unknown: not measured, no benchmark found | Unknown: not measured, no benchmark found | Unknown: not measured, no benchmark found |
| Swift/AppKit + WinUI 3 | macOS: SwiftTerm or libghostty. Windows: no mature ConPTY terminal control found | Lost, unless the web UI is kept as a second UI | Two new UIs in two languages | Sparkle (macOS), WinSparkle (Windows) | Unknown: not measured, no prototype exists | Unknown: not measured, no prototype exists | Unknown: not measured, no prototype exists |
| Rust + GPUI | `alacritty_terminal` + `portable-pty` (as Zed and `gpui-terminal` do) | Lost, unless a second UI is kept | One new UI in Rust, on a pre-1.0 API | Not provided; own updater | Unknown: not measured, no benchmark found | Unknown: not measured, no benchmark found | Unknown: not measured, no benchmark found |
| Flutter | `xterm.dart` (says it works with IMEs); PTY from the daemon | A web build is possible, but it replaces the React dashboard | One new UI in Dart | Not provided; own updater | Unknown: not measured, no benchmark found | Unknown: not measured, no benchmark found | Unknown: not measured, no benchmark found |
| libghostty | Best native terminal on macOS (Ghostty, cmux) | Not applicable | Zig C API, still changing | Not applicable | Not applicable: an engine, not an app | Not applicable: an engine, not an app | Not applicable: an engine, not an app |

With options B and C the Node daemon still runs, so its memory adds to the UI's in every row.

Cut, with the reason:

- **Qt** (LGPL, GPL, or commercial licence): a third language (C++), and for a closed or mixed-licence build the LGPL terms or a paid licence must be managed. No terminal advantage found over GPUI.
- **Compose Multiplatform** (Apache-2.0): a third language (Kotlin) and a JVM runtime in the installer. No research found on its terminal widget for this use.
- **Avalonia** (MIT): a third language (C#) on macOS too. No research found on its terminal widget for this use.
- **Thin Swift / WinUI host over a webview**: the same webview risks as Tauri, plus two shells to write. Tauri does the same job once.

Sources: [Tauri releases](https://github.com/tauri-apps/tauri/releases), [Tauri webviews](https://v2.tauri.app/reference/webview-versions/), [Electrobun releases](https://github.com/blackboardsh/electrobun/releases), [Electrobun targets](https://framework.blackboard.sh/electrobun/guides/compatability), [Deno 2.9](https://deno.com/blog/v2.9), [Deno 2.3 addons](https://deno.com/blog/v2.3), [Wails releases](https://github.com/wailsapp/wails/releases), [SwiftTerm](https://github.com/migueldeicaza/SwiftTerm), [GPUI Cargo.toml](https://github.com/zed-industries/zed/blob/main/crates/gpui/Cargo.toml), [gpui-terminal](https://docs.rs/gpui-terminal), [portable-pty](https://docs.rs/portable-pty), [xterm.dart](https://pub.dev/documentation/xterm/2.6.0/index.html), [Ghostty](https://github.com/ghostty-org/ghostty), [Qt licensing](https://www.qt.io/licensing/), [Sparkle](https://sparkle-project.org/), [WinSparkle](https://winsparkle.org/). Licences from each project's GitHub repo: [Electron](https://github.com/electron/electron), [Deno](https://github.com/denoland/deno), [Flutter](https://github.com/flutter/flutter), [Compose Multiplatform](https://github.com/JetBrains/compose-multiplatform), [Avalonia](https://github.com/AvaloniaUI/Avalonia), [Windows App SDK](https://github.com/microsoft/WindowsAppSDK), [Swift](https://github.com/swiftlang/swift).

### Size, memory, start time

| Framework | Bundle | Start (median of 10) | Idle memory |
|---|---|---|---|
| Tauri | 57 MB | 311 ms | 109 MB |
| Deno Desktop | 111 MB | 242 ms | 98 MB |
| Electron | 323 MB | 273 ms | 128 MB |
| Electrobun (CEF) | 418 MB | 773 ms | 208 MB |

- Source: [Better Stack](https://betterstack.com/community/guides/scaling-nodejs/tauri-vs-electron-vs-deno-vs-electrobun/), updated 2026-07-20. One small app, macOS builds, machine not stated. Indicative only.
- Natamux 0.1.13 on disk: 327 MB (`du -sh /Applications/Natamux.app`, measured 2026-10-13).
- Start time and idle memory are close. The real gain of a webview shell is download size.

## ADE and IDE survey

"Confirmed" = official repo, official docs, or the app bundle. "Reported" = third-party only, or a non-technical official page such as a job post.

### Agent workspaces

| Product | Platforms | Stack | Source | Checked | Status |
|---|---|---|---|---|---|
| Superset | macOS; Linux experimental; Windows not yet | Electron 41 + xterm.js 6 + node-pty | [package.json](https://github.com/superset-sh/superset/blob/main/apps/desktop/package.json) | 2026-10-13 | Confirmed |
| Emdash | macOS, Windows, Linux | Electron + node-pty | [package.json](https://github.com/generalaction/emdash/blob/main/package.json) | 2026-10-13 | Confirmed |
| Crystal → Nimbalyst | Nimbalyst: macOS, Windows, Linux | Electron (Crystal on Electron 37; Nimbalyst has `packages/electron`) | [Crystal](https://github.com/stravu/crystal/blob/main/package.json), [Nimbalyst](https://github.com/nimbalyst/nimbalyst) | 2026-10-13 | Confirmed |
| Sculptor (Imbue) | macOS arm64, Linux | Electron 42 + React + xterm.js; Python backend | [package.json](https://github.com/imbue-ai/sculptor/blob/main/sculptor/frontend/package.json) | 2026-10-13 | Confirmed |
| Claude desktop | macOS, Windows | Electron (bundle has Electron helper apps; 899 MB on disk) | Local `/Applications/Claude.app` 2.31226.1 | 2026-10-13 | Confirmed |
| OpenAI Codex app | macOS (2026-02-02), Windows (2026-03-04) | Electron + React over a bundled Rust `codex` binary | [teardown](https://www.kitze.io/posts/codex-electron-app-technical-breakdown), [HN](https://news.ycombinator.com/item?id=46859054), [TechRepublic](https://www.techrepublic.com/article/news-openai-codex-windows-app-launch/) | 2026-10-13 | Reported |
| Vibe Kanban | Tauri bundle for all OSes; also a local web app. Shutting down | Tauri 2 + Rust server + web UI | [repo](https://github.com/BloopAI/vibe-kanban), [shutdown](https://www.vibekanban.com/blog/shutdown) | 2026-10-13 | Confirmed |
| Jean (coollabsio) | Not checked | Tauri 2 + xterm.js | [repo](https://github.com/coollabsio/jean) | 2026-10-13 | Confirmed |
| cmux (manaflow) | macOS only | Swift + AppKit; libghostty for the terminal | [repo](https://github.com/manaflow-ai/cmux) | 2026-10-13 | Confirmed |
| Conductor (Melty Labs) | macOS Apple silicon only | TypeScript + Tauri + React, per its own job post. No public repo; no app bundle on this machine to check `Contents/Frameworks` | [job post](https://www.ycombinator.com/companies/conductor/jobs/MYjJzBV-product-engineer), [App Store](https://apps.apple.com/us/app/conductor-build/id6791228564), [review](https://thenewstack.io/a-hands-on-review-of-conductor-an-ai-parallel-runner-app/) | 2026-10-13 | Reported |
| Warp | macOS, Windows (2025-02-26), Linux | Rust + own GPU UI (`warpui`); own ConPTY wrapper | [repo](https://github.com/warpdotdev/warp), [Windows post](https://www.warp.dev/blog/launching-warp-on-windows) | 2026-10-13 | Confirmed |

### Editors and terminals

| Product | Platforms | Stack | Source | Checked | Status |
|---|---|---|---|---|---|
| Zed | macOS, Windows (since 2025-10), Linux; 1.0 on 2026-04-29 | Rust + GPUI; Metal, Direct3D 11, wgpu/Vulkan; terminal on `alacritty_terminal` | [repo](https://github.com/zed-industries/zed), [Windows post](https://zed.dev/blog/zed-for-windows-is-here), [1.0.0 notes](https://zed.dev/releases/stable/1.0.0) | 2026-10-13 | Confirmed |
| VS Code | macOS, Windows, Linux | Electron | [repo](https://github.com/microsoft/vscode) | 2026-10-13 | Confirmed |
| Cursor | macOS, Windows, Linux | VS Code fork, Electron (bundle has Electron helper apps) | Local `/Applications/Cursor.app` 3.17.21 | 2026-10-13 | Confirmed |
| Windsurf | macOS, Windows, Linux | VS Code fork | [comparison](https://computingforgeeks.com/cursor-vs-windsurf-vs-kiro/) | 2026-10-13 | Reported |
| Kiro (AWS) | macOS, Windows, Linux | Code OSS fork | [OMG Ubuntu](https://omgubuntu.co.uk/2025/07/kiro-ai-ide-public-preview-linux) | 2026-10-13 | Reported |
| Google Antigravity | macOS, Windows, Linux | VS Code fork | [VS Magazine](https://visualstudiomagazine.com/articles/2025/11/20/google-joins-ai-ide-race-to-compete-with-vs-code-apparently-forking-vs-code.aspx) | 2026-10-13 | Reported |
| JetBrains Air | macOS, Windows (2026-06), Linux | Not stated by JetBrains; said to build on the Fleet platform (Fleet ended 2025-12) | [launch](https://blog.jetbrains.com/air/2026/03/air-launches-as-public-preview-a-new-wave-of-dev-tooling-built-on-26-years-of-experience/), [Fleet end](https://devclass.com/2025/12/09/jetbrains-abandons-fleet-ide-pins-hopes-on-forthcoming-air-agentic-development-tool/) | 2026-10-13 | Platforms confirmed, stack reported |
| Lapce | macOS, Windows, Linux | Rust + Floem + wgpu | [repo](https://github.com/lapce/lapce) | 2026-10-13 | Confirmed |
| Panic Nova | macOS only | Native Mac, own text engine | [Tools & Toys](https://toolsandtoys.net/nova-code-editor-for-mac-by-panic/) | 2026-10-13 | Reported |
| Sublime Text | macOS, Windows, Linux | Fully custom UI framework with an OpenGL or software renderer. Written in C++ per a former Sublime HQ developer (reported) | [Sublime HQ blog](https://www.sublimetext.com/blog/articles/2021/04), [GPU rendering docs](https://sublimetext.com/docs/gpu_rendering.html), [Will Bond](https://wbond.net/thoughts/building_a_high_performance_text_editor) | 2026-10-13 | Confirmed (C++ reported) |
| Ghostty | macOS, Linux; no Windows (maybe late 2026, no promise) | Zig core; Swift/AppKit + Metal on macOS, GTK4 on Linux; Sparkle for updates (bundle) | [README](https://github.com/ghostty-org/ghostty), [Windows discussion](https://github.com/ghostty-org/ghostty/discussions/12290) | 2026-10-13 | Confirmed |

### Leads from the spec

| Lead in `spec.md` | Verdict | Source |
|---|---|---|
| Codex app is Electron, on Windows since March 2026, Electron chosen to reach Windows and Linux | Reported, all three hold: Windows on 2026-03-04; reason from an OpenAI engineer on HN | [teardown](https://www.kitze.io/posts/codex-electron-app-technical-breakdown), [TechRepublic](https://www.techrepublic.com/article/news-openai-codex-windows-app-launch/), [HN](https://news.ycombinator.com/item?id=46859054) |
| Zed is Rust + GPUI; 1.0 on macOS, Linux, Windows in April 2026 | Confirmed: 1.0.0 dated 2026-04-29 on zed.dev; Windows since 2025-10 | [1.0.0 notes](https://zed.dev/releases/stable/1.0.0), [Windows post](https://zed.dev/blog/zed-for-windows-is-here) |
| cmux is Swift/AppKit + libghostty, macOS only | Confirmed | [repo](https://github.com/manaflow-ai/cmux) |
| Conductor is macOS only; stack unknown | Platforms reported; stack now found: TypeScript + Tauri + React (its own job post, so reported) | [job post](https://www.ycombinator.com/companies/conductor/jobs/MYjJzBV-product-engineer) |
| Electrobun: Windows still on the roadmap | Wrong now: Windows x64 is a listed target; Windows arm64 is beta; macOS is arm64 only | [Electrobun targets](https://framework.blackboard.sh/electrobun/guides/compatability) |
| Tauri 2 runs a Node.js server as a sidecar; no source for `node-pty` in it | Confirmed for the sidecar guide. Still no source for node-pty in a packaged sidecar: a risk and a spike item | [Tauri Node sidecar](https://v2.tauri.app/learn/sidecar-nodejs/), [Node SEA](https://nodejs.org/docs/latest/api/single-executable-applications.html) |

### Patterns

- Electron + node-pty + xterm.js is the default for agent workspaces. Every agent workspace that ships on Windows today uses Electron or Rust.
- Fully native apps (cmux, Nova, Ghostty's macOS app) are macOS only, or ship a second UI per OS. Ghostty still has no Windows app and has not chosen a Windows UI framework.
- The only native apps on all three OSes (Zed, Warp, Lapce) are Rust with their own GPU UI. Each is built by a funded team. Zed needed separate shaders per GPU API for Windows ([The Register](https://www.theregister.com/2025/08/22/everything_is_different_on_windows/), reported).
- Tauri is used by Conductor, Vibe Kanban and Jean. Vibe Kanban and Jean have Rust backends; Conductor ships macOS only.

## Cost for Natamux

### Reasons in plan-tech-stack section 6

| Reason given for Electron | Still holds? |
|---|---|
| The daemon is Node with node-pty; Electron runs it with no sidecar | Yes. No source shows node-pty working in a packaged Tauri sidecar; Node SEA loads `.node` files only by copying them to a temp file and calling `process.dlopen()` |
| Tauri needs a Rust toolchain and a per-OS sidecar | Yes. The Tauri guide still names sidecar binaries by target triple under `src-tauri/binaries` |
| Swift is macOS only and needs a second codebase for Windows | Yes. cmux and Ghostty are still macOS-first; Ghostty has no Windows UI yet |
| Comparable open source ADEs use Electron + TypeScript | Yes. Superset, Emdash, Nimbalyst and Sculptor (confirmed above) |

### Terminal, per option on the shortlist

The shortlist is Electron (recommended) and Tauri 2 (spike). Native per OS is shown for contrast.

| Need | Electron (today) | Tauri 2 | Native per OS (B) |
|---|---|---|---|
| PTY on macOS and Linux | node-pty in the Electron utility process (`packages/daemon/src/pty.ts`) | node-pty in a packaged Node sidecar. node-pty also needs `spawn-helper` next to it. No confirmed working example | Daemon unchanged (node-pty) |
| ConPTY on Windows | node-pty's ConPTY files | Same files must ship in the sidecar. Not proven | Daemon unchanged; UI has no PTY |
| Rendering | xterm.js WebGL in Chromium (`packages/web/src/lib/terminal-session.ts`), same as the browser tab | macOS WKWebView: WebKit bug 264268 (artifacts with WebGL and transparency). Windows WebView2 is Chromium, so it matches today | macOS: SwiftTerm or libghostty. Windows: libghostty has no Windows renderer; only a WIP fork with WinUI 3 and immature ConPTY controls |
| Vietnamese IME | Chromium IME. Natamux already tunes for input methods that send DEL then a letter (`INPUT_GAP_MS`, `.claude/skills/terminal/SKILL.md`) | WebKit bug 239360 (IME popup in the wrong place in WKWebView) is still open; xterm.js issue 4486 (Chinese IMEs). The `INPUT_GAP_MS` tuning must be re-tested on WebKit | Each widget handles IME itself. Zed fixed CJK IME in its terminal only in 2026-03 |
| Large scrollback | xterm.js scrollback; the daemon keeps history in `@xterm/headless` (`packages/daemon/package.json`) | Same code; WebKit speed not measured | Depends on the widget; unknown |
| Same engine as the browser tab | Yes (Chromium) | No on macOS (WebKit) | No |

Sources: [Tauri Node sidecar](https://v2.tauri.app/learn/sidecar-nodejs/), [Node SEA](https://nodejs.org/docs/latest/api/single-executable-applications.html), [WebKit 264268](https://bugs.webkit.org/show_bug.cgi?id=264268), [WebKit 239360](https://bugs.webkit.org/show_bug.cgi?id=239360), [xterm.js 4486](https://github.com/xtermjs/xterm.js/issues/4486), [Ghostty on Windows](https://github.com/ghostty-org/ghostty/discussions/12290), [wintty fork](https://github.com/deblasis/wintty), [ConPTY NuGet](https://packages.nuget.org/packages/Microsoft.Windows.Console.ConPTY/), [Zed IME fix](https://git.secluded.site/zed/commit/ddc327fe035beb690d52a55c0d7645f0def233b8.patch).

### Features of today's shell

Every feature of `packages/desktop` and where it would go:

| Feature | Today | Tauri 2 | Native per OS |
|---|---|---|---|
| Daemon start | `main.ts` `startDaemon` (utility process); `startup.ts` `freePort`, `waitForHealth` | Start the Node sidecar from Rust; port and health check ported | Start a bundled Node from Swift / C# |
| Window | `main.ts` `createWindow`; `window-options.ts` `windowOptions` | Tauri window config | NSWindow / WinUI window |
| Menu | `main.ts` `buildMenu`, `addProjectFromMenu` | [Tauri menu](https://v2.tauri.app/learn/window-menu/) | NSMenu / WinUI menu |
| Dialogs | `main.ts` `pickRepository`, `confirmStop`, `runningTasks` | [Tauri dialog plugin](https://v2.tauri.app/plugin/dialog/) (folder pick, confirm) | NSOpenPanel / WinUI pickers |
| Updater | `main.ts` `makeUpdater`; `updater.ts`; `update-check.ts` | [Tauri updater plugin](https://v2.tauri.app/plugin/updater/), with its own signing key; losing the key blocks all updates. `update-check.ts` logic can be ported | [Sparkle](https://sparkle-project.org/) on macOS, [WinSparkle](https://winsparkle.org/) on Windows |
| Login-shell PATH | `startup.ts` `loginShellPath`; `daemon-env.ts` `daemonEnv`, `fallbackBinDirs` | Ported to Rust | Ported to Swift / C# |
| `natamux` command in agent terminals | `cli-shim.ts` `writeCliShim` (runs the CLI with the app's own runtime) | No Node in the shell: ship Node in the sidecar and point the shim at it, or compile the CLI with Node SEA | Same as Tauri |
| Token hand-off | `daemon-access.ts` `loadOrCreateDaemonToken`; `preload.cts` | An init script or a Tauri command gives the token to the page | Native UI sends the token itself |
| Link and sandbox rules | `navigation.ts` `isDashboardUrl`, `opensInBrowser`; sandbox in `windowOptions` | Navigation handler plus the [opener plugin](https://v2.tauri.app/plugin/opener/); Tauri capabilities replace the sandbox | No web page; links open with the OS |
| Ghostty font | `ghostty-config.ts` `readGhosttyFont` | Ported to Rust, or moved into the daemon | Ported, or read by libghostty |

### Work per option

Sizes are estimates, not measured.

| Option | Kept | Rewritten | New toolchain in CI | Rough size (estimate) | Main risks |
|---|---|---|---|---|---|
| Stay on Electron | All | Nothing | None | 0 | 327 MB download |
| A. Tauri 2 | `packages/daemon`, `packages/web`, `packages/cli`; logic in `update-check.ts` | `packages/desktop` in Rust (all rows of the table above); Node sidecar packaging; updater | Rust, plus node-pty prebuilds per OS and arch | 2–4 weeks, then a long tail on WebKit | node-pty in a sidecar; xterm.js and IME in WKWebView; two browser engines to test |
| B. Native UI | `packages/daemon`, `packages/cli` | All of `packages/web`, once per OS (Swift, C#) or once in GPUI | Xcode + .NET, or Rust | Months; doubles UI work for every later feature | Browser tab and Codespaces need the old web UI, so two UIs to keep in step; no mature Windows terminal widget |
| C. Full rewrite | Task file format | Daemon and UI | Rust, or Swift + C# | Many months | All of B, plus re-proving git, hooks and agent adapters |

## Recommendation

**Stay on Electron.** Spend the effort on build steps 10 and 11 (Windows build, then signing) instead.

Why:

- The terminal is the core of Natamux. Electron gives the same Chromium engine in the desktop app and in the browser tab, and the Vietnamese input tuning was done on it. Every option that changes the engine reopens known terminal bugs (WebKit 264268, 239360).
- Natamux has no Windows build yet (`packages/desktop/README.md`). Electron is the shortest way there: `.github/workflows/desktop.yml` already packages on `windows-latest`, and `packages/desktop/electron-builder.yml` already has an `nsis` target.
- Most agent workspaces that ship on Windows use Electron, including the Codex app, which picked it to reach Windows. Fully native competitors are macOS only.
- Options B and C break the browser-tab goal or force two UIs on one developer.
- The main gain of Tauri is download size, and Natamux would give part of it back by shipping Node for the daemon.

Browser tab and Codespaces: **kept**. The daemon serves the dashboard and the API on one port, and the shell only shows that page (`packages/desktop/README.md`, `plan-tech-stack.md` section 6). Staying on Electron changes none of this.

If download size or memory becomes a real user complaint, run a time-boxed Tauri 2 spike as its own task. It passes only if:

- node-pty runs in the packaged Node sidecar on macOS arm64 and Windows x64, and the `natamux` command works in agent terminals.
- xterm.js with WebGL in WKWebView shows no artifacts, Vietnamese Telex input works with `INPUT_GAP_MS`, and 10,000 lines of scrollback stay smooth.
- The measured download size and idle memory beat Electron 0.1.13 by enough to matter.

What the recommendation keeps, changes and replaces:

| Package | Stay on Electron | If the Tauri spike passes later |
|---|---|---|
| `packages/desktop` | Kept. Add Windows parts only where tests show a gap (`plan-tech-stack.md`, "Native code per OS") | Replaced by a Rust shell; `update-check.ts` logic ported |
| `packages/daemon` | Kept | Kept; packaged as a sidecar binary |
| `packages/web` | Kept | Kept; terminal and IME re-tested on WebKit |
| `packages/cli` | Kept | Kept; runs on the sidecar's Node |

Watch list, re-check in 2027: a tagged libghostty release with a Windows renderer; GPUI 1.0; Electrobun stable with macOS x64; Deno Desktop no longer experimental.

## Migration path

For the recommended path. It follows `plan-tech-stack.md` section 10, where steps 10 and 11 are not done.

1. Step 10, Windows build: add a Windows job to `.github/workflows/release.yml` (its matrix has only `mac-arm64`, plus paused `mac-x64` and `linux-x64`), with a packaged-app smoke test like the macOS one. Test agent CLIs under PowerShell, cmd and WSL.
2. Step 10, other targets: turn the Intel Mac and Linux jobs back on when runners allow.
3. Step 11, Windows self-update: `installKind` returns `manual` on Windows today (`docs/design-desktop-update.md`); add an NSIS kind.
4. Step 11, signing and notarization: waits for the Apple Developer account and a Windows certificate.
5. Only after that, and only if users ask: the Tauri spike above, as a follow-up task. Not done here.

## Sources

All checked on 2026-10-13.

Frameworks and tools:

- [Tauri releases](https://github.com/tauri-apps/tauri/releases)
- [Tauri Node sidecar](https://v2.tauri.app/learn/sidecar-nodejs/)
- [Tauri updater](https://v2.tauri.app/plugin/updater/)
- [Tauri webviews](https://v2.tauri.app/reference/webview-versions/)
- [Tauri menu](https://v2.tauri.app/learn/window-menu/)
- [Tauri dialog plugin](https://v2.tauri.app/plugin/dialog/)
- [Tauri opener plugin](https://v2.tauri.app/plugin/opener/)
- [Electron](https://github.com/electron/electron)
- [Electrobun releases](https://github.com/blackboardsh/electrobun/releases)
- [Electrobun targets](https://framework.blackboard.sh/electrobun/guides/compatability)
- [Deno](https://github.com/denoland/deno)
- [Deno 2.9](https://deno.com/blog/v2.9)
- [Deno 2.3 addons](https://deno.com/blog/v2.3)
- [Wails releases](https://github.com/wailsapp/wails/releases)
- [Node SEA](https://nodejs.org/docs/latest/api/single-executable-applications.html)
- [Swift](https://github.com/swiftlang/swift)
- [Windows App SDK](https://github.com/microsoft/WindowsAppSDK)
- [GPUI Cargo.toml](https://github.com/zed-industries/zed/blob/main/crates/gpui/Cargo.toml)
- [Flutter](https://github.com/flutter/flutter)
- [Flutter supported platforms](https://docs.flutter.dev/reference/supported-platforms)
- [Compose Multiplatform](https://github.com/JetBrains/compose-multiplatform)
- [Avalonia](https://github.com/AvaloniaUI/Avalonia)
- [Qt licensing](https://www.qt.io/licensing/)
- [Sparkle](https://sparkle-project.org/)
- [WinSparkle](https://winsparkle.org/)
- [Better Stack benchmark](https://betterstack.com/community/guides/scaling-nodejs/tauri-vs-electron-vs-deno-vs-electrobun/)

Terminals:

- [Ghostty](https://github.com/ghostty-org/ghostty)
- [Ghostty on Windows](https://github.com/ghostty-org/ghostty/discussions/12290)
- [SwiftTerm](https://github.com/migueldeicaza/SwiftTerm)
- [portable-pty](https://docs.rs/portable-pty)
- [gpui-terminal](https://docs.rs/gpui-terminal)
- [xterm.dart](https://pub.dev/documentation/xterm/2.6.0/index.html)
- [WebKit 264268](https://bugs.webkit.org/show_bug.cgi?id=264268)
- [WebKit 239360](https://bugs.webkit.org/show_bug.cgi?id=239360)
- [xterm.js 4486](https://github.com/xtermjs/xterm.js/issues/4486)
- [wintty fork](https://github.com/deblasis/wintty)
- [ConPTY NuGet](https://packages.nuget.org/packages/Microsoft.Windows.Console.ConPTY/)
- [Zed IME fix](https://git.secluded.site/zed/commit/ddc327fe035beb690d52a55c0d7645f0def233b8.patch)

Agent workspaces:

- [Superset package.json](https://github.com/superset-sh/superset/blob/main/apps/desktop/package.json)
- [Emdash package.json](https://github.com/generalaction/emdash/blob/main/package.json)
- [Crystal package.json](https://github.com/stravu/crystal/blob/main/package.json)
- [Nimbalyst](https://github.com/nimbalyst/nimbalyst)
- [Sculptor package.json](https://github.com/imbue-ai/sculptor/blob/main/sculptor/frontend/package.json)
- [Codex app teardown](https://www.kitze.io/posts/codex-electron-app-technical-breakdown)
- [Codex on HN](https://news.ycombinator.com/item?id=46859054)
- [Codex on Windows](https://www.techrepublic.com/article/news-openai-codex-windows-app-launch/)
- [Vibe Kanban](https://github.com/BloopAI/vibe-kanban)
- [Vibe Kanban shutdown](https://www.vibekanban.com/blog/shutdown)
- [Jean](https://github.com/coollabsio/jean)
- [cmux](https://github.com/manaflow-ai/cmux)
- [Conductor job post](https://www.ycombinator.com/companies/conductor/jobs/MYjJzBV-product-engineer)
- [Conductor on the App Store](https://apps.apple.com/us/app/conductor-build/id6791228564)
- [Conductor review](https://thenewstack.io/a-hands-on-review-of-conductor-an-ai-parallel-runner-app/)
- [Warp](https://github.com/warpdotdev/warp)
- [Warp on Windows](https://www.warp.dev/blog/launching-warp-on-windows)

Editors:

- [Zed](https://github.com/zed-industries/zed)
- [Zed on Windows](https://zed.dev/blog/zed-for-windows-is-here)
- [Zed 1.0.0](https://zed.dev/releases/stable/1.0.0)
- [The Register on Zed for Windows](https://www.theregister.com/2025/08/22/everything_is_different_on_windows/)
- [VS Code](https://github.com/microsoft/vscode)
- [Cursor, Windsurf, Kiro comparison](https://computingforgeeks.com/cursor-vs-windsurf-vs-kiro/)
- [Kiro on Linux](https://omgubuntu.co.uk/2025/07/kiro-ai-ide-public-preview-linux)
- [Antigravity](https://visualstudiomagazine.com/articles/2025/11/20/google-joins-ai-ide-race-to-compete-with-vs-code-apparently-forking-vs-code.aspx)
- [JetBrains Air launch](https://blog.jetbrains.com/air/2026/03/air-launches-as-public-preview-a-new-wave-of-dev-tooling-built-on-26-years-of-experience/)
- [Fleet discontinued](https://devclass.com/2025/12/09/jetbrains-abandons-fleet-ide-pins-hopes-on-forthcoming-air-agentic-development-tool/)
- [Lapce](https://github.com/lapce/lapce)
- [Nova](https://toolsandtoys.net/nova-code-editor-for-mac-by-panic/)
- [Sublime HQ blog, 2021-04](https://www.sublimetext.com/blog/articles/2021/04)
- [Sublime Text GPU rendering](https://sublimetext.com/docs/gpu_rendering.html)
- [Will Bond on Sublime Text](https://wbond.net/thoughts/building_a_high_performance_text_editor)
