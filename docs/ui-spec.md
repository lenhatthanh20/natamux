# Natamux UI spec v1

Status: draft for the owner. Date: 2026-10-09.
Inputs: `orca.md`, `vscode-guidelines.md`, `vscode-live.md`, `audit-navigation.md` (NAV-*), `audit-workspace.md` (WS-*), `audit-responsive.md` (RA-*), and the current code in `packages/web/src`.
Rules from AGENTS.md that bind this spec: no code or text copied from Orca (ideas only); keep dependencies few (no new package without the owner's OK); `pnpm` only; build order in `docs/plan-tech-stack.md` section 10 (this is all step 4/6/7 polish, no skipping ahead to the Electron shell; the title bar is made "Electron-ready" but runs in the browser now).

Notation: `Mod` = Cmd on macOS, Ctrl on Windows. Sizes are CSS px at 100% zoom. "Main width" = width of the `<main>` element, not the window.

---------------------------------------------------------------------

## 1. Principles

1. **Who needs me, in 3 seconds.** From any screen, at any width, the user sees how many tasks wait for them and which ones, without scrolling or opening anything. Needs-you count lives in the title bar, the status bar, the document title and system notifications; the sidebar tree shows each waiting workspace as a card. A waiting agent's question is shown as text, not hidden in an activity log.
2. **Color means state.** Chrome is neutral gray. The 9 task states own the only strong hues. The blue accent is reserved for focus, keyboard selection, links and progress. Primary buttons are neutral (inverted ink), not blue. If something is colored, it is telling you something.
3. **Keyboard first, mouse friendly.** Every action is a command in the palette (Mod+K). Frequent actions have one key. Every shortcut is visible where the action lives (tooltip, menu row, palette row). A terminal never loses a key it needs.
4. **Calm density.** 13px body, 28px controls, 44px two-line rows. Quiet hover (a 5% wash), no movement, no reordering under the pointer, no layout jump when a status line appears. Live data updates in place.
5. **The work keeps the space.** The terminal and the diff are the product. Side panels yield first: they overlay, collapse or become pages before the work area drops below its minimum (terminal 480px, diff 560px).
6. **Say what happens, in plain words.** Buttons are verbs. Disabled controls say why. Errors say how to fix. Destructive actions confirm and say what is kept. No jargon ("PORT is the first", "Fan out").
7. **One system.** One token set for light and dark, one component per job, one focus ring, one time format. Monaco and xterm follow the app theme, not the OS.
8. **Works at every size.** From 390px phones to 2560px monitors. Nothing is ever clipped without a visible hint (fade, "+N", scrollbar).

---------------------------------------------------------------------

## 2. Design tokens

All tokens live in `packages/web/src/styles.css`. Raw values on `:root` (light) and `:root[data-theme="dark"]`. Tailwind names come from `@theme inline` (`--color-<name>: var(--<name>)`), so `bg-surface`, `text-ink-2`, `border-line` etc. work. Existing names are kept where they exist, to limit churn.

### 2.1 Fonts

| Token | Value | Use |
|---|---|---|
| `--font-sans` | `"Inter Variable", ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif` | all UI text. Inter at 13px is the UI font of JetBrains IDEs |
| `--font-mono` | `"Geist Mono Variable", ui-monospace, "SF Mono", "Cascadia Mono", Menlo, Consolas, monospace` | ids, branches, paths, hashes, ports, +/- counts, kbd |
| terminal (`TERMINAL_FONT` in `lib/fonts.ts`) | `"JetBrains Mono Variable", "Symbols Nerd Font Mono"`, then the `--font-mono` stack | terminals and code views (Monaco diff and file). The icon font is bundled (`src/assets/fonts`) and loads on the first icon |
| body features | `font-feature-settings: "ss01", "cv11"; font-variant-numeric: tabular-nums; -webkit-font-smoothing: antialiased` | already set, keep |

- Both fonts are already installed (`@fontsource-variable/geist`, `geist-mono`). No new font package.
- Mono is for machine text only. Times, counts in prose and labels stay sans.
- Weights: 400, 500, 600 only. No 700.

### 2.2 Type scale

Defined in `@theme` so Tailwind classes map 1:1. Each size has exactly one line height (fixes RA-19).

| Role | Class | Size / line height | Weight | Tracking | Where |
|---|---|---|---|---|---|
| display | `text-xl` | 20 / 28 | 600 | -0.01em | sign-in title, empty-state hero title |
| title-lg | `text-lg` | 16 / 24 | 600 | -0.01em | workspace title, details title |
| title | `text-md` (new) | 15 / 22 | 600 | -0.005em | view titles ("All tasks"), dialog title, palette input text |
| body | `text-sm` (= body default) | 13 / 20 | 400 | 0 | default text |
| body-strong | `text-sm font-medium` | 13 / 20 | 500 | 0 | row titles, buttons, tabs, menu items |
| label | `text-xs font-medium` | 12 / 16 | 500 | 0 | field labels, table headers, chip text |
| meta | `text-xs text-muted` | 12 / 16 | 400 | 0 | second lines, times, helper text |
| caps | `text-2xs font-semibold uppercase tracking-[0.06em]` | 11 / 14 | 600 | +0.06em | section headers (sidebar, details, palette groups) |
| micro | `text-2xs font-medium` | 11 / 14 | 500 | 0 | count badges, kbd |
| mono-meta | `font-mono text-xs` | 12 / 16 | 400 | 0 | ids, branches, paths |
| code | Monaco | 12 / 18 | 400 | 0 | diff |
| terminal | xterm | 13, lineHeight 1.3 | 400 | 0 | terminal |

`@theme` values: `--text-2xs: 0.6875rem; --text-2xs--line-height: 0.875rem; --text-xs: 0.75rem; --text-xs--line-height: 1rem; --text-sm: 0.8125rem; --text-sm--line-height: 1.25rem; --text-md: 0.9375rem; --text-md--line-height: 1.375rem; --text-lg: 1rem; --text-lg--line-height: 1.5rem; --text-xl: 1.25rem; --text-xl--line-height: 1.75rem`. `body { font-size: var(--text-sm); line-height: var(--text-sm--line-height) }`.

Rules: nothing below 11px. 11px only for caps labels, badges and kbd. Hierarchy comes from weight and color, not more sizes.

**UI size setting** (answers RA-18 without making every screen less dense): `html { font-size: calc(16px * var(--ui-zoom)) }`, `--ui-zoom` = 0.92 (Compact, 12px body), 1 (Default, 13px), 1.077 (Large, 14px). All sizes are rem, so the whole UI scales. xterm and Monaco font sizes read `13 * zoom` and `12 * zoom`. Stored in localStorage `natamux.ui-size`.

### 2.3 Color tokens

Contrast = WCAG 2.x ratio, computed with the script in the study folder (`tools/contrast.py`). Text needs >= 4.5:1, UI boundaries and focus >= 3:1. "on X" = the surface that text sits on.

#### Surfaces

| CSS variable | Tailwind | Dark | Light | Use |
|---|---|---|---|---|
| `--shell` | `bg-shell` | `#0c0d0f` | `#f3f4f6` | title bar, status bar, window canvas behind parts |
| `--sidebar` | `bg-sidebar` | `#0c0d0f` | `#f3f4f6` | sidebar (same as shell: chrome reads as one frame) |
| `--canvas` | `bg-canvas` | `#111214` | `#fafafb` | main area background, diff background |
| `--surface` | `bg-surface` | `#16171a` | `#ffffff` | cards, table, details panel, pane headers |
| `--raised` | `bg-raised` | `#1c1d21` | `#f6f7f9` | secondary buttons, segmented track, kbd |
| `--popover` | `bg-popover` | `#1c1d21` | `#ffffff` | menus, tooltips (dark), toasts, dialogs, palette |
| `--input` | `bg-input` | `#0f1012` | `#ffffff` | text fields, selects, textareas (sunken in dark) |
| `--terminal` | `bg-terminal` | `#0f1219` | `#f7f9fd` | terminal body (sunken) |
| `--scrim` | `bg-scrim` | `rgb(0 0 0 / 0.5)` | `rgb(15 17 20 / 0.32)` | behind dialogs, drawers on small screens |
| `--skeleton` | `bg-skeleton` | `rgb(255 255 255 / 0.06)` | `rgb(20 24 31 / 0.06)` | loading bars |
| `--badge` | `bg-badge` | `#2a2c31` | `#e6e8ec` | count badges |

#### Text

| CSS variable | Tailwind | Dark | Contrast (dark) | Light | Contrast (light) |
|---|---|---|---|---|---|
| `--ink` | `text-ink` | `#e6e7ea` | shell 15.7, canvas 15.2, surface 14.5, raised 13.6 | `#17191d` | shell 16.0, canvas 16.9, surface 17.6 |
| `--ink-2` (new) | `text-ink-2` | `#b4b7be` | shell 9.7, surface 8.9, raised 8.4, badge 7.0 | `#3f444c` | shell 8.9, surface 9.8, badge 8.0 |
| `--muted` | `text-muted` | `#8d919a` | shell 6.2, canvas 5.9, surface 5.7, raised 5.3, hover-on-raised 4.7, selected row 5.2 | `#5f6570` | shell 5.3, canvas 5.6, surface 5.9, raised 5.5, accent selection 5.0 |
| `--placeholder` (new) | `placeholder:text-placeholder` | `#7d818a` | input 4.9, surface 4.6 | `#6b717c` | white 4.9 |
| `--faint` (new) | `text-faint` | `#5c6068` | shell 3.1 (icons and lines only, never text) | `#a3a8b0` | 2.4 (decoration only) |
| `--link` (new) | `text-link` | `#8fb0ff` | shell 9.1, surface 8.4 | `#2448b8` | shell 7.1, surface 7.8 |

Disabled text uses `opacity: 0.5` on the control (exempt from contrast by WCAG, but the reason is shown, see 4.1).

#### Lines and controls

| CSS variable | Tailwind | Dark | Light | Contrast | Use |
|---|---|---|---|---|---|
| `--line` | `border-line` | `#26282d` | `#e3e5e9` | dark 1.3 on shell (was 1.23, NAV-30) | dividers inside a surface |
| `--line-strong` | `border-line-strong` | `#363940` | `#cfd3d9` | dark 1.7, light 1.4 | part borders, popover borders, pane borders |
| `--control` (new) | `border-control` | `#3d4048` | `#c2c7cf` | dark 1.8 on surface | input/select/secondary button border |
| `--control-hover` (new) | `border-control-hover` | `#4a4e57` | `#aab0b9` | 2.2 | hovered control border |
| `--contrast-border` (new) | `border-contrast` | `transparent` | `transparent` | becomes `#8d919a` / `#5f6570` (6.0 / 5.9) under `@media (prefers-contrast: more)` | extra outline on controls, rows, panes, popovers |

Control borders alone are below 3:1 (same as VS Code and Orca). Controls are identified by fill + label + placeholder. Users who need more get `prefers-contrast: more`, which turns `--contrast-border` on and swaps `--control` to the contrast value.

#### Interaction states

| CSS variable | Tailwind | Dark | Light | Use |
|---|---|---|---|---|
| `--hover` | `bg-hover` | `rgb(255 255 255 / 0.05)` | `rgb(20 24 31 / 0.04)` | pointer over a row/ghost control |
| `--active` (new) | `bg-active` | `rgb(255 255 255 / 0.08)` | `rgb(20 24 31 / 0.07)` | pressed |
| `--selected` | `bg-selected` | `rgb(255 255 255 / 0.08)` | `rgb(20 24 31 / 0.07)` | selected row when its list is NOT focused; current workspace |
| `--selection-active` (new) | `bg-selection-active` | `color-mix(in srgb, var(--accent) 18%, transparent)` | `color-mix(in srgb, var(--accent) 12%, transparent)` | selected row when its list HAS focus (ink 11.9 / 13.6; muted 4.7 / 5.0) |
| `--focus` (new) | `outline-focus` | `#7aa2ff` | `#2f5fe0` | focus ring: 7.8 on shell, 7.2 on surface / 5.0 on shell, 5.5 on white; 5.9 on selection-active |
| `::selection` | — | `color-mix(in srgb, var(--accent) 35%, transparent)` | same | text selection (keep) |

#### Accent, danger, signal

| CSS variable | Dark | Light | Contrast |
|---|---|---|---|
| `--accent` | `#7aa2ff` | `#2f5fe0` | as text: dark 7.5 on canvas; light 5.3 on canvas |
| `--accent-hover` (new) | `#93b4ff` | `#2650c4` | — |
| `--accent-fg` (new) | `#0b1020` | `#ffffff` | text on accent fill: 7.6 / 5.5; on hover 9.2 / 6.9 |
| `--danger` (button fill) | `#c93a2b` | `#b42318` | white text 5.1 / 6.6 |
| `--danger-text` (new) | `#ff8577` | `#b42318` | dark 7.6 on surface; light 6.6 on white |
| `--signal` (needs you) | `#f5b54a` | `#8a5300` | dark 10.7 on shell, 9.7 on signal-soft; light 5.8 on shell, 5.3 on signal-soft (was 3.0 / 3.2, RA-12) |
| `--signal-soft` | `rgb(245 181 74 / 0.07)` | `rgb(217 144 15 / 0.10)` | background of needs-you areas |
| `--signal-line` (new) | `rgb(245 181 74 / 0.28)` | `rgb(217 144 15 / 0.35)` | 1px ring/top bar of needs-you areas |

Primary button = `bg-ink text-canvas` (dark 15.2, light 17.6). Hover = ink at 85% (dark 11.1).

#### Task state colors

One `data-state` attribute drives chip, dot, glyph, board column and filter. `--chip-fg` is text and glyph color; `--chip-bg` is the chip fill. All pairs >= 4.5 (fixes RA-10, RA-11).

| State | Dark fg / bg | fg on chip / on shell | Light fg / bg | fg on chip / on shell |
|---|---|---|---|---|
| draft | `#9a9ea6` / `#1d1f23` | 6.1 / 7.2 | `#5f6570` / `#eceef1` | 5.0 / 5.3 |
| ready | `#6cc4d6` / `#13262b` | 7.8 / 9.7 | `#0b6274` / `#e0f1f5` | 6.0 / 6.3 |
| running | `#8fb0ff` / `#172140` | 7.4 / 9.1 | `#2448b8` / `#e4ebff` | 6.5 / 7.1 |
| needs-input | `#f5b54a` / `#33260f` | 8.1 / 10.7 | `#8a5300` / `#fdefd0` | 5.6 / 5.8 |
| review | `#b9a4ff` / `#241d3d` | 7.5 / 9.1 | `#5634b8` / `#ede7fc` | 6.8 / 7.4 |
| blocked | `#f08fb3` / `#331a26` | 7.1 / 8.6 | `#a33b62` / `#f9e4ec` | 5.2 / 5.7 |
| failed | `#ff8577` / `#3a1a17` | 6.6 / 8.2 | `#b0321f` / `#fbe3de` | 5.2 / 5.7 |
| done | `#6fd19a` / `#142a1e` | 8.2 / 10.4 | `#1c7442` / `#e1f3e8` | 5.0 / 5.3 |
| cancelled | `#8d919a` / `#1d1f23` | 5.2 / 6.2 (was 2.1-3.3) | `#5f6570` / `#eceef1` | 5.0 / 5.3 (was 2.0-3.3) |

Running (blue) and needs-input (amber) are far apart in hue, so "working" never reads as "needs you".

#### Project colors

Each project gets one of six colors from a hash of its id (`features/sidebar/project-color.ts`), so the color stays the same with nothing stored. No amber or red, so a project color never reads as needs-you or failed. Contrast is the worst case: as the count text on its 10% tint, over the band at 20% (hover), over the card.

| CSS variable | Dark | Light | Contrast (dark / light) |
|---|---|---|---|
| `--project-0` teal | `#4fd1c0` | `#0a5a54` | 5.4 / 4.9 |
| `--project-1` violet | `#b39bff` | `#5b2fb0` | 4.7 / 5.0 |
| `--project-2` fuchsia | `#f08ad0` | `#8e1f6c` | 4.8 / 4.8 |
| `--project-3` green | `#9ad65a` | `#2f5f0b` | 5.6 / 4.7 |
| `--project-4` sky | `#5cc0f0` | `#065580` | 5.1 / 4.8 |
| `--project-5` indigo | `#9fa0ff` | `#3f3bb4` | 4.7 / 5.0 |

#### Diff and git

| CSS variable | Dark | Light | Contrast |
|---|---|---|---|
| `--add` (text) | `#6fd19a` | `#1a7a42` | dark 10.1 on canvas, 8.2 on add-bg; light 5.2 on canvas, 4.7 on add-bg (was 3.9, RA-14) |
| `--remove` (text) | `#ff8577` | `#b42318` | dark 7.9 / 6.6 on remove-bg; light 6.3 / 5.7 |
| `--add-bg` (new) | `rgb(46 160 67 / 0.16)` | `rgb(46 160 67 / 0.12)` | ink on it: 12.4 / 15.5 |
| `--remove-bg` (new) | `rgb(248 81 73 / 0.16)` | `rgb(248 81 73 / 0.12)` | ink on it: 12.7 / 15.2 |
| `--add-word` (new) | `rgb(46 160 67 / 0.32)` | `rgb(46 160 67 / 0.28)` | changed words inside a line |
| `--remove-word` (new) | `rgb(248 81 73 / 0.32)` | `rgb(248 81 73 / 0.28)` | |
| `--modified` (new) | `#e8c15a` | `#8a5a00` | file status letter M (11.4 / 5.7) |
| `--renamed` (new) | = `--accent` | = `--accent` | file status letter R |

#### Tooltip, scrollbar

| CSS variable | Dark | Light |
|---|---|---|
| `--tooltip-bg` | `#2a2c31` + 1px `--line-strong` | `#24272d` (inverted) |
| `--tooltip-fg` | `#e6e7ea` (11.3) | `#ffffff` (15.0) |
| `--scroll` | `rgb(255 255 255 / 0.14)` | `rgb(20 24 31 / 0.20)` |
| `--scroll-hover` | `rgb(255 255 255 / 0.26)` | `rgb(20 24 31 / 0.36)` |

#### Terminal ANSI palettes (xterm `ITheme`)

The "Night" palette, picked from four designs in `docs/design-terminal-look.md` (2026-10-12). Ratios are against the background. Set `minimumContrastRatio: 4.5` too, so any app color is pushed to readable (fixes WS-11). Dark black is a background color in practice; its 1.7 is fine because the ratio option lifts it when used as text.

| | Dark (on `#0f1219`) | Light (on `#f7f9fd`) |
|---|---|---|
| background / foreground | `#0f1219` / `#d6dcf0` (13.7) | `#f7f9fd` / `#1b2238` (15.0) |
| cursor / cursorAccent | `#7aa2ff` / `#0f1219` | `#2f5fe0` / `#f7f9fd` |
| selectionBackground / selectionInactiveBackground | `#7aa2ff47` / `#7aa2ff21` | `#2f5fe030` / `#2f5fe018` |
| black / brightBlack | `#343b52` (1.7) / `#7c86a8` (5.2) | `#262d45` (12.9) / `#5d6684` (5.4) |
| red / brightRed | `#f7768e` (7.1) / `#ff9bae` (9.4) | `#c4304f` (5.1) / `#b02a46` (6.1) |
| green / brightGreen | `#8fd694` (10.9) / `#aee6b2` (13.2) | `#2c7a3a` (5.0) / `#256c33` (6.1) |
| yellow / brightYellow | `#e6c07b` (10.9) / `#f0d39d` (12.9) | `#8c5f00` (5.3) / `#7b5300` (6.5) |
| blue / brightBlue | `#7aa2ff` (7.5) / `#a6c1ff` (10.4) | `#2f5fe0` (5.2) / `#2448b8` (7.4) |
| magenta / brightMagenta | `#bb9af7` (8.1) / `#d2bbfa` (10.9) | `#7c4dd6` (5.1) / `#6a3ec2` (6.5) |
| cyan / brightCyan | `#6fd0e0` (10.5) / `#9ce0eb` (12.7) | `#0b6a80` (5.9) / `#095c6f` (7.2) |
| white / brightWhite | `#b8c0dc` (10.4) / `#e8ecf8` (15.9) | `#5d6684` (5.4) / `#1b2238` (15.0) |

The terminal follows the app theme (RA-17). A setting "Terminal: always dark" is P2.

#### Monaco themes (`features/files/monaco-setup.ts`)

Dark `natamux-dark` (base `vs-dark`): `editor.background #111214`, `editorGutter.background #111214`, `editorLineNumber.foreground #6b6f78`, `editorLineNumber.activeForeground #b4b7be`, `editor.lineHighlightBackground #00000000`, `diffEditor.insertedLineBackground #2ea04329`, `diffEditor.removedLineBackground #f8514929`, `diffEditor.insertedTextBackground #2ea04352`, `diffEditor.removedTextBackground #f8514952`, `diffEditorGutter.insertedLineBackground #2ea0434d`, `diffEditorGutter.removedLineBackground #f851494d`, `diffEditor.diagonalFill #ffffff0d`, `diffEditor.unchangedRegionBackground #16171a`, `scrollbarSlider.background #ffffff24`, `scrollbarSlider.hoverBackground #ffffff42`, `editorWidget.background #1c1d21`, `editorWidget.border #363940`, `focusBorder #7aa2ff`.
Light `natamux-light` (base `vs`): `editor.background #ffffff`, gutter `#ffffff`, line numbers `#8a9099` / active `#3f444c`, inserted line `#2ea0431f`, removed line `#f851491f`, inserted text `#2ea04347`, removed text `#f8514947`, gutters `#2ea04333` / `#f8514933`, unchanged region `#f6f7f9`, sliders `#14181f33` / `#14181f5c`, focus `#2f5fe0`.
The current dark theme background `#101e2d` (blue) is replaced.

### 2.4 Spacing

Tailwind's 4px base (`--spacing: 0.25rem`) stays. Allowed steps: 2, 4, 6, 8, 10, 12, 16, 20, 24, 32, 40, 48. No 6.4 (`0.4rem`, RA-22: change to `gap-1.5`).

| Where | Value |
|---|---|
| icon to text, dense rows | 6 (`gap-1.5`) |
| icon to text, buttons | 6 |
| between controls in a toolbar | 4 (icon buttons) / 8 (text buttons) |
| row inner padding x | 8 |
| control padding x | 10 (28px) / 12 (32px) |
| panel padding | 16 |
| dialog padding | 20 |
| between sections | 16 (panel) / 24 (page) |
| sidebar outer gutter | 8 |

### 2.5 Radii

Tailwind defaults already match: `rounded-sm` 4, `rounded-md` 6, `rounded-lg` 8, `rounded-xl` 12, `rounded-full`.

| Radius | Use |
|---|---|
| 4 | kbd, tooltip, label chip, menu item, scrollbar thumb, segmented inner button |
| 6 | buttons, inputs, selects, list rows, tabs pill, status bar items |
| 8 | cards, panes, menus, popovers, toasts, table frame |
| 12 | dialogs, command palette, drawers/sheets (inner corner only) |
| full | state chips, count badges, dots |

(RA-20: remove other values.)

### 2.6 Elevation

Three levels only. Hierarchy between parts comes from surface steps, not shadows.

| Level | CSS variable | Dark | Light | Use |
|---|---|---|---|---|
| 0 | — | none (borders) | none | everything in flow |
| 1 | `--shadow-1` | `0 8px 24px rgb(0 0 0 / 0.36), inset 0 1px 0 rgb(255 255 255 / 0.04)` | `0 8px 24px rgb(15 17 20 / 0.12), 0 0 0 1px rgb(15 17 20 / 0.04)` | menu, tooltip, toast, select popup, combobox list |
| 2 | `--shadow-2` | `0 24px 64px rgb(0 0 0 / 0.5), inset 0 1px 0 rgb(255 255 255 / 0.05)` | `0 24px 64px rgb(15 17 20 / 0.18)` | dialog, palette, drawer, details sheet |

z-index tokens: `--z-sticky 10`, `--z-sheet 30` (details overlay, sidebar drawer), `--z-toast 40`, `--z-dialog 70`, `--z-popover 75` (menu, above a dialog that opens it), `--z-palette 80`, `--z-tooltip 90`.

### 2.7 Icons

- Library: `lucide-react` only. No second set. No emoji in UI labels.
- Stroke in screen pixels, the same at every size (CSS `vector-effect: non-scaling-stroke` on `.lucide *`, as lucide's `absoluteStrokeWidth`): 1.25px at 12, 14 and 16; 1.5px at 20 and 32. A `strokeWidth` prop does not win over it.
- Action icons are outlined and neutral, never in a state color. State glyphs are filled shapes of their own (4.10), not lucide.
- Sizes (export as constants in `ui/styles.ts`: `ICON = { xs: 12, sm: 14, md: 16, lg: 20, xl: 32 }`):

| Size | Use |
|---|---|
| 12 | chevrons in dense rows, icons inside meta text |
| 14 | default: rows, buttons, menu items, status bar, tabs |
| 16 | icon buttons in title bar and toolbars, dialog close, palette search |
| 20 | sign-in, empty state (small) |
| 32 | empty-state hero |

- Color: `currentColor`. Idle icon-button icon `--muted`, hover `--ink`. Menu item icon `--ink-2`.
- Action icon map (one icon per meaning everywhere): new `Plus`, add repo `FolderPlus`, filter `ListFilter`, search `Search`, toggle sidebar `PanelLeft`, toggle details `PanelRight`, all tasks `ListTodo`, agents `LayoutGrid`, terminal `SquareTerminal`, changes `FileDiff`, history `History`, push `ArrowUpFromLine`, pull request `GitPullRequest`, branch `GitBranch`, copy `Copy`, more `Ellipsis`, settings `Settings`, close `X`, maximize `Maximize2` / restore `Minimize2`, new shell `SquareSplitHorizontal`, refresh `RefreshCw`, theme `SunMoon`, keyboard `Keyboard`, notifications `Bell` / `BellOff`, account `CircleUser`, pin `Pin` / `PinOff`, blur names `EyeOff` / show names `Eye`, rename `Pencil`, add subtask `ListPlus`, remove workspace and remove project `FolderMinus`, external link `ExternalLink`, wrap `WrapText`.

### 2.8 Motion

| CSS variable | Value | Use |
|---|---|---|
| `--dur-press` | 80ms | pressed bg |
| `--dur-hover` | 120ms | color/bg on hover (current value, keep) |
| `--dur-pop` | 160ms | menu, tooltip, toast in |
| `--dur-modal` | 200ms | dialog, palette, sheet, drawer in |
| `--ease-out` | `cubic-bezier(0.16, 1, 0.3, 1)` | entering |
| `--ease-in` | `cubic-bezier(0.4, 0, 1, 1)` | leaving (70% of enter duration) |
| `--ease-standard` | `cubic-bezier(0.4, 0, 0.2, 1)` | toggles, chevrons |

- What moves: opacity; menus and tooltips +4px slide from the anchor side; dialog and palette scale 0.98 to 1; drawer and sheet slide on X. Never animate panel width while dragging. No bounce.
- Endless motion is built for the 8 frames a second of `lib/motion-ticker.ts`: every duration is a whole number of 125 ms ticks, and keyframes sit on ticks. Details and the designs that were compared: `docs/design-icons.md`.
- Running (`.hm-beat`): a running state glyph holds still and blinks once per 2000ms (opacity and scale on the outer svg; 3 of 16 frames differ). Every running glyph blinks on the same tick. Nothing else in a glyph moves; chips do not pulse.
- Busy spinner (`ui/Spinner.tsx`, `.hm-spin8`): 8 dashes, a bright head and a fading tail, `rotate` with `steps(8)` over 1000ms: one dash per tick.
- Link into a running stage: fills toward the node (`scaleX`, then fades), 2000ms, on the same clock as the blink.
- Sidebar (`.sb-*` in `styles.css`): the current-row bar grows on Y over `--dur-pop`; chevrons turn over `--dur-hover`; an unfolded list slides in 4px over `--dur-modal`; a row added after the first paint slides in (`hm-pop`); a row that starts to need input flashes `--signal-soft` once over 600ms; a state glyph changes color over `--dur-modal`. The first paint does not move.
- Skeleton (`.hm-pulse`): one opacity pulse per list (1 → 0.45 → 1, 2000ms); the bars are still and do not repaint.
- `@media (prefers-reduced-motion: reduce)`: all transitions become opacity only at 80ms; the running glyph is still (its own shape, not a frozen frame); the spinner shows three dots; the stage link is a full line; the skeleton does not pulse; `scroll-behavior: auto`. This is CSS only; no component reads the media query. Every `.sb-*` class is listed there too (a test checks it): unfold is an 80ms fade, the bar and chevrons do not move, no flash.
- Window without focus: `lib/window-focus.ts` sets `data-window="inactive"` on `<html>`, in the app and in a browser tab. Every endless animation stops then and shows the still form of reduced motion (running glyphs, spinners, stage links, skeletons), and the title bar text turns `--muted`. A test checks that each endless animation in `styles.css` is listed. Endless motion animates `transform` or `opacity` only, never `background-position`.
- With focus, `lib/motion-ticker.ts` moves every endless CSS animation 8 times a second (125 ms), all on one clock, so the page draws 8 frames a second however many things move. It does nothing while the page is hidden. Transitions and one-shot animations run as usual.
- Feedback by wait time: under 100ms nothing; 100ms-1s the control is busy (disabled + label kept); 1-3s spinner in the control; over 3s a stage label ("Creating worktree...", "Pushing..."). Skeletons appear only after 200ms and stay at least 300ms (no flash, RA-29).

---------------------------------------------------------------------

## 3. Layout

### 3.1 App shell

```
+------------------------------------------------------------------+
| Title bar 36                                                     |
+-----------+------------------------------------+-----------------+
| Sidebar   | Main                               | Details         |
| 320       | view header 44 / workspace hdr 44  | 320             |
| (220-440) | tabs 36                            | (320-560)       |
|           | content                            |                 |
+-----------+------------------------------------+-----------------+
| Status bar 28                                                    |
+------------------------------------------------------------------+
```

Parts are flush (no floating cards): the shell color frames sidebar, title bar and status bar; main is one step lighter. Borders between parts: 1px `--line-strong`.

**Title bar** (new, `features/app/TitleBar.tsx`), 36px, `bg-shell`, bottom border `--line`:
- Left: in Electron on macOS a 76px empty gutter for the traffic lights (`:root[data-shell="electron"][data-platform="mac"]`); on Windows Electron a 140px right gutter for `titleBarOverlay` window controls. In the browser no gutter.
- Left group: sidebar toggle (`PanelLeft`, icon button 28), wordmark "Natamux" 13/600 (hidden < 1024), breadcrumb of the current view: `repo / T-0018 title` (13, ink-2, truncate, max 40%).
- Center: command center button, 28px high, width `clamp(200px, 36vw, 520px)`, `bg-input`, 1px `--control`, radius 6, `Search` 14 + "Search or run a command" muted + `Mod K` kbd at right. Opens the command palette.
- Right group: needs-you pill (hidden when 0): amber chip "3 need you" (`bg-signal-soft`, 1px `--signal-line`, text `--signal`, 12/500, 24px high); click opens the jump palette filtered to needs-you. Then "Start a task" (`Plus`, icon button with tooltip "Start a task  C"), details toggle (`PanelRight`).
- Electron: whole bar is `-webkit-app-region: drag`; every control `no-drag`. Window inactive (`:root[data-window="inactive"]`): title bar text and icons use `--muted`.
- Moves out of the sidebar header: "Natamux" title and the "Start a task" button (the sidebar keeps the filter field and lists).

**Sidebar** (`<nav aria-label="Workspaces">`), `bg-sidebar`, default 320 (the same as the details), min 220, max 440, resizable, collapsible (Mod+B or title bar toggle; dragging below 180 collapses). Structure top to bottom:
1. Filter field 28px (`ListFilter` icon, placeholder "Filter tasks", clear button, `/` kbd hint). 8px gutter.
2. Nav rows (28px): All tasks + count, Agents + count.
3. "Workspaces" header 28px, sticky (`position: sticky; top: 0`), with "Add a project" and "Start a task" icon buttons (24).
4. Scrolling tree of repo groups and workspace rows. 12px bottom padding.
Only part 4 scrolls (fixes NAV-16).

**Main** (`<main id="main">`), `bg-canvas`, `container-type: inline-size; container-name: main`. Min widths: terminal content 480, diff 560, table 360 (card list below 560).

**Details** (`<aside>`), `bg-surface`, default 320 (the same as the sidebar), min 320, max 560, resizable. Modes: docked, overlay sheet, full page (see 3.2). Open/closed remembered per task id in the view store (fixes WS-16). Until the user opens or closes them, the details of a task are open when they dock beside its workspace (the task has a workspace), so its progress is in sight; elsewhere, and as a sheet or a page, they start closed. A list address hides the details without counting as a close. Auto-closed when the Changes tab opens and main would drop below 900 (WS-03/WS-25).

**Status bar** (`features/app/StatusBar.tsx`), 28px, `bg-shell`, top border `--line`, items 24px (see 4.27).

**Panes** (terminal grid, agents grid): gap 8, outer padding 8, pane radius 8, 1px `--line-strong`, header 32, min pane 320 x 200. Grid uses `repeat(auto-fill, minmax(320px, 1fr))` unless the user picked a layout.

**Fixed heights**: title bar 36, view header 44, workspace header 44, tab row 36, section header 28, pane header 32, status bar 28, control 28 (dialog 32), list row 28, workspace row 44 (+16 for activity), table row 40.

### 3.2 Responsive rules

Breakpoints (Tailwind v4 defaults kept, plus one): `md` 768, `lg` 1024, `xl` 1280, `2xl` 1440 (set `--breakpoint-2xl: 90rem`). A hook `hooks/useLayoutMode.ts` mirrors them with `matchMedia` and returns `{ sidebar: 'docked' | 'drawer', details: 'docked' | 'sheet' | 'page', coarse: boolean }`. Content that depends on main width (table columns, diff mode, pane grid, header actions) uses container queries `@container main (...)`, not the viewport.

Extra rule at every width: details dock only if main stays >= 640 after docking; otherwise they open as a sheet. So a wide sidebar never crushes main.

| Width | Sidebar | Details | Main content | Title bar | Status bar |
|---|---|---|---|---|---|
| >= 1440 | docked 320 | docked 320 when open | table all columns; diff side by side; agents auto-fill | full | full |
| 1280-1439 | docked 320 | docked if main >= 640, else sheet | table drops Branch | full | full |
| 1024-1279 | docked, default 248, max 30vw | sheet: slides over main from the right, 360, `--shadow-2`, no scrim, stays until Esc / close / toggle | table drops Branch + PR; diff inline if diff width < 720; agents max 2 columns | wordmark hidden | Projects / Import become icon-only with tooltips |
| 768-1023 | drawer: 300 (max 85vw), slides from left over a scrim, opened by the title bar toggle or Mod+B, closes on pick / Esc / scrim click | sheet with scrim, 360 (max 90vw) | table: Id, Task, State, Last activity; diff inline only, file list collapses to a file picker above the diff; terminal panes stack when < 320 each; workspace header actions: Push stays, PR becomes icon+"PR" | breadcrumb shortened to task id | only Live dot, running, needs-you, `?`, account |
| < 768 (design at 390) | drawer full height, width min(360, 100vw) | full page with a Back button (`ArrowLeft`), replaces main | table becomes a card list; board shows one column at a time with a state segmented control on top; workspace: title wraps to 2 lines, Push/PR go into a "..." menu, tabs scroll, ONE terminal at a time with a pane switcher (Shell 1 / Shell 2 chips); agents: 1 column, each pane `min-height: 60vh`, scroll; diff inline + wrap on; dialogs become bottom sheets; toasts full width | 44px: menu, view title, needs-you pill, `+`; command center becomes a `Search` icon button | hidden; Live state is a 6px dot on the title bar menu button; status items move to the drawer footer |

Touch (`@media (pointer: coarse)`, any width): control height 40, list rows 48, workspace row 56, icon hit area 40 (icon size unchanged), resize handles hidden (use settings or drag-free defaults), hover-only controls are always visible (fixes RA-09).

Large screens (>= 1920): no max width; the Task column takes extra space; table text stays 13 (RA-08: the UI size setting covers users who want bigger).

Status bar overflow rule (all widths >= 768): items have a priority. When the bar's `scrollWidth > clientWidth` (ResizeObserver), drop labels of the lowest priority items first (icon + tooltip), then fold them into a "..." menu. Never wrap to a second line (fixes NAV-19, RA-04, WS-34).

---------------------------------------------------------------------

## 4. Components

Each lives in `packages/web/src/components/ui/` unless noted. "Keys" lists keyboard behavior beyond Tab.

### 4.1 Button (`styles.ts` `button()` + `ui/Button.tsx`)
- Variants: `primary` (bg ink, text canvas; one per view), `secondary` (bg raised, 1px control border, ink), `ghost` (transparent, ink-2 text, hover bg-hover + ink), `danger` (bg danger, white), `link` (no box, `--link`, underline on hover).
- Sizes: `xs` 24 (pad x 8, icon 12, only inside rows/pane headers), `sm` 28 (default, pad x 10, icon 14), `md` 32 (dialogs, sign-in, empty states; pad x 12, icon 14). Text 13/500. Gap 6. Radius 6.
- States: hover (primary 85% ink; secondary bg-hover + control-hover border), active (bg-active), focus-visible (2px `--focus` ring, offset 1), busy (the 8-dash spinner of `ui/Spinner.tsx` replaces the icon, label stays, width locked with `min-width` from first render), disabled (opacity 0.5).
- Disabled with a reason: use `aria-disabled="true"` (stays focusable) + tooltip with the reason ("Nothing to push: up to date with origin/<branch>"). Click does nothing. Use `disabled` only when no reason is useful.
- Optional shortcut chip at the end (`Kbd` on-inverted variant for primary, 70% opacity). Never on Cancel.
- Labels: verb first, sentence case, no period, max 3 words when possible.

### 4.2 Icon button (`styles.ts` `iconButton()` + `ui/IconButton.tsx`)
- Square 28 (icon 16) in title bar/toolbars; 24 (icon 14) in rows and pane headers. Radius 6. Idle color muted; hover bg-hover + ink; pressed state for toggles via `aria-pressed` = bg-selected + ink.
- Required props: `label` (sets `aria-label` and tooltip), optional `shortcut`. No `title` attribute.
- Hover-revealed icon buttons (row "...") use `opacity-0` + `group-hover/focus-within:opacity-100`; always visible on coarse pointers and while their menu is open.

### 4.3 Input (`styles.ts` `input`, `ui/SearchField.tsx`)
- Height 28 (32 in dialogs), pad x 10, bg input, 1px control border, radius 6, text 13 ink, placeholder `--placeholder`.
- Hover: control-hover border. Focus: border `--focus` + no box-shadow; the ring is the border plus `outline: 1px solid var(--focus); outline-offset: -1px` on the wrapper via `:focus-within` (one ring, fixes RA-24, WS-26). Inner `<input>` has no outline.
- Invalid: `aria-invalid="true"` -> border danger-text; message below in 12px danger-text with `CircleAlert` 12, linked by `aria-describedby`.
- SearchField: leading icon 14 muted; trailing clear button (`X` 14, icon button 20) when there is a value; `/` kbd hint when empty and not focused. Esc: first press clears the value, second press blurs (fixes NAV-15).
- Textarea: same tokens, pad 8/10, min 3 rows, resize vertical.

### 4.4 Select (`ui/Select.tsx`)
- Native `<select>` (keyboard and screen readers work), drawn like Input. Sizes 28 / 32. Chevron `ChevronDown` 14 muted at right 8. One component for every select in the app, including `TaskEditForm` priority (fixes NAV-10).
- Long or searchable lists (branches, repos with many entries, "add a pane") use `ui/Combobox.tsx` (in-house, ARIA combobox + listbox): input 28/32, popup with `--shadow-1`, rows 28, groups with caps headers ("Recent", "Branches", "Task branches"), type to filter, Up/Down, Enter, Esc. Fixes NAV-28, WS-28.

### 4.5 Checkbox
- Native `<input type="checkbox">` 14px, `accent-color: var(--accent)`, inside a `<label>` row of height 28 with gap 8. Focus ring on the box. No custom drawing.

### 4.6 Segmented control (`ui/Segmented.tsx`)
- For 2-4 exclusive view options (Table/Board, Side by side/Inline, pane layout 1/2/4/Auto).
- Track: bg raised, 1px line, radius 6, pad 2, height 28. Segment: height 22, pad x 8, radius 4, 12/500, icon 14. Selected: bg surface (light) / `#2a2c31` (dark), ink, `0 1px 2px rgb(0 0 0 / 0.2)`. Unselected: muted, hover ink.
- Semantics: `role="radiogroup"` + `aria-label`; segments `role="radio"` `aria-checked`. One tab stop; Left/Right/Home/End move and select.
- Labels are always the option name, never the current state (fixes WS-03). When main < 560, labels hide and segments become icon-only with tooltips.

### 4.7 Tabs (`ui/Tabs.tsx`)
- Row 36, bottom border line. Tab: pad x 10, 13/500, icon 14 + label gap 6, muted; hover ink; selected ink + 2px ink bar at the bottom (inside the row). Optional count badge.
- `role="tablist"`/`tab`/`tabpanel`, `aria-controls`. One tab stop; Left/Right/Home/End move and activate. Number keys 1-4 activate workspace tabs (see 5.1).
- Overflow: horizontal scroll with 16px edge fades; no wrap. Right side of the row holds tab-specific actions (New shell, layout).

### 4.8 List row (generic)
- Height 28, pad x 8, radius 6, gap 6, icon 14, text 13. Trailing meta 12 muted, right-aligned, tabular.
- States: hover bg-hover; selected bg-selected; selected + list focused bg-selection-active; focus-visible inset 2px ring (`outline-offset: -2px`).
- Used by: changed-file rows, menu rows, palette rows (32 there), nav rows.

### 4.9 Tree row: workspace row (`features/sidebar/WorkspaceRow.tsx`)
The height follows what the row needs (`rowTier` in `features/sidebar/sidebar-tree.ts`). Pad 10 left and right, radius 6 (8 for a card), no chevron:
```
attention (needs-input, failed, blocked), min 68px, card tinted with the state color (.row-attn):
[glyph10] Title (13/600, ink, truncate)                [unseen dot] [age 12 muted | ... on hover]
         Which test runner should I use? (12 ink, up to 2 lines)
         [Answer] T-0018 3/5 Review                                [agent]
live (running, review, ready), min 40px:
[glyph10] Title (13/500, ink, truncate)                [unseen dot] [age 12 muted | ... on hover]
         T-0018 3/5 Review [subtasks 1/3] Edit a.ts (ink-2, truncate)  [PR #12] [agent]
quiet (done, cancelled, draft), min 24px:
[glyph10] Title (12/400, ink-2; muted when finished, cancelled line-through)   [age 12 muted]
   |  [glyph8] [agent12] Implement  Edit a.ts …                 S1
   |  [term12] zsh                                              S2
```
- Text column starts at 32px (10 pad + 14 glyph slot + 8 gap); lines 2 and 3 indent 22px from the row's content edge, so they start under the title.
- Glyph (`components/ShapeGlyph.tsx`): the state shapes of 4.10 at 10px in a 14px slot, for every tier, in `--chip-fg`. 10px matches the cap height of 13px Inter; 8px read as a bullet and blurred the small shapes. Tooltip and screen readers get the state label.
- Task glyph: `running` while any row or parallel stage under the task runs, else the task state (`taskGlyphState`).
- Only the open workspace shows the rows under it, and the parent of an open subtask. No row opens by itself, and the user cannot fold the open one. A folded row still shows its subtasks in an attention tier (needs-input, failed, blocked) as cards under it (T-0022). The row carries `aria-expanded` when it has work under it. A click, Enter or Right on a folded row opens that task.
- Live line 2: repo chip (pinned rows), task id in mono (tooltip: branch), the step `n/m` (only with 2 steps or more; a parallel group is one step), the current stage (in a group the most urgent stage), subtasks `done/all`, what the agent does (running and folded only; open, the shell rows say it), pull request number in the checks color, the icon of the task's agent (icon only, tooltip: its name; folded and open, since the shell rows name the agent of each stage). Screen readers hear every step through `aria-describedby` ("Steps: Spec done, Implement needs input, …").
- Attention card: the question (needs-input; "Waiting for your answer" until it is known) or the reason (failed, blocked) on up to 2 lines, then id, step and stage. A needs-input card has an **Answer** button (`Answer T-0018`, not a tab stop) that opens the terminal of the waiting shell, or the first shell.
- Under the open row, inside a 1px `--line` guide under the task glyph (`ml-4`): first one row per open shell in shell order, then a stand-in row for each parallel stage with no shell yet, then the subtask rows (full rows, one level only). Shell rows are 22px (40 on touch), glyph lined up with the title: shape glyph 8 (muted ring with a dot, tooltip "State not known yet", for an agent with no state yet, such as one started by hand that has had no prompt yet; `SquareTerminal` 12 muted for a shell with no agent), agent icon 12 (tooltip: the agent's label), stage name (a shell that a stage of an earlier step opened keeps that stage and shows done, or the state its agent's hooks give later) or "Shell 2" for an agent on no stage or the program (`bash`, `zsh`) for a shell with no agent, what the agent does (muted, truncate), shell tag `S1` (11 mono muted, tooltip "Shell 1"; none when the name already says the shell). Accessible name: "Implement, Claude Code, Shell 1, Running"; "Shell 2, Claude Code, State not known"; "Shell 2, zsh". They come from `GET /tasks/<id>/terminals` (`terminals`, `lanes`, `programs`), read again when the live feed says a shell started or ended, its program in front changed, or a hook changed its agent's state (`shells.changed`), and every 5 min as a fallback, for tasks that are running or need input, and the open workspace. A pick opens the task's terminal on that shell (`requestShell`); a shell that closed since then gives the first agent shell, as opening the workspace does.
- A finished or pinned subtask stays where it was (finished list, pinned group). An open subtask of a finished parent stays at the top level, so it is never folded away in "N finished". A subtask whose parent is not shown stays at the top level. A filter that matches a subtask keeps its parent in view, open, with the subtask under it.
- Folds of groups and "N finished" lists are kept per browser in `natamux.sidebar-folds` (only what the user picked; the newest 300). Task rows keep no fold.
- Age moves to line 1 and swaps with the "..." button on hover/focus, so the slug gets the full line 2 (fixes NAV-27 truncation) and nothing overlaps.
- States: hover bg-hover; current (open in main) bg-selected + a 2px `--accent` bar at the left (`.sb-row`); keyboard focus ring inset; selected + focused list bg-selection-active.
- Finished rows: title and meta use `--muted` (no opacity, fixes RA-10/11); cancelled title `line-through` in the finished group only.
- Unseen change (agent finished or asked since the user last opened it): title weight 600 and a 6px accent dot before the age; the row name ends with ", unseen" (P2).
- Rename: F2 or double-click title or menu "Rename". Field replaces the title line only (the row keeps its height and other lines), height 24, all text selected in both paths, Enter saves, Esc cancels, focus returns to the row (fixes NAV-23).
- Row menu (right-click, Shift+F10, ContextMenu): View changes, Rename, Add subtask… (`ListPlus`; only for a task with no parent, in every state; opens the Add subtask dialog), Pin to the top, Copy branch name, Copy folder path, Remove workspace, Delete task.
- Group (repo): a card around the header and its rows (radius 10, 1px `--line`, `--surface` 55%; 8px between groups; rows inset 2px, 4px below the last). The card sets `--project` to the project color (see Project colors). Header: 28px band with a gradient from `--project` 16% to transparent at 85% (20% to 4% on hover), chevron 12, `FolderGit2` 14 in `--project`, name 13/600 ink, project count badge (screen readers: "3 workspaces"), hover "+" (24), hover gear "Settings of <project>" (24, opens Project settings), hover blur toggle "Blur names" / "Show names" (24, `EyeOff` / `Eye`, `aria-pressed`, always shown while on), hover "more" menu (24): Start a task, Settings, Blur names / Show names, separator, Remove finished workspaces (only while the group has some), separator, Remove project (for every project; disabled with the line "Remove its workspaces first" while the project has workspaces; asks first). Right-click, Shift+F10 or ContextMenu on the header opens the same menu. Collapsed group with the current or a needs-you workspace inside shows a 6px dot in that state color after the count (NAV-26).
- Blurred project (for screen sharing; per browser, `localStorage` `natamux.blurred-projects`): the card gets `data-blurred`; every `data-private` text in it blurs 5px: project name, task titles, ids, steps, stages, questions, activity, agent row labels and shell tags, also in subtasks, finished rows and the project's pinned rows. Glyphs, ages, counts and pull request numbers stay sharp. Blurred text has no tooltip, and labels that show as tooltips say "this project" instead of the name. The rename field stays readable.
- "N finished" disclosure: 24px row, 12 muted, chevron 12, indent 32. Hover `FolderMinus` button (24) "Remove finished workspaces in <project>": asks first, listing the task ids; removes the workspaces one after the other, skips any with changes that are not committed, and ends with one toast. Pinned workspaces are not part of it.
- No projects: one 12 muted line "No projects yet" and an "Add project" text button that opens the projects panel. "Add task" is off with "Add a project first"; "Start a task" opens the projects panel with the toast "Add a project first."
- Pinned group: caps header "Pinned" + count; pinned rows show the repo name as a prefix chip on line 2 (NAV-25).
- Keys (roving tabindex, one tab stop for the whole tree, fixes NAV-17, RA-23): Up/Down move, Home/End, Right expands a group, opens a folded task / moves to first child, Left collapses a group / moves to parent (the open task does not fold), Enter opens, F2 renames, Shift+F10 or ContextMenu opens the row menu, Delete asks to remove the workspace, type-ahead by title.

### 4.10 Status glyphs (`components/ShapeGlyph.tsx`, `components/StatusGlyph.tsx`)
One family of filled shapes for every place: the table, palette, pane tabs and terminal header (`StatusGlyph`: 10px shape in a 16px box), the sidebar (10px; shell rows 8px) and the stage markers. Color `--chip-fg` of the state. Every state has its own silhouette, so color is never the only cue (fixes WS-33, WS-35). Each art size is drawn on whole pixels (`ShapeSvg`, viewBox = size + padding). Hollow means not started, full means alive, half means running but quiet.

| State | Shape | Motion | Label (sr-only + tooltip) |
|---|---|---|---|
| draft | dashed ring (4 dashes) | — | Draft |
| ready | ring | — | Ready |
| running | full disk | blinks once per 2000ms (`.hm-beat`) | Running |
| running but quiet | ring, lower half filled, needs-input color | — | Running, but quiet |
| needs-input | diamond | — | Needs input |
| review | rounded square | — | In review |
| blocked | two bars | — | Blocked |
| failed | triangle | — | Failed |
| done | check | — | Done |
| cancelled | dash | — | Cancelled |
| state not known yet | ring with a dot, muted | — | State not known yet |

Stage markers (`features/stages/StageMarker.tsx`) use the same shapes: done is a green disk with the check; the current stage is a disk in `--chip-bg` with a 2px ring in `--chip-fg` and the state shape inside (only running blinks); upcoming is a muted ring; skipped is a muted dashed ring with the dash. Inner shapes are an even number of pixels.

Dots (`.state-dot`, 8px) stay for chips, filters and board headers. Draft dot = 1.5px ring.

### 4.11 Chips and badges
- State chip (`.state-chip`): 20px high, pad x 8, radius full, 12/500, 6px dot, `--chip-bg` / `--chip-fg`. No pulse animation.
- Count badge: min-width 16, height 16, pad x 5, radius full, 11/600 tabular, bg badge, ink-2. Signal variant (needs-you): bg signal-soft, text signal, 1px signal-line. Accent variant: bg `--accent` 16%, text link (accent text is under 4.5:1 on it in light). Project variant (project headers): bg `--project` 10%, text `--project`.
- Label chip (task labels, relation "part of T-0014", "waits for T-0009"): 20px, pad x 6, radius 4, 12/400, 1px line, ink-2.
- Kbd (`ui/Kbd.tsx`): height 18, min-width 18, pad x 4, radius 4, 11/500 mono, bg raised, 1px line, `box-shadow: inset 0 -1px 0 var(--line-strong)`, muted text. Renders platform keys: macOS glyphs joined (`⌘K`, `⇧⌘P`), Windows words with `+` (`Ctrl+K`). Source: `lib/keys.ts` `formatShortcut('Mod+K')`.

### 4.12 Needs you (`features/app/NeedsYouAnnouncer.tsx`)
- No sidebar section since T-0022. The tree's attention cards (4.9) name each waiting workspace once, with the question; a strip above the tree said it a second time and took up to 40% of the height.
- Signals that stay: title bar pill (3.1), status bar / drawer footer count, document title "(N) Natamux", notifications, the palettes' Needs you group and scope (4.17).
- The pill opens the jump palette scoped to needs-you. With that scope it lists tasks without a workspace too, because the pill counts them.
- A polite live region (`role="status"`, sr-only), outside the sidebar, says "<title> needs you" when a task starts to wait. The tasks that wait when the page opens are not said.

### 4.13 Menu (`ui/Menu.tsx`)
- Popup: min-width 200, max 320, pad 4, radius 8, bg popover, 1px line-strong, `--shadow-1`, z popover.
- Item: 28px, pad x 8, radius 4, 13/400, icon 14 ink-2, gap 8; shortcut at right 12 muted (Kbd text, no box); submenu chevron 12; check mark 14 for toggles.
- Groups: `type: 'separator'` (1px line, margin 4 0) and `type: 'label'` (caps, 24px). Destructive items go last, after a separator, `--danger-text` (NAV-13).
- Position: below the trigger, right edges aligned, 4px gap; flips above if it would overflow; never covers its trigger.
- Keys: opens with Enter/Space/Down (focus first item), Up opens focus last; Up/Down/Home/End; type-ahead; Enter/Space activates; Right opens submenu, Left closes it; Esc closes and returns focus; Tab closes and moves on.
- Context menu: same component at the pointer, opened by right-click on rows and by Shift+F10 / ContextMenu key on the focused row.

### 4.14 Dialog (`ui/Dialog.tsx`)
- Sizes: `sm` 400, `md` 520, `lg` 640. Max height 80vh, body scrolls. Top 14vh.
- `xl` 1120 is a screen of its own (Project settings): height 88vh, top 6vh, body fills the rest and scrolls. Below 768 it takes the whole screen (100dvh, no radius).
- Panel: radius 12, bg popover, 1px line-strong, `--shadow-2`. Scrim `--scrim` (no blur), covers everything except the toast layer.
- Header pad 20 20 8: title 15/600, optional description 13 muted. Close icon button 28 (icon 16) at top right (NAV-29: was 28 with 12 icon).
- Body pad 12 20, fields stack gap 16, label 12/500 ink-2 above the control, helper 12 muted below.
- Footer pad 12 20, border-top line, right-aligned: Cancel (ghost) then primary (with `Mod Enter` kbd). Mod+Enter submits from any field.
- Focus: first field or `[data-autofocus]`; destructive confirms focus Cancel. Tab is trapped (wraps); everything outside gets `inert` (set on `#root` while open; the dialog portals to `body`). Esc closes unless busy. Focus returns to the opener (fixes NAV-01, RA-25).
- Submitting closes the dialog at once when the work can continue in the background; a toast or progress row reports the result.
- < 768: bottom sheet, full width, radius 12 12 0 0, max height 90vh, slides up.

### 4.15 Toast (`ui/Toaster.tsx`, `lib/toasts.ts`)
- Bottom-right, 16px from the right edge, 12px above the status bar (40px from bottom), z toast. Width 360 (max `100vw - 32`). Max 3 visible; older ones collapse.
- Box: pad 12 12 12 14, radius 8, bg popover, 1px line-strong, `--shadow-1`, gap 10. Icon 16 top-aligned (success add, error danger-text, info accent). Text 13 ink; optional detail 12 ink-2.
- Optional actions: `{ label, run }[]` as link-style buttons under the text ("Open", "Undo", "Show details").
- Timing: info/success 5s; error with an action stays until dismissed; error without action 10s. Timer pauses on hover and focus-within. Dismiss icon button 24.
- `role="status"` (info) / `role="alert"` (error) on each item, inside a polite live region (already partly done; keep).
- Use for confirmations ("Committed 3 files", "T-0019 is ready" + Open). Persistent problems stay inline in their view.
- < 768: full width at the bottom with 8px margins.

### 4.16 Tooltip (`ui/Tooltip.tsx`, new)
- One global tooltip layer mounted once in the Dashboard. Triggers opt in with attributes: `data-tip="Close shell"`, optional `data-tip-kbd="Mod+W"`, optional `data-tip-side="bottom"`. `IconButton` sets them from its props. Text that may be truncated gets `data-tip-overflow` and shows its full text only when `scrollWidth > clientWidth` (fixes RA-32, WS-27).
- Shows after 400ms hover, or at once on keyboard focus (`:focus-visible`). Once one is visible, moving to another trigger within 600ms shows the next at once. Hides on pointer leave, blur, Esc, scroll, pointer down.
- Box: bg tooltip-bg, text tooltip-fg 12/16, pad 4 8, radius 4, max-width 280, `--shadow-1`, 6px from the trigger, flips to stay in the viewport. Content: label, then Kbd chips.
- `role="tooltip"`; the trigger gets `aria-describedby` only when the tooltip adds something beyond its `aria-label` (shortcut, full text).
- Rules: tooltips name icon-only controls and show shortcuts; they never hold the only copy of important info. All `title=` attributes in the app are replaced (grep `title=` under `components/`).

### 4.17 Command palette (Mod+K) and jump palette (Mod+J) (`features/palette/CommandPalette.tsx`, `lib/commands/`)
Shared shell:
- Width 640 (max `100vw - 32`), top 12vh, radius 12, bg popover, 1px line-strong, `--shadow-2`, scrim `rgb(0 0 0 / 0.24)` dark / `rgb(15 17 20 / 0.12)` light. < 768: full width sheet from the top.
- Input row 44: `Search` 16 muted, input 15px, right: scope chip, "Needs you" (signal colors) or a project name (line, raised, ink-2; max width 160, truncated), removable with its X or Backspace on empty input.
- List: max height `min(440px, 60vh)`, rows 32 (one line) or 44 (two lines: title + 12 muted id/branch/repo), radius 6, inset 6. Group headers caps 24px. Matched characters in ink 600 + accent underline-free. Selected row: bg selection-active + `inset 0 0 0 1px color-mix(in srgb, var(--accent) 40%, transparent)` (visible on white too). Right side: Kbd for commands, state glyph + age for tasks.
- Footer 28: hints in 12 muted: `↑↓ move`, `↵ open`, `⇧↵ add to Agents`, `⌥1-9 jump`, `Esc close`.
- Keys: Up/Down (wraps), PageUp/PageDown, Enter, Shift+Enter, Alt+1..9 picks row N (Cmd/Ctrl+digits belong to the browser), Tab cycles scope chips (none → Needs you → each project that has a task, in project order → none), Esc closes and restores focus. Order is frozen while open (live updates do not move rows).
- Mod+K (everything): empty query shows "Needs you" (max 5), "Recent" (workspaces opened, max 5), "Suggested" commands. Typing searches commands (with category prefix: "Task:", "Workspace:", "View:", "Theme:", "Project:"), tasks (id, title, branch), workspaces and projects. `>` prefix = commands only; `#` prefix = tasks by id. No match: rows "Add task “query”" and "Start task “query”". A project scope keeps only that project's tasks and hides Recent and Suggested; its empty state is "No workspace in <project>."
- Mod+J (jump): only workspaces and their terminals (Shell 1 agent, Shell 2...). Empty query: needs-you first (needs-input > review > failed > blocked), then running, then recent. Enter opens the workspace on that terminal; Shift+Enter adds it to the Agents grid.
- `lib/commands/` is the single registry: `{ id, title, category, shortcut?, when?(ctx), run(ctx) }`. The palette, menus (shortcut column), tooltips and the shortcut sheet all read it, so a shortcut is defined once.

### 4.18 Shortcut sheet (`?`) (`features/app/ShortcutSheet.tsx`)
- Dialog `lg`, title "Keyboard shortcuts", search field, groups from section 5.1, two columns >= 1024. Opened by `?` and the status bar `Keyboard` button. Replaces the status bar hint text (NAV-19).

### 4.19 Empty state (`ui/EmptyState.tsx`)
- Centered in its container, max width 360, gap 8. Icon 32 muted (stroke 1.5) in a 56px circle bg raised; title 15/600; body 13 muted (1-2 sentences); one primary action (`md`) + optional link; optional Kbd hint.
- Variants with exact copy:
  - No tasks: "No tasks yet" / "Add a task to the backlog, or start one now in its own workspace." / [Add task `N`] [Start a task `C`].
  - Filter miss: `No tasks match “zzzz”` / "Check the spelling or clear the filter." / [Clear filter]. Sidebar group under a filter: "No matches in natamux (8 hidden)" (fixes NAV-15, WS-24, RA-30).
  - No changes: "No changes" / "The workspace matches its last commit."
  - No agents: "No agent is working" / "Start an agent on a task, or add a pane." / [Add a pane].
  - Error: icon `CircleAlert` danger-text, title "Could not load tasks", friendly sentence, [Retry], "Show details" disclosure with the raw message in mono 12 (NAV-18 pattern).

### 4.20 Loading skeleton (`ui/Skeleton.tsx`)
- Bars radius 4, bg skeleton, heights 10 (meta) / 12 (title), widths from a fixed list (60%, 40%, 75%...) so it does not look random each render. The list pulses its opacity once per 2000ms (`.hm-pulse`); a bar that stands alone pulses by itself. Still with reduced motion and in a window without focus.
- Same row heights as the real content: table 8 rows of 40, sidebar 5 rows of 44, details 3 sections, changes list 6 rows of 28, history 8 rows. Monaco and terminal loading use a skeleton block, not "Loading the viewer." text.
- 200ms delay, 300ms minimum.

### 4.21 Scrollbar
- `* { scrollbar-width: thin; scrollbar-color: var(--scroll) transparent }`, hover `--scroll-hover` (keep). WebKit (Electron): 8px, thumb radius 4 with 2px transparent border, no track.
- Scroll containers with content above the fold get a "scrolled" hairline: the sticky header shows a bottom `--line-strong` border only when `scrollTop > 0` (`data-scrolled`).
- Horizontal overflow (table, filter chips, tabs, board) shows 16px edge fades in the container's background color.
- Terminal: xterm scrollbar visible on pane hover, 8px, same tokens, no border line beside it (WS-30).

### 4.22 Resize handle (`ui/ResizeHandle.tsx`)
- Hit area 8 (`-mx-1`), visual 1px line-strong. Hover after 150ms: 2px accent at 60%. Dragging / focused: 2px accent. Cursor `col-resize`.
- `role="separator"`, `aria-valuenow/min/max`, `aria-valuetext="288 pixels"`.
- Keys: Left/Right 16px, Shift+arrow 64px, Home = min, End = max, Enter = reset to default. Double-click resets (fixes NAV-27).
- Sidebar: dragging below 180 collapses it; the collapsed state shows the title bar toggle as pressed=false.
- Hidden on coarse pointers.

### 4.23 Terminal pane header (`features/terminals/TerminalPane.tsx`)
- Height 32 fixed (fixes the 29/30px mismatch, WS-12), pad 0 4 0 10, bg surface, border-bottom line.
- Left: status glyph 16 (agent panes) or `SquareTerminal` 14 (shells); title 12/500 ink ("Shell 2", or in Agents: `T-0018` mono muted + task title as a link-style button with pointer cursor, underline on hover, tooltip "Open workspace T-0018", WS-20); agent badge only when an agent runs in it, naming the agent the daemon found there, not the task's (WS-13); state text 12 muted truncate; an agent on no stage (started by hand in an extra shell) shows the state its own hooks gave, or none ("Running · Edit notes/T-0018.md", WS-21).
- Right: icon buttons 24: Maximize/Restore (`Mod+Shift+Enter`), Open workspace (agents grid only), Close. Close tooltip: "Close shell (ends the process)" and confirms when a foreground process other than the shell is running.
- Focus: the focused pane gets `outline: 1px solid var(--focus); outline-offset: -1px` on the pane frame; header title ink, other headers muted.
- Opening another task's workspace on its Terminal tab puts the keyboard in its first agent shell (the lowest-numbered shell that runs an agent, else Shell 1), once the shells are read. Not on a phone, not when opened from an address, and a shell picked in the jump palette wins.
- Needs-input pane: one mark only, the state text "Needs input" in signal. No top bar and no header tint (WS-21, T-0015).
- In a workspace the mark shows only while two or more agent panes are on the screen, so it says which one waits. With one agent pane (also when one is maximized, and on a phone) the pane shows no state text for needs-input: the header chip and the stage card in the details (a banner under the header on a phone) say it. The Agents grid has no banner, so its panes always show the mark.
- Body: bg terminal, pad 8 top and bottom, 10 left, 2 right (xterm keeps 8 for its scrollbar, so text has 10 on both sides). 13px, line height 1 (xterm multiplies the font's own height: about 17px rows, as in Ghostty and VS Code), bold 700 in its own color (not bright), block cursor, outline cursor when unfocused, Alt + wheel scrolls 5×. "Jump to latest" pill (24px, radius full, bg popover, `--shadow-1`, `ArrowDown` 14) bottom-right when scrolled up (WS-30). Connection states ("Connecting", "Reconnecting", "Could not start" + [Start a new shell]) as a 28px strip at the top of the body; "Connecting" and "Reconnecting" lead with the 12px `Spinner`, which fades in after 200 ms (`.hm-late-in`), so a quick connect shows none. A shell that comes back after a restart (`restoring` of `GET /tasks/<id>/terminals`) is covered by an overlay over the whole body (bg terminal, centered `Spinner` 16 and "Restoring GitHub Copilot…" or "Restoring shell…", 12px muted, `role="status"`) until it leaves `restoring`; the overlay fades out in 150 ms and never changes the terminal's size. "Reconnecting" shows while a dropped connection comes back by itself; it has no button, the screen stays as it was, and the output missed meanwhile follows. If the shell is gone by then (a daemon restart), the pane shows the ended state below. A pane that comes back (another tab, another task) shows the same screen and scroll position at once, with no strip; "Connecting" shows only for a new connection. While a new connection writes the shell's history, the screen stays hidden and the body shows the pane's skeleton; the final screen then shows at once, scrolled to the bottom, so no history scrolls by. An ended shell dims the screen (opacity 60%), hides the cursor and takes no input; a 32px bar at the bottom (bg surface, border-top) shows the task state glyph (done, cancelled) or `SquareTerminal`, "The shell ended." (with the exit code when it is not 0 and the task is not finished; the finished banner says how the task ended), and [Open a new shell].
- With two or more panes, the panes without the focus are not dimmed. Only the focused pane shows the focus outline.

### 4.24 Diff view chrome (`features/git/ChangesPanel.tsx`, `ChangedFiles.tsx`, `DiffToolbar.tsx`, `MonacoDiff.tsx`, `ImageDiff.tsx`)
- Layout (main >= 720): left file panel resizable (default 260, min 200, max 420) | right diff column. Below 720: file picker (Select) above the diff, full width, in the order of the file panel.
- File panel (`ChangedFiles`): header 32 ("4 changed files" 12/500 muted, then `+340 −88` mono 12 summed over the files with counts; right: in tree view "Collapse all folders" / "Expand all folders", the "View as tree" toggle (aria-pressed, remembered in `natamux.changes-view`), "Rollback…" (Undo2), Refresh). Below it, one `role="tree"` with rows of `--row-h`.
  - List view: status colour on the file name, folder 12 muted truncated at the start, `+12 −3` when the panel is at least 15rem wide, status letter in a 16px mono box with tooltip.
  - Tree view: folders first, natural sort, single-child folders compacted ("web/src"), 12px indent a level, indent guides on hover or focus, chevrons on folders. A collapsed folder shows its summed counts. Collapsed folders are kept per task while the page is open. Arrow keys, Home, End and type-ahead work as in VS Code.
  - Per-file counts come from `git diff --numstat`, and a line count for untracked text files. Binary files, files that are too big, and untracked files after the first 300 have no counts.
  - Rollback: the row menu (right click, Shift+F10, ContextMenu key) has "Rollback…" for a file, or for every file under a folder; disabled with the hint "Resolve the conflict first" when only conflicts are left. `RollbackDialog` (md): title "Roll back changes", an "All files" checkbox (indeterminate when some), one row per file (checkbox named "<path>, <status label>", file icon, path, status letter), conflicts unchecked and disabled. Danger text "N new files will be deleted." for A, U and R files. Footer: ghost Cancel (focused) and danger "Roll back N files", off with "Check a file first" or "Another git action is running". The success toast has Undo.
  - Commit box docked at the bottom of the panel: textarea (3 rows, placeholder "Commit message"), then a row: ghost `Generate` (Sparkles icon, tooltip "Draft a message with the task's agent", `Mod+Shift+G`) and primary `Commit all` filling the rest with `Mod Enter` kbd. While `Generate` runs it is busy and `Commit all` is off with the reason "Drafting the message…". The draft replaces the text only when the user did not type meanwhile. `Commit…` in the header drafts when the box is empty.
- Diff toolbar (`DiffToolbar`, min 36, wraps):
  - Left: the path (mono 12, folder muted / name ink; a rename shows `old → new`, tooltip "Renamed from x to y"), Copy path, badge "New file · N lines" / "Deleted · N lines".
  - Right: `+12 −3` (text diffs); change navigator (Previous change `Shift+Alt+F5`, "2 of 7" or "7 changes", Next change `Alt+F5`; wraps at the ends); Segmented [Side by side | Inline] (icons only under 960; [Side by side | Stacked] for images); Collapse unchanged lines (aria-pressed); Wrap long lines (aria-pressed); Show image preview (SVG only); Diff options menu (Ignore leading and trailing whitespace, Show moved code); prev/next file (`[` `]`, in the order of the file panel).
- Remembered per browser: `natamux.diff-side-by-side` (on), `natamux.diff-collapse-unchanged` (on), `natamux.diff-ignore-whitespace` (on), `natamux.diff-show-moves` (off), `natamux.diff-wrap-prose` (on), `natamux.diff-wrap-code` (off).
- Side by side is offered only when the diff column >= 720. Below it the control shows Inline selected and Side by side disabled with tooltip "Needs a wider window" (WS-03). The stored choice is kept and comes back on a wider window. Monaco's own switch (`useInlineViewWhenSpaceIsLimited`) is off, so only one rule decides.
- Monaco fills the remaining height (`flex-1 min-h-0`, no `70vh`, WS-15). Options: theme from the app theme (not `prefers-color-scheme`, root cause of the white editor WS-01), `fontFamily` = `TERMINAL_FONT` (JetBrains Mono, as the terminal; `remeasureFonts()` once it loads), `fontSize` 12, `lineHeight` 18, `lightbulb` off, `glyphMargin: false`, `diffAlgorithm: 'advanced'`, `renderOverviewRuler: true` (green and red marks), `hideUnchangedRegions` (context 3, reveal 20) from the toggle, sticky scroll, `wordWrap` from the toggle (prose files wrap by default, WS-04), `scrollbar: { verticalScrollbarSize: 8, horizontalScrollbarSize: 8 }`.
- Monaco code (`features/files/monaco-setup.ts`): the editor API, the parts these views use (cursor and word moves, diff actions with F7, find, folding, sticky scroll, context menu) and the colors of the languages in `features/files/language.ts`; no language services, so only the editor worker. JSON gets colors only. The dashboard loads the diff code when the page is idle (`prefetchWhenIdle`).
- The editor is built again only for a new file or a new UI size. Every other option, and new text from a refresh (after `workspace.dirty`, or the 60 s fallback), updates it in place, so the scroll position stays.
- New or deleted files are not diffs: show a read-only single editor with the whole file decorated as added (or removed), and no phantom empty line (WS-02). An empty one shows "Empty file."
- Notices instead of an editor: "Renamed without changes.", "The text did not change. Only the file mode may differ.", binary, larger than 1 MB. Above the editor, when the diff finds nothing: "Only whitespace or line endings changed." with [Show whitespace].
- Images (png, jpg, gif, webp, and svg on demand): Before (last commit) and After (working tree), side by side or stacked, on a checkerboard, with the pixel size and the file size. Read from `GET /tasks/:id/changes/image` (5 MB limit, `nosniff`, sandbox CSP).
- Comment-for-the-agent box docked at the bottom of the diff column: one line field that grows to 4 lines, label "Comment on line 12 for the agent" (or "on this file"), [Send to agent] with `Mod Enter`. Stepping to a change moves the cursor, so the comment is about that line.
- Ask about a selection (`features/files/monaco-ask.ts`): a non-empty selection in any editor of the diff (or a whole file) shows a secondary xs [Ask agent] button (`shadow-pop`) as a Monaco content widget at the cursor, above the line or below it. Name "Ask the agent about lines 12–14", tooltip `Mod Shift A`. The same action is `Mod+Shift+A` in the editor and "Ask agent about the selection" in its context menu. It adds the whole lines to the comment box: a chip "Lines 12–14" (left side: "(before the change)"; over 10,000 characters: "· too long to quote, only the line numbers are sent") with an xs [×] "Remove the selected lines", and the label "Comment on lines 12–14 for the agent". A successful send or another file drops it; a failed send keeps it.

### 4.25 Table (`features/tasks/TaskTable.tsx`)
- Frame: radius 8, 1px line, bg surface. Header 32, sticky, 12/500 muted, bottom border line. Rows 40 fixed, bottom border line, cells pad x 12, one line, ellipsis (fixes NAV-06, RA-06).
- Columns (container query on main):

| Main width | Columns |
|---|---|
| >= 1000 | Id 72 (mono 12 muted) · Task (flex, min 240) · State 128 · Agent 120 · Branch 200 (mono 12) · Changes 88 (right) · PR 120 · Last activity 104 (right) |
| 760-999 | Id · Task · State · Changes · Last activity |
| 560-759 | Id · Task · State · Last activity |
| < 560 | card list: 56px rows: glyph + title (13/500) / id mono · state text · age (12 muted) |

- Age column removed; created time goes in the Last activity tooltip (NAV-08). Changes and PR columns are hidden when no visible row has a value (NAV-08). Last activity cell is `white-space: nowrap`, min 72 (NAV-07).
- Task cell: title 13/500 truncate + label chips after it (relation "part of T-0014" / "waits for T-0009", repo when several repos), never wrapping. Running activity shows in the Agent column as 12 muted text instead of a second line.
- Whole row is the click target (stretched button over the row, `cursor: pointer`), hover bg-hover, selected bg-selected (bg-selection-active when the table has focus) (RA-26, RA-28).
- Sorting: headers are buttons with `aria-sort` and a 12px arrow; click cycles asc/desc. Default sort is stable: state group (needs-input, review, failed, blocked, running, ready, draft, done, cancelled), then id descending. Live updates never reorder rows while the pointer is over the table or within 2s after a keyboard move; they apply on the next pause (fixes NAV-05).
- Keys: the table is one tab stop (roving on rows); Up/Down/j/k, Home/End, Enter opens the workspace, `i` toggles details, Shift+F10 row menu: Open workspace, Show details, Add subtask… (`ListPlus`, after Show details; only for a task with no parent), the actions for its state, copy items, Cancel task, Delete task.
- Overflow at intermediate widths: right-edge fade + visible thin scrollbar (NAV-03).

### 4.26 Board card (`features/tasks/TaskBoard.tsx`)
- Column 272 wide, full available height (`flex-1`, own scroll, NAV-21), gap 12, bg `color-mix(in srgb, var(--surface) 60%, transparent)`, radius 8, pad 8. Header 36: state chip + count badge + collapse chevron.
- Card: pad 10 12, radius 8, bg surface, 1px line, gap 6. Line 1: id mono 12 muted, quiet chip, age 12 muted right. Line 2: title 13/500, max 2 lines. Line 3: meta 12 muted (agent, `+12 −3`, PR #, waits for).
- Needs-you cards (needs-input, review, failed, blocked): 2px left border in the state color ("tint means look here"). Others neutral.
- Hover border line-strong; selected border accent + 1px accent ring; focus ring 2px.
- The state filter controls which columns show. Done and Cancelled are hidden behind a "Show finished" toggle in the view header. Empty non-finished columns show "Nothing here" 12 muted. Edge fades on horizontal overflow.
- < 768: one column at a time, a Segmented/scrolling chip row of states at the top.

### 4.27 Details panel sections (`features/tasks/TaskDetail.tsx`)
- Header 40: id mono 12 muted, then icon buttons 28: Edit (`Pencil`, `E`), More ("..."), Close (`X`, `Esc`). In page mode (< 768) Back (`ArrowLeft`) replaces Close.
- View tabs under the header (36): **Progress** and **Details**. Progress is the task progress of `design-task-stages.md` 7.5, in one column, so the user can watch the terminal and approve a stage without a tab change. The view the user picks is remembered for all tasks (`natamux.details-view`). Until then, the details beside an open workspace show Progress, and beside the list show Details. Edit always shows Details. The palette command "Show the task progress" and Send back open Progress. Opening Progress scrolls the current stage to the middle.
- Progress, a branch: a path not taken is one folded row (chevron, name, condition, **Not taken**, stage count). Its button has `aria-expanded` and `aria-controls`; it opens to the stage cards (`design-task-stages.md` 7.5).
- Progress, the current stage card while the stage needs the user (T-0115): needs-input or failed, and no check or agent reason in the reasons box: a box with the reason (amber `bg-signal-soft` for needs-input, red state tone for failed; "The agent asks: <question>", "The agent is waiting for your answer in its terminal.", "The agent ended its turn. Reply in the terminal, or resume it.", or the failure reason, else "The agent failed."). Actions: wide primary [Approve] (tooltip with the `a` key) or [Approve anyway] first, then a row: [Answer in terminal] (secondary xs; needs-input with a question, and only for the task of the open workspace: shows its Terminal tab and focuses the waiting shell; a sheet over the workspace closes first), [Resume] (ghost xs, busy while it runs), Start agent (needs-input and failed), [Mark as done], [Skip], [Send back]. Not on a phone this is the one place with the reason and these actions; the details are their own column, so a state change never resizes the shells.
- Progress, a stage card not reached yet: a ghost `xs` button **Skip** under its finish rule marks it to skip (`design-task-stages.md` 3.9). A marked card says **Will be skipped** in its chip, has a muted title, and the button reads **Don't skip**. No button on a task that is done or cancelled, or on the context stage.
- Title block pad 16: title 16/600 (wraps), chips row (state, priority, labels) gap 6.
- Action row: primary state action (`button primary`), secondary actions, agent action (Start agent / Restart agent / hidden while running, WS-17). Label above in caps "Actions" only in the narrow page mode.
- Banners (when relevant), pad 10 12, radius 6: quiet ("No sign of life for 12m. The agent may be stuck.") and waiting ("Waits for T-0009 to be done"), bg signal-soft, 1px signal-line.
- Sections, each a disclosure with a 28px header button (chevron 12, caps label, optional count badge, optional right action); open state remembered in localStorage per section: Notes (description; empty state "No notes. Add some with Edit.", NAV-11), Workspace (Branch with copy, Folder with copy, "Dev ports 20190-20199 (first is $PORT)", [View changes] [Remove workspace]) , Pull request, Subtasks, Tries (only when > 1), Details (repo, depends on, created, updated), Activity.
- Field rows: grid `96px 1fr`, gap 8, row min 24, label 12 muted, value 13; copy icon button 24 appears on row hover/focus.
- Edit mode: the title block and Notes become the form (no duplicate title, NAV-09); focus moves to the Title field; sticky footer 52 with [Cancel] [Save `Mod Enter`].
- "..." menu: Save as template / separator / Cancel task (danger, confirm) / Delete task (danger, confirm).
- Copy: "Run 2 more tries" (not "Try 2 more in parallel / Fan out"), "Keep this try, cancel the others" with confirm, "Tries of T-0002" (not `tries-t-0002`) (NAV-14). Fan-out lives in the "..." menu and is hidden for running tasks (WS-17).
- Activity: relative times with exact time tooltip; repeats grouped ("Details edited ×4 · You"); actor "You" / "Agent" / "Daemon" (NAV-12).

### 4.28 Status bar items (`features/app/StatusBar.tsx`)
- Item: height 24, pad 0 6, radius 6, gap 6, icon 14, text 12/16 muted; hover bg-hover + ink; clickable items are buttons with tooltips. Bar pad 0 6, gap 2.
- Left (workspace-wide, high priority): Live dot (6px add / signal) + "Live"/"Reconnecting" (tooltip explains); "3 running" (click: filter Running); "2 need you" in signal (click: jump palette scoped to needs-you).
- Right (lower priority, drop labels first): Projects (`FolderGit2`), Import issues (`Download`), Notifications (`Bell`; blocked state `BellOff` with tooltip "Notifications are blocked. Allow them in the browser's site settings."), Keyboard shortcuts (`Keyboard`, `?`), Account menu (`CircleUser`): Settings… (`Settings`, Mod+,, opens the Settings dialog), separator, Theme (System / Light / Dark), UI size (Compact / Default / Large), Keyboard shortcuts, Report an issue (`Bug`, opens a new GitHub issue in the browser), separator, Sign out (confirm dialog "Sign out of Natamux?" / "This removes the daemon token from this browser. Your tasks keep running."). The desktop app has no Sign out and no separator before it: its preload puts the token back on every load. Theme and sign out are no longer two 21px icons side by side (NAV-19).
- No shortcut hint text in the bar.

### 4.29 View header and workspace header
- View header 44, pad x 16, border-bottom line: title 15/600 + count badge (NAV-31), then the state filter (4.30), then right: Show finished (board only), Segmented Table/Board (`aria-label="Layout"`, options "Table", "Board", NAV-22).
- Workspace header 44, pad x 16: id mono 12 muted · title 16/600 truncate (wraps to 2 lines < 768) · state chip (needs-input, failed and review: a button, tooltip "Show in progress", that opens the details on Progress with the current stage in view, T-0115) · branch chip (mono 12, middle-truncated, copy on click, tooltip full name, WS-27) · right: [Push ↑2] secondary (label shows the commits that `origin/<branch>` does not have, and `↓N` when origin has commits the branch lacks; "Publish branch" when origin has no branch of this name; `aria-disabled` + tooltip with the reason when nothing to push, origin is ahead, no `origin` remote or no branch, WS-05), [Create PR] primary (`aria-disabled` + tooltip "Push first" when nothing is pushed/no commits; nowrap, RA-07), details toggle. Below 768 Push/PR move into a "..." menu.
- Git action results go to toasts, never a status line inside the header (fixes WS-09 layout shift). A done or cancelled task shows no Commit…, Push or Create PR: its finished banner offers the next git step. [Open pull request] stays.
- Needs-input banner (`features/workspaces/NeedsInputBanner.tsx`) when the task is needs-input or failed, on a phone only. Wider screens have the reason and its actions in the current stage card of the details (4.27, T-0115), and the header has no notice, so its height and the shells never change with the state. The review banner (`features/workspaces/ReviewBanner.tsx`, "Ready for review: <stages>", [Approve] [Send back]) is the same. On a phone both are a row under the header (the details are a page there and would hide the terminal): 40px, bg signal-soft, top+bottom 1px signal-line, `CircleQuestionMark` 16 signal, text "The agent asks: <question>" (from the latest needs-input event payload; fallback "The agent is waiting for your answer in its terminal."; an agent that ended its turn without `natamux done`: "The agent ended its turn. Reply in the terminal, or resume it."), right: [Answer in terminal] (focuses Shell 1) [Resume] (WS-08). On a phone it is the one place in the workspace with the reason, on every tab.
- Finished banner (`features/workspaces/FinishedBanner.tsx`) under the header of a done or cancelled task (40px; 200ms fade, none with reduced motion). The shells of a finished task end and the wrap-up takes their place, so this row does not resize a live shell. Tone of the state (`data-state` done or cancelled), `CircleCheck` or `Ban` 16. Text: **Task done.** / **Task cancelled.** and the next step, the first that matches (`lib/finished.ts`): files not committed → [Commit…]; commits not pushed → [Push ↑N]; commits and no pull request → [Create PR]; pull request open → [Open pull request]; else "Nothing is left in the workspace." → [Remove workspace]. [Remove workspace] is always there; with files not committed it is `aria-disabled` and "Commit or discard them first" shows beside it. While git is read: only the title and [Remove workspace] as a secondary button.
- Wrap-up (`features/workspaces/WrapUp.tsx`): the Terminal tab of a done or cancelled task. It opens no shell. Card "Wrap-up" (rows 44: mark 16, text, detail 12 muted, one xs action): changes, push, pull request (merged, open, draft, closed, none), branch (merged and deleted on removal, or kept with N commits). Card "Shells": shells still running (program · running, [Open] [Stop] that asks first), then the shells the task ended, with the agent, the stage that ran there last and the end time, and [Show last screen] (a dimmed monospace block, max 240px, the last 200 lines the daemon kept). [Open a new shell] shows the shell panes, with [Back to wrap-up] in the tab row.
- Add subtask dialog (`features/tasks/AddSubtaskDialog.tsx`, size sm), one for the sidebar row menu and the table row menu. Title "Add a subtask to T-1". One field, label "Title", focused. Footer [Cancel] [Add subtask]; [Add subtask] is disabled while the title is empty or the request runs; Enter and Mod+Enter submit. Success: the dialog closes, toast "Added T-5 to T-1", focus returns to the row. Error: the toast shows the daemon's message; the dialog stays open with the title.
- Remove workspace dialog (`features/workspaces/RemoveWorkspaceDialog.tsx`), one for the banner, the details and the row menu. Files not committed: title "N files are not committed", up to 5 paths with their letter, [Close] [View changes] [Commit…], no remove. Otherwise "Remove the workspace of T-1?": rows "Deletes the folder" + path; "Ends N shells. Nothing runs in them." or one warn row per program ("Stops node in Shell 7", button then reads [Stop it and remove]); "Stops the agent. T-1 stays running." for a task still at work; checkbox "Also delete the branch" (on and enabled only when the branch is merged, else off, disabled, with the reason); "Keeps the task, its notes, docs and history." Cancel has the focus. While the daemon works the dialog stays, the button shows a spinner and "Removing…", Escape and the scrim do nothing; an error shows inside as an alert.
- After removal a done or cancelled task shows a summary card (max 520) instead of "No workspace": glyph 32, "T-1 is done" / "T-1 was cancelled" (gets the focus when the old view had it), "Its workspace was removed. The task, its notes, docs and history stay.", pull request, branch ("Deleted" or "Kept: <name>"), finished (the time of the last event that moved the task), [Open pull request] [Show progress] [Start a follow-up task].

### 4.30 State filter (`features/tasks/StateFilter.tsx`)
- Main >= 900: chip buttons 24 high (dot + label + count, radius 6, `aria-pressed`). The row measures itself; chips that do not fit fold into a "+N" menu button (checkbox items). Needs-you states come first. Never clipped (NAV-04, RA-05).
- Main < 900: one "State" button (`ListFilter` + current value, e.g. "Running · 3") opening the same checkbox menu.
- The filter drives both table and board (NAV-21). Counts follow the active search (NAV-15).

### 4.31 Project settings (`features/projects/ProjectSettingsDialog.tsx`, `StageList.tsx`, `StageDetails.tsx`, `StagePicker.tsx`, `SkillPicker.tsx`)
- Dialog `xl`, title "Project settings of <project>". Tabs **Stages** (default), **Issues** and **JSON** (sticky under the header). All edit one draft: the text of `.natamux.json`. JSON that does not parse, or settings outside `stages` that break the rules, keep the JSON tab open with the error.
- Stages tab, top to bottom: pipeline preview (4.32); stage list (left, 352) and details (right) side by side from 1024; one column below. Below 640 the details (or the stage picker) take the place of the list, with **Back**.
- Issues tab: a fieldset "Open a GitHub issue for a task" with one native radio per `tracker.createIssue` value (label 14, the trigger in words; "Default" badge next to `manual`), disabled with the line "Issues are off: tracker.type is none (JSON tab)" (12 `--signal`) when there is no tracker. Under it, a list "Where the issue comes from" (12 muted): `gh issue create`, title, body.
- Footer: the line "New tasks use this pipeline. Tasks on their way keep their stages, but use the new prompts and approvals." (12 muted; not on the Issues tab), then **Reset to default** (ghost, asks, only changes the draft), **Import…** (ghost; hidden file input `.json`, max 1 MiB; reads the file, then asks once when the draft has edits or the file sets `check`, `command` or `args` on any stage, listing each as `<name> (<id>) · <field>: <value>`; only changes the draft), **Export** (ghost; downloads the draft as `<project>.natamux.json`; `aria-disabled` "Fix the JSON first" while the draft is not readable), the problem count (`role="status"`: errors `--danger-text`, warnings `--signal`), **Cancel**, **Save** (`Mod Enter`; `aria-disabled` with the reason while there are errors, or while nothing changed and the file is not broken).
- Stage row (`--row-h`): drag handle `GripVertical` 14 `--faint` (drag only), native checkbox with `role="switch"` named "<stage> on", a button with the kind icon 14 + name + marks (`UserRound` 12 waits for approval, `Pencil` 12 custom prompt; the name says them, for example "Spec, waits for approval, custom prompt, off"), and a "..." menu: Move up (`Alt+Up`), Move down (`Alt+Down`), Run alongside the stage above (agent and manual stages; disabled with the reason as hint) or Run on its own, separator, Remove (danger; a stage of your own asks first). A stage that is off is `--muted` with a line through. Selected: `bg-selected`. Problems of a stage sit under its row, 12, with `CircleAlert`.
- A group is one block (radius 8, 1px line, `bg-raised`, pad 4) with a caps header (its name, "side by side", drag handle for the whole group), then its rows.
- Drag and drop: native HTML5 drag events. A 2px `--accent` line shows where the row lands. Dropped on a row of a group, an agent or manual stage joins the group when it has room.
- Kind icons: context `BookOpen`, agent `Bot`, review `Eye`, testing `FlaskConical`, release `Rocket`, manual `Hand`.
- Details: header (kind icon 16, name 15/600, kind, id in mono, catalog sentence), then fields as fieldsets with label 12/500: Name; Wait for my approval (checkbox; "Open the pull request when I click" for release; checked and disabled with "This stage is a check by a person." for review and manual); Tasks can run it as a workflow (agent; checkbox, with Default or Changed against the catalog, and a muted note); Prompt (agent, testing; monospace textarea; chips `{title}` `{description}` `{dir}` `{ref}` and **Insert skill** above; the fixed line "When you are done, run `natamux done`, unless the steps above say not to." below); Agent and Arguments (stages that take their own agent; Arguments also on any stage with `args`); Command (testing, and any stage with `command`; elsewhere the help says the stage does not use it); Check command (every stage; mono); Output file (agent; read-only for catalog stages); Required sections (one per line); Finished when. A value that can differ from its default shows a "Default" badge (`bg-badge`, muted), or "Changed" (`bg-signal-soft`, `--signal`) and **Use default** (ghost xs).
- A branch is one block like a group, with a caps header (its name, "branch of <n> paths") and one sub-list per path ("<path>, stages"), each stage a normal row. The list header has **Add branch** next to **Add stage**. Its "..." menu has **Remove branch** (asks first). Stages move into a path, between paths and out of the branch with Move up and Move down; context, release, groups and branches never enter a path.
- Branch details (`features/stages/editor/BranchDetails.tsx`): region "Settings of <name>", Branch name, then an ordered list "Paths, checked in order" with one card per path (radius 8, 1px `--line`, `bg-raised`, pad 12): Path name; **If** with selects "Condition type" (**File has text**, **Command succeeds**) and "Condition holds" (**holds**, **does not hold**); the fields of the type (File, Section (optional), Text to look for, or Command; errors under the field, 12 `--danger-text`); **Make this the Otherwise path** (last path only); the stages of the path with "Add a stage to <path>"; **Remove path** (ghost xs; asks when the path has stages; disabled at 1 path). **Add path** is hidden at 4 paths.
- Stage picker (Add stage): replaces the details. Search field "Search stages", catalog stages not in the pipeline by phase (caps titles), each a row with name, sentence, kind and default approval; **Custom stage** opens a form (Name, Id from the name, Kind, Output file for agent) with errors under the fields.
- Skill picker (Insert skill): a box under the chips (`bg-popover`, `--shadow-1`). Search, the daemon's notices, groups **Project**, **Plugins**, **Only on this machine**; each skill: name in mono, description, the agents that have it, warnings in `--signal`. Escape closes it first, then the screen.
- Closing with changes asks "Discard your changes?". A file that changed on disk shows an alert with **Reload** (drops the edits) and **Overwrite** (danger: reads the file again and saves the keys the person changed over it; the other keys keep the new file's values).

### 4.32 Pipeline preview (`features/stages/editor/PipelinePreview.tsx`, `.pipeline*` in `styles.css`)
- The pipeline of the draft, not the saved one. Stages that are off are not drawn. Box: radius 8, 1px line, `bg-raised`, pad 12 16; scrolls sideways when wide.
- Node (104 wide, radius 8): the "not reached" marker of `StageMarker` (24, `.stage-node[data-status="upcoming"]`) with the kind icon inside, the name under it (12, one line, ellipsis). Marks at the marker's right corners on a 14 circle (`--popover`, 1px `--line-strong`): `UserRound` top (waits for a person), `Pencil` bottom (custom prompt). Hover `--hover`, selected `--selected`. The tooltip says kind, approval, output file and command.
- Lines: 2px dashed `--muted`, from marker to marker. A group forks into one lane per stage (lanes 4 apart) and joins before the next step.
- A step with an error: the marker gets a `--danger-text` ring, the name turns `--danger-text`; a group with an error gets red fork lines.
- A branch is a diamond **If** node (`.pipeline-if`, `.pipeline-diamond`), then a fork (`.pipeline-fork`) with one lane per path (`.pipeline-path`) and a join. Each lane has a condition label (`.pipeline-when`: path name, then `if <file> has "<text>"`, `NOT` for a turned-around condition, or Otherwise; the tooltip has the full text) and its stages in a row. Without an Otherwise path, a dashed line (`.pipeline-skip`) says "No path holds → skip to <next>". A path with no stage that is on is not drawn.
- Accessible (branch): the **If** node is a button "Branch <name>, one path of <n>"; the fork is a list "<name>, <n> paths, checked in order" with a list "<path>, stages" per lane; under the chart a list "Paths of <name>" says each path, its condition and its stages.
- Below 640: top to bottom. Nodes are rows (marker, then name); a group branches off the main line, one row per stage. A branch is an "If · <name>" row, then one bordered card per path with its condition.
- Still: no motion, so it reads the same with reduced motion. Light and dark through tokens only.
- Accessible: an `ol` named "Pipeline: Context, then Spec, …, then Test spec and Implement side by side, …"; a group is a nested list "<names>, side by side"; each node is a button named like the stage row, and a click selects the stage.

### 4.33 Files explorer and editor tab (`features/files/FileTree.tsx`, `FileEditor.tsx`, `MonacoFile.tsx`, `TerminalView.tsx`)
- Place: the **Files** tab of the details (`Progress | Files | Details`), only for a task with a workspace. In the details, not in a workspace tab, so the terminal stays in sight for a drag. A stored `files` pick shows **Progress** for a task without a workspace.
- Header (32, like the Changes list): the hint "Drag a file onto a terminal to type its path." and **Refresh** (xs icon button).
- Tree: rows of `--row-h`, the row recipe of the Changes tree (4.24): indent 12 per level, chevron for a folder, `FileIcon` for a file, the name, then `edited` (2xs, `--signal`) for an unsaved draft, then the status letter in its tone. A folder with changes below gets the 6 px dot in `--modified`. Folders first, then files, each by name with numbers in order. One request per folder, when it opens; open folders are kept per task while the page lives. `workspace.dirty` reads the open folders again.
- Keys: the roving tree of the Changes list: `Up`/`Down`/`Home`/`End`, `Right` opens or goes to the first child, `Left` closes or goes to the parent, `Enter` or `Space` opens a file or toggles a folder, type-ahead. `role="tree"` "Files of the worktree", rows `treeitem` with level, position and `aria-expanded`; the open file is `aria-selected`.
- Drag: a row puts `{ path, kind }` under the type `application/x-natamux-path-<task id>` and the path as `text/plain`. A terminal of the same task (`TerminalView`, open shell only) shows a 2 px `--focus` outline inside while the drag is over it, and on the drop pastes the path from the worktree root (folder with `/`, quoted in `'…'` when it has other characters than `A-Z a-z 0-9 . _ / @ + -`), then a space, with xterm `paste` (bracketed paste), no Enter, and takes the focus. Another task's drag keeps the browser's "not allowed".
- Editor tab: `editor` after **Docs**, only while a file is open, label = file name, icon `FileCode`, palette command **Show the open file** (no key: `5` showed Docs before). Address `/w/<id>/editor?file=<path>`; without a file the address opens the terminal. Closing goes back to the tab picked last.
- Editor header (40): `FileIcon`, the path (`PathText`), `Modified` (2xs, `--signal`) while there is a draft, **Revert** (ghost, asks first), **Save** (primary, `Mod+S`, off without a draft or while the banner shows), **Close the file** (x; asks first when there is a draft).
- Body: `MonacoFile` with `kind="edit"`: the options of the file view, editable, no whole-file marks, the last line break kept, wrap for prose files. Notices instead: "Binary file. Not shown.", "Larger than 1 MB. Not shown.".
- Drafts: an edit is kept per task and path while the page lives (`features/files/file-drafts.ts`), with the version it began on. `beforeunload` asks while one exists. A new text on disk goes into the editor in place when there is no draft.
- Banner (under the header, `role="alert"`, xs): "This file changed on disk." with **Reload** (drop the draft) and **Keep mine** (the draft takes the new version; the next save replaces the file). It shows when the draft's version is not the one on disk, also after a save answered 409.

### 4.34 Settings dialog (`features/app/SettingsDialog.tsx`)
- Opened by: the OS menu item **Settings…** of the desktop app (macOS app menu, File menu on Windows and Linux, `CmdOrCtrl+,`), the Account menu, and Mod+, (`app.settings`).
- `Dialog` size `md`, title "Settings", no footer: every change applies at once.
- Sections (xs muted heading, rows of 28 with the label left and the control right):
  - **Appearance**: Theme (`Segmented` System / Light / Dark), UI size (`Segmented`), **Terminal font…** (secondary button; the font dialog takes the place of this one).
  - **Desktop app**, only when the preload gives `getDesktopSettings`: checkboxes **Keep computer awake while agents run** and **Check for updates automatically** (hidden when the app cannot update itself). A failed save puts the tick back and shows an error toast.
  - **Projects**: one row per project with an xs ghost **Settings** button (opens 4.31). No project: "No projects yet" and **Add a project** (opens the projects panel).

---------------------------------------------------------------------

## 5. Interaction

### 5.1 Keyboard map

Rules:
- Chords use `Mod`. Single keys work only when focus is not in a text field, a terminal or Monaco.
- Inside a terminal on Windows, Ctrl+letter belongs to the shell (Ctrl+K kill line, Ctrl+B back, Ctrl+J newline). There, only these reach the app: F1 (palette), F6 / Shift+F6 (leave the terminal), Ctrl+Shift+chords listed below. On macOS, Cmd chords never reach the shell, so Mod+K/J/B work everywhere. xterm gets `attachCustomKeyEventHandler` that returns false for these keys.
- Every shortcut is listed in `lib/commands/` and shown in tooltips, menus, the palette and the `?` sheet.

| Group | Key | Action |
|---|---|---|
| Global | Mod+K, F1 | Command palette |
| | Mod+, | Settings dialog. The desktop app's OS menu item has the same accelerator |
| | Mod+J | Jump palette. Inside a terminal on Windows, Ctrl+J is the shell's and Ctrl+Shift+J opens Chrome DevTools, so use F1 there (the palette's empty state lists needs-you and recent workspaces first) |
| | Mod+B | Toggle sidebar (drawer on small screens) |
| | Mod+Alt+B | Toggle details |
| | F6 / Shift+F6 | Focus next / previous part: sidebar, main, details, status bar |
| | Mod+Enter | Submit the current form (dialog, commit, comment, edit) |
| | Mod+, | Settings (later; until then the account menu) |
| | Esc | Close the top layer: tooltip > menu > palette > dialog > sheet/drawer. Else in a field: clear, then blur. Else in a workspace: back to All tasks |
| Single keys | `?` | Shortcut sheet |
| | `/` | Focus the sidebar filter |
| | `c` | Start a task |
| | `n` | New task (focus the backlog field) |
| | `j` / `k`, Down / Up | Move selection in the focused list |
| | Enter | Open the selected task (workspace, or the no-workspace view) |
| | `i` | Show / hide details of the selected task |
| | `e` | Edit the selected task in details |
| | `a` / `r` | Approve / Request changes (review state only; existing) |
| | `1` `2` `3` `4` | Workspace tabs: Terminal, Changes, History, Docs |
| | `[` / `]` | Previous / next changed file (Changes tab) |
| | `R` (Shift+r) | Refresh changes / history |
| Lists and trees | Home / End | First / last |
| | Left / Right | Collapse / expand group (tree) |
| | F2 | Rename workspace |
| | Shift+F10, ContextMenu | Row menu |
| | Delete | Remove workspace (asks) |
| | Alt+Up / Alt+Down | Move the focused stage in the stage list of Project settings |
| | type letters | Jump to a row by title |
| Panes | Mod+Shift+Enter | Maximize / restore the focused pane |
| | Ctrl+Shift+` | New shell (works inside a terminal on both OSes) |
| | Mod+F (in a pane, P2, needs `@xterm/addon-search`) | Find in scrollback |
| Diff | `[` `]` | Previous / next file |
| | Alt+F5 / Shift+Alt+F5 | Next / previous change in the file |
| | F7 / Shift+F7 (Monaco built-in) | Accessible diff view |
| | Mod+Shift+A | Comment on the current line for the agent |
| | Mod+Alt+Z | Roll back the shown file |
| Palette | Up/Down, Enter, Shift+Enter, Alt+1..9, Tab, Esc | see 4.17 |

`b`, `g`, `p`, `x` are left free on purpose for later (board drag, go-to sequences, pin).

### 5.2 Focus rules
- One visible focus style: 2px `--focus` outline, offset 1px on buttons/controls; inset (`-2px`) on rows, cells and anything inside a scroll container that clips. Inputs use the 1px border+outline in 4.3.
- Tab order follows the visual order: title bar, sidebar (filter, nav, tree as one stop), main, details, status bar. A "Skip to main content" link is the first tab stop (visible on focus, top-left, `bg-popover`, `--shadow-1`) (RA-23).
- Each toolbar, tab list, segmented control, list and tree is ONE tab stop; arrows move inside (roving tabindex).
- Opening a layer moves focus into it (first field, first menu item, palette input). Closing returns focus to the opener. If the opener is gone (row removed), focus goes to the list container.
- After create: focus goes where the new thing is (started task: Shell 1 terminal; added task: its row; subtask: stays in the subtask field).
- Selection in a list follows focus. The workspace open in main is "current" (aria-current) and is separate from selection.
- Terminal keeps all keys except those in 5.1; F6 always gets out.

### 5.3 Hover, active, selected
- Hover: bg-hover (5% / 4%), 120ms. Ghost controls also change text muted to ink. No movement, no shadow, no underline except links and link-style task titles.
- Pressed: bg-active (8% / 7%), 80ms.
- Selected, list not focused: bg-selected. Selected, list focused: bg-selection-active. Current workspace: bg-selected + inset line-strong hairline.
- `cursor: pointer` on every enabled button, link, row, chip and tab (one base rule; Tailwind preflight sets `default`, RA-27). `cursor: not-allowed` on `aria-disabled`.
- Hover-revealed controls must also appear on `:focus-within` and on coarse pointers, and must not shift layout (they swap with existing meta).

### 5.4 What opens where

| Action | Result |
|---|---|
| Click a workspace row (sidebar, jump palette) | Opens the workspace in main. Tab: Terminal when running/needs-input; otherwise the last tab used for THAT workspace; Terminal for a new one (WS-18). Details keep their per-task open state; never opened or closed, they open where they dock (see Details in 3.1). |
| Click a table row / board card | Selects it and opens details (dock or sheet by 3.2). |
| Double-click a row / Enter | Opens the workspace; for a task without one, the No-workspace view in main with [Create workspace] and [Start agent] (WS-23). |
| Shift+Enter in palette, "Add to Agents" in row menu | Adds the task's Shell 1 to the Agents grid. |
| Agents pane title | Opens that workspace (Terminal tab). |
| Start a task (dialog) or Create workspace (details) | Closes the dialog, opens the new workspace on Terminal, toast "T-0021 is ready in natamux/t-0021-..." with [Open] when the user has moved on (WS-23). |
| PR link | New browser tab (`ExternalLink` icon 12 after the text). |
| System notification click | Focuses the window and opens that workspace. |
| Destructive actions (remove workspace, remove finished workspaces, cancel task, delete task, close a busy shell, keep one try, sign out) | Confirm dialog that says what is deleted and what is kept; Cancel focused. |

### 5.5 Deep links (WS-22)
Routes (history API, no router package, `lib/route.ts`): `/` all tasks, `/board`, `/agents`, `/t/T-0018` (details open), `/w/T-0018/terminal|changes|history`, `/w/T-0018/changes?file=src/a.ts`. Back/forward work; reload restores the view; the daemon serves `index.html` for these paths.

### 5.6 Live updates without jumps
- Rows never reorder under the pointer or right after keyboard moves (4.25).
- Reserved space for anything that appears later (activity line uses a fixed 16px slot only for running rows; banners push content; not on a phone the reason and actions of a waiting stage sit in the stage card of the details, a column of its own, so a state change never resizes the shells).
- Counts animate nothing; they just change.

### 5.7 Time format (WS-32)
One helper in `lib/tasks.ts`: `formatAge(date, now, 'short' | 'long')`. Short (rows, tables): `now`, `4m`, `3h`, `2d`, then `Oct 3`. Long (sentences, details): `just now`, `4m ago`, `3h ago`, `2d ago`, `Oct 3`. Exact local date-time in a tooltip everywhere.

---------------------------------------------------------------------

## 6. Accessibility rules

1. Contrast: all text >= 4.5:1, large text (>= 18.66px bold / 24px) >= 3:1, focus ring and state glyphs >= 3:1 against their background. Every pair in section 2.3 lists its ratio; any new color needs one.
2. Never color alone: every state has a glyph shape and a text label (sr-only in dense places, tooltip on hover). Diff lines have `+`/`-` gutter markers. Needs-you has text ("2 need you"), not only amber.
3. Keyboard: everything reachable and operable; no keyboard trap except modal dialogs (which trap on purpose and close with Esc); terminal has F6 out.
4. Focus visible everywhere (5.2). Never `outline: none` without a replacement.
5. Landmarks: `header` (title bar), `nav` (sidebar), `main`, `aside` (details), `footer` (status bar). One `h1` (the current view title, visually the view header), `h2` for sections.
6. Names: every icon-only control has `aria-label`; toggles use `aria-pressed`; disclosures `aria-expanded`; current workspace `aria-current="true"`; sorted column `aria-sort`; tabs/radios/listbox/menu use their ARIA roles; segmented options say the option, not the state.
7. Live regions: toasts (`status`/`alert`), needs-you count changes announced politely ("T-0008 needs input"), connection state. Do not announce every terminal line.
8. Modals: `aria-modal`, `inert` on the rest, focus trap, return focus.
9. Targets: >= 24x24 everywhere (WCAG 2.2 target size minimum), >= 40 on coarse pointers.
10. Zoom and text size: layouts survive 200% browser zoom and the UI size setting; no fixed px heights on text containers except the listed chrome heights.
11. Motion: respect `prefers-reduced-motion`; nothing flashes more than 3 times per second.
12. `prefers-contrast: more`: turn on `--contrast-border`, use `--muted` for control borders.
13. `prefers-color-scheme`: first visit follows the OS; the user choice (System / Light / Dark) is stored.
14. Language: `lang="en"`; plain words; no emoji as content.
15. Screen reader names for rows read as one sentence: "Fix token refresh race, Needs input, T-0008, 34 minutes".
16. Terminal: xterm `screenReaderMode` follows a setting (off by default); Monaco diff keeps its accessible diff viewer (F7).

---------------------------------------------------------------------

## 7. Gap analysis

### 7.1 Where Natamux will be better

| Area | Orca | VS Code | Natamux target |
|---|---|---|---|
| Who needs me | `?` glyph on the row, jump palette ranks needs-you first | not a concept | Needs-you count in title bar + status bar + document title + notifications, attention cards in the tree, AND the agent's question as text in a banner with Answer / Resume (WS-08) |
| Task model | worktrees with optional linked issues | none | tasks are first class: 9 states, table and board, dependencies, subtasks, tries; workspace is one view of a task |
| Stability | live lists | static lists | stable sort, frozen palette order, no reorder under the pointer, no layout shift (5.6) |
| Responsive | desktop only; status bar condenses; mobile is a separate app | shrinks parts to minimums, then clips and scrolls the window under ~800px | one app from 390 to 2560 with drawer, sheet and page modes, container-query tables, stacked panes (3.2) |
| Contrast | contrast treated as a requirement in places | WCAG for webviews; HC themes | every text pair measured and >= 4.5 in both themes; `prefers-contrast` support; glyph shapes per state |
| Keyboard | rich keymap, remappable | the reference | VS Code-grade model (one tab stop per widget, F6 parts, palette has every command) plus single-key task actions, and an explicit terminal-safe key policy for Windows |
| Light theme | supported (source only) | first class | first class; terminal and diff follow it |
| Plain words | good STYLEGUIDE | good | written copy rules and fixed copy for every empty/error state (4.19) |

### 7.2 What Natamux must still match

| Area | Reference behavior | Where in this spec |
|---|---|---|
| Density and type | 11-13px working sizes, weight for hierarchy | 2.2 |
| Quiet chrome | neutral surfaces, color for state | 1, 2.3 |
| Command palette | top-center, ~600px, kbd chips, groups, recents, create fallback | 4.17 |
| Jump palette | needs-you first, frozen order, digit jump, create row | 4.17 |
| Panes | splits, maximize, sizes saved per workspace, each workspace owns its layout | 4.23; per-workspace pane layout saved in `view-store` (P1) |
| Terminal quality | themed palette, min contrast, scrollback search, links, scroll to bottom | 2.3, 4.23 (search and links need two xterm addons; ask owner) |
| Diff quality | themed Monaco, hide unchanged, wrap, hunk nav, per-line comments to the agent | 4.24 |
| Create flow | short dialog, Mod+Enter, closes at once, progress in context | 4.14, 5.4 |
| Status bar | information only, measured condensation | 4.28, 3.2 |
| Tooltips | named icon buttons with shortcuts, consistent delay | 4.16 |
| Floating surfaces | one recipe, three elevation levels | 2.6 |
| Settings | theme, UI size, density later | 4.28 account menu; full settings page later |

Not matched on purpose (for now): embedded browser, several editor tabs, image diff, SSH, plugins, usage meters (no data source yet). The files explorer opens one file at a time (4.33).

---------------------------------------------------------------------

## 8. Backlog

Five PRs, in order. Each PR is pushed once when complete (CI minutes are short, see memory note). Each item: id, priority, audit ids, files, exact change. Paths are under `packages/web/src/` unless they start with `packages/`.

No new npm package is needed for PR 1-5. Two optional items need the owner's OK first: `@xterm/addon-search` (find in terminal) and `@xterm/addon-web-links` (clickable links) — both P2, item W-19.

### PR 1 — Foundation: tokens, theme, primitives

| Id | Pri | Fixes | Files | Change |
|---|---|---|---|---|
| F-01 | P0 | RA-10, RA-11, RA-12, RA-14, RA-15, RA-16, NAV-30 | `styles.css` | Replace `:root` and `[data-theme="dark"]` values with section 2.3 (surfaces, text, lines, states, signal, diff, tooltip, scroll, scrim, skeleton). Add new variables (`--shell`, `--popover`, `--input`, `--ink-2`, `--placeholder`, `--faint`, `--link`, `--control`, `--control-hover`, `--contrast-border`, `--active`, `--selection-active`, `--focus`, `--accent-hover`, `--accent-fg`, `--danger-text`, `--signal-line`, `--add-bg`, `--remove-bg`, `--add-word`, `--remove-word`, `--modified`, `--badge`, `--tooltip-bg`, `--tooltip-fg`, `--skeleton`, `--scrim`, `--shadow-1`, `--shadow-2`, `--z-*`, `--dur-*`, `--ease-*`) and their `--color-*` mappings in `@theme inline`. Replace all 9 state chip pairs per theme. Add `@media (prefers-contrast: more)` block. |
| F-02 | P1 | RA-19, RA-20, RA-22, RA-18 (part) | `styles.css`, components using `gap-[0.4rem]`/`0.4rem` (`.state-chip`) | Add the type scale (`--text-2xs` ... `--text-xl` with line heights) and `--breakpoint-2xl: 90rem`; body uses `--text-sm`; `.state-chip` gap 6px; `.lucide { stroke-width: 1.75 }`; `html { font-size: calc(16px * var(--ui-zoom, 1)) }`. Delete the `running-pulse` keyframes and their uses (chips and dots stop pulsing; the sidebar glyph spinner is the only running motion). |
| F-03 | P0 | WS-01, RA-17 | `lib/theme.ts`, `features/files/monaco-setup.ts`, `features/git/MonacoDiff.tsx`, `main.tsx` | Theme becomes `'system' \| 'light' \| 'dark'`; `resolvedTheme()` uses `prefers-color-scheme` only for `system`; first visit = system. Export `useResolvedTheme()` (zustand or `useSyncExternalStore` on a `MutationObserver` of `data-theme`). MonacoDiff picks `natamux-dark/light` from the resolved APP theme and re-themes on change (today it reads the OS scheme, which is why the diff is white in a dark app). Define both Monaco themes with the colors in 2.3. |
| F-04 | P1 | WS-11, RA-17 | `features/terminals/TerminalView.tsx`, new `features/terminals/terminal-theme.ts` | Full ANSI `ITheme` per theme from 2.3, `minimumContrastRatio: 4.5`, `lineHeight: 1.3`, font size `13 * zoom`; re-apply `terminal.options.theme` when the resolved theme changes. |
| F-05 | P0 | NAV-01, RA-25, NAV-29 (close button) | `components/ui/Dialog.tsx` | Focus trap (Tab/Shift+Tab wrap over focusable elements), `inert` on `#root` while open (dialog portals to `body`), sizes `sm/md/lg` (400/520/640), radius 12, `--shadow-2`, scrim token, close button 28 with 16 icon, footer kbd on primary, Mod+Enter submits (`onSubmit` prop), destructive `ConfirmDialog` focuses Cancel, bottom sheet under 768. |
| F-06 | P1 | RA-24, WS-26, NAV-22 | `styles.css`, `features/app/Dashboard.tsx` (search label), `ui/styles.ts` | One focus style: `:focus-visible { outline: 2px solid var(--focus); outline-offset: 1px }`; `.focus-inset` utility (`outline-offset: -2px`); remove the `input:focus-visible` box-shadow rule; field wrappers use `:focus-within` border+outline; inner inputs `outline: none`. Table/Board toggle gets names "Table" / "Board" (moved to Segmented in F-10). |
| F-07 | P1 | RA-21, NAV-10, RA-13 | `ui/styles.ts`, `ui/Select.tsx`, new `ui/Button.tsx`, `ui/IconButton.tsx`, `features/workspaces/ChangesView.tsx`, `TerminalView.tsx`, `TaskEditForm.tsx`, `Dashboard.tsx` | Button sizes xs/sm/md = 24/28/32, variants incl. `link`; `IconButton` (28/24, required `label`, optional `shortcut`, sets tooltip attributes, `aria-pressed` support); `aria-disabled` + `disabledReason` prop. Select sizes 28/32 and used everywhere (replace the native-styled priority select in `TaskEditForm`). Remove ad-hoc `toolButton`, `primaryButton`, `footerButton`, local `iconButton`, `navButton` strings. Add task button: `aria-disabled` with reason "Type a title first". |
| F-08 | P1 | RA-32, WS-27, NAV-19 (tooltip), WS-12 (tooltip) | new `ui/Tooltip.tsx`, `Dashboard.tsx` (mount), every component with `title=` (`WorkspaceList`, `WorkspaceRow`, `Menu`, `TerminalPane`, `AgentPanes`, `ChangesView`, `TaskTable`, `TaskDetail`, `StatusGlyph`, `MonacoDiff`, `TerminalView`, `Dashboard`) | Global delegated tooltip layer per 4.16 (`data-tip`, `data-tip-kbd`, `data-tip-side`, `data-tip-overflow`). Replace all `title=` attributes (keep `title` only on `<abbr>`/iframes if any). |
| F-09 | P1 | — (base for all shortcut display) | new `lib/keys.ts`, new `ui/Kbd.tsx`, `ui/styles.ts` (`kbd`) | `isMac()`, `formatShortcut('Mod+Shift+Enter')` -> `⇧⌘↵` / `Ctrl+Shift+Enter`; Kbd component per 4.11. |
| F-10 | P1 | NAV-22, WS-03 (control part) | new `ui/Segmented.tsx`, new `ui/Tabs.tsx` | Components per 4.6 and 4.7 with roving keys. |
| F-11 | P1 | NAV-13 | `ui/Menu.tsx` | Item union with `separator` and `label`; `shortcut` column; flip-above positioning that never covers the trigger; Home/End/type-ahead; Up-arrow opens at last item; Tab closes; export `openContextMenu(event, items)` for right-click and Shift+F10. |
| F-12 | P1 | NAV-20 | `ui/Toaster.tsx`, `lib/toasts.ts` | Position 16 right / 40 bottom, width 360, max 3, actions array, pause on hover/focus, error-with-action persists, error 10s, info 5s, icon 16, `--shadow-1`, full-width under 768. |
| F-13 | P1 | RA-29 (part), RA-30 (base) | new `ui/Skeleton.tsx`, new `ui/EmptyState.tsx` | Components per 4.19 and 4.20 with 200ms delay / 300ms minimum hook `useDelayedFlag`. |
| F-14 | P2 | RA-27 | `styles.css` | `button:not(:disabled), [role="tab"], [role="menuitem"], a, summary { cursor: pointer }`; `[aria-disabled="true"] { cursor: not-allowed }`. |
| F-15 | P1 | WS-33, WS-35 | `components/StatusGlyph.tsx` | Glyph map per 4.10 (needs-input `CircleQuestionMark`, blocked `CirclePause`, failed `CircleX`, cancelled `CircleSlash`), reduced-motion glyph `CircleEllipsis`, draft uses `--chip-fg` (now 7.2:1). Label via tooltip attributes, not `title`. |

### PR 2 — App shell and responsive layout

| Id | Pri | Fixes | Files | Change |
|---|---|---|---|---|
| S-01 | P0 | NAV-02, RA-01, WS-07 | new `hooks/useLayoutMode.ts`, `features/app/Dashboard.tsx` (split into new `features/app/AppShell.tsx`), `lib/view-store/` | Shell per 3.1. `useLayoutMode` with 768/1024/1280 and the "details dock only if main >= 640" rule. Sidebar modes docked / drawer (300, scrim, focus trap while open, closes on navigation). View store gets `sidebarOpen` (persisted) and `drawerOpen`. |
| S-02 | P0 | RA-02, WS-16 | `features/tasks/TaskDetail.tsx`, `AppShell.tsx`, `lib/view-store/` | Details modes docked / sheet / page (3.2). Sheet: fixed right, `--shadow-2`, slide 200ms, Esc closes, focus moves in and back. Page (< 768): replaces main with Back. Open state per task id: `detailOpenByTask: Record<string, boolean>` persisted; default closed for workspaces, open for table selection. Auto-close when the Changes tab opens and main < 900. |
| S-03 | P0 | RA-03 | `features/terminals/AgentPanes.tsx`, `features/terminals/ShellPanes.tsx`, `lib/panes.ts` | Grid `repeat(auto-fill, minmax(320px, 1fr))` when layout is Auto; < 768 one column with `min-height: 60vh` per pane (agents) or a pane switcher (shells). |
| S-04 | P1 | (enables principle 1), NAV-19 | new `features/app/TitleBar.tsx`, `AppShell.tsx` | Title bar per 3.1: sidebar toggle, wordmark, breadcrumb, command center button (until PR 5 it focuses the filter field), needs-you pill, Start a task, details toggle; Electron gutters via `data-shell`/`data-platform` attributes (set nothing in the browser); `data-window` inactive dimming hook. Move "Natamux" and "Start a task" out of the sidebar header. |
| S-05 | P1 | NAV-19, RA-04, WS-34 | new `features/app/StatusBar.tsx`, `Dashboard.tsx` (remove footer) | Status bar per 4.28: 28px, priority-based condensation (ResizeObserver) with "..." overflow menu, never wraps; remove the hint text; Keyboard button; account menu with Theme (System/Light/Dark), UI size, Sign out with confirm (browser only, hidden in the desktop app); blocked notifications tooltip. Hidden < 768 (items go to the drawer footer). |
| S-06 | P1 | NAV-16 | `Dashboard.tsx` / new `features/sidebar/Sidebar.tsx`, `WorkspaceList.tsx` | Sidebar structure per 3.1: only the tree scrolls; "Workspaces" header sticky; 12px bottom padding. Filter field uses `SearchField`. |
| S-07 | P1 | RA-09 | `styles.css`, `ui/styles.ts` | `@media (pointer: coarse)`: `--control-h: 40px`, rows 48, workspace row 56, icon hit 40; hover-only controls visible; resize handles hidden. Control classes read `--control-h`. |
| S-08 | P1 | RA-23 | `AppShell.tsx`, `styles.css` | "Skip to main content" link; landmarks (`header`, `nav`, `main#main`, `aside`, `footer`); F6 / Shift+F6 part cycling (also from xterm via custom key handler). |
| S-09 | P1 | RA-18 | `lib/theme.ts` (or new `lib/ui-size.ts`), account menu | UI size setting Compact/Default/Large -> `--ui-zoom` 0.92/1/1.077 on `html`, stored `natamux.ui-size`; xterm and Monaco font sizes follow. |
| S-10 | P2 | RA-08 | `TaskTable.tsx` | At >= 1920 the Task column absorbs extra width (`width: 100%` on that column only). |

### PR 3 — Workspace: header, terminal, diff, agents

| Id | Pri | Fixes | Files | Change |
|---|---|---|---|---|
| W-01 | P0 | WS-02 | `features/workspaces/ChangesView.tsx`, `MonacoDiff.tsx` | For status A/U/D: render a read-only single editor with whole-file add/remove decorations and a "New file"/"Deleted" badge, not a diff against an empty string. `lightbulb` off. Trim the trailing empty line artifact. |
| W-02 | P0 | WS-03, WS-04, WS-15, WS-25 (diff part) | `ChangesView.tsx`, `MonacoDiff.tsx` | Diff chrome per 4.24: resizable file panel (`useStoredWidth('natamux.files-width', 260, 200, 420)`), file picker under 720, diff toolbar with Segmented [Side by side \| Inline] (labels are options; Side by side disabled under 720 with reason), Wrap toggle, prev/next file; Monaco `flex-1 min-h-0` (drop `h-[70vh]`), options per 4.24; comment box docked at the bottom; commit box docked at the bottom of the file panel. |
| W-03 | P0 | WS-05, RA-07, WS-27 | `ChangesView.tsx`, `lib/api/` (if ahead count is not on `Task`), `packages/daemon/src/git/` + `server.ts` (only if needed) | Workspace header per 4.29: 44px single row, branch chip with middle truncation + copy + tooltip; Push shows ahead count and is `aria-disabled` with reason when 0 ahead / no branch; Create PR `aria-disabled` with "Push first" when nothing is pushed, nowrap, "Open PR" under 900 main, "..." menu under 768. If the ahead count is not available, add `ahead` to the diff-stats or changes API (daemon `git rev-list --count base..branch`). |
| W-04 | P0 | WS-06 | `packages/daemon/src/terminals/terminal-service.ts`, `packages/daemon/src/terminals/__tests__/terminal-service.test.ts`, `features/terminals/ShellPanes.tsx` | `closeOne` removes the session from the map at once and kills the PTY (SIGHUP, then SIGKILL after 2s if still alive); scrollback dropped; the next "New shell" always starts a fresh process (verify the reuse path with a test: close "2", open new -> new pid, empty scrollback). UI: confirm when a foreground process runs (P2 if the daemon cannot tell). |
| W-05 | P0 | WS-08 | new `features/workspaces/NeedsInputBanner.tsx`, `ChangesView.tsx`, `features/tasks/task-events.ts` | Banner per 4.29 under the workspace header for needs-input tasks; question text from the latest needs-input event (if the payload has none, the fallback sentence); [Answer in terminal] switches to Terminal and focuses Shell 1; [Resume] sends `agent.resumed`. |
| W-06 | P1 | WS-09 | `ChangesView.tsx` | Remove the in-header notice line; results and errors of commit/push/PR go to toasts (errors with [Show details]). Busy state stays in the button (spinner + locked width). |
| W-07 | P1 | WS-10 | `features/git/GitGraph.tsx` | Ref chips capped at 2 + "+N" chip with tooltip listing the rest; message column `min-w-0 truncate`; graph-to-message gap 12; Refresh moves onto the summary row as an icon button. |
| W-08 | P1 | WS-12, WS-21, WS-20, NAV-34 | `features/terminals/TerminalPane.tsx`, `AgentPanes.tsx`, `ShellPanes.tsx` | Pane header per 4.23: fixed 32px, 24px icon buttons, maximize/restore (`Mod+Shift+Enter`), state text + last activity, needs-input state text in signal (no bar or tint since T-0015), focused-pane outline, title button with pointer cursor and "Open workspace" tooltip. |
| W-09 | P1 | WS-13 | `packages/daemon/src/agents/agent-runner.ts`, `packages/daemon/src/agents/__tests__/agent-runner.test.ts`, `ShellPanes.tsx` | Start the agent without echoing its full launch command (run it through a short wrapper script or clear the line before exec; test that the scrollback does not contain the settings path). Label Shell 1 "Agent" only when `task.agent` is set; otherwise "Shell 1". |
| W-10 | P1 | WS-14 | `TerminalView.tsx`, `packages/daemon/src/terminals/terminal-service.ts` | Debounce `fit()` to 100ms on ResizeObserver; send resize only when cols/rows change; on re-attach send the current size before replaying scrollback; cap replayed scrollback to the last screen + 1000 lines. |
| W-11 | P1 | WS-18 | `lib/view-store/`, `Dashboard.tsx` | Per-workspace last tab `tabByTask: Record<string, WorkspaceTab>`; open rules per 5.4 (running/needs-input -> Terminal; new workspace -> Terminal; else last tab, default Terminal). |
| W-12 | P1 | WS-19 | `AgentPanes.tsx`, `lib/panes.ts`, `lib/view-store/` | Header 44: title + count, Segmented layout 1 / 2 / 4 / Auto (persisted), "Add a pane" Combobox (search by id/title). Last pane spans the remaining columns so no hole. |
| W-13 | P1 | WS-29 (hint part) | `ShellPanes.tsx` | Remove the "Shell 1 runs the agent..." line; New shell moves into the tab row's right side as an icon+label button with tooltip "New shell  Ctrl+Shift+`. Shell 1 runs the agent." |
| W-14 | P1 | WS-17 | `TaskDetail.tsx`, `features/tasks/FanOutForm.tsx` | Agent action: "Start agent" when none runs, hidden while running/needs-input, "Restart agent" after failed/ended. Fan-out moves to the "..." menu as "Run more tries..." (dialog with count + checkbox) and is hidden for running tasks. |
| W-15 | P2 | WS-30 | `TerminalPane.tsx`, `TerminalView.tsx`, `styles.css` | Visible xterm scrollbar on hover; "Jump to latest" pill when scrolled up. |
| W-16 | P2 | WS-30 (find, links) | `TerminalView.tsx`, `package.json` | ASK OWNER FIRST: add `@xterm/addon-search` (Mod+F find bar 28px top-right: input, count, prev/next, case, regex) and `@xterm/addon-web-links` (underline on hover, open in new tab). |
| W-17 | P2 | WS-31 | — | Dropped for now: a CJK mono fallback needs another font package; system fallback stays. |

### PR 4 — Sidebar, table, board, details

| Id | Pri | Fixes | Files | Change |
|---|---|---|---|---|
| L-01 | P0 | NAV-03, NAV-06, NAV-07, NAV-08, RA-06, WS-25 | `features/tasks/TaskTable.tsx`, `styles.css` | Table per 4.25: fixed 40px rows, Id column 72, title truncates with relation chips inline, container-query column sets (>=1000 / 760-999 / 560-759 / card list < 560), Age removed (tooltip on Last activity), Changes/PR hidden when empty, nowrap times, sticky header, right-edge fade. Drop `min-w-[52rem]`. |
| L-02 | P0 | NAV-05 | `TaskTable.tsx`, `TaskBoard.tsx`, `lib/tasks.ts` (`sortTasks`), `lib/view-store/` (`sort`) | Stable default sort (state group order, then id desc); sortable header buttons with `aria-sort` + arrow; freeze row order while the pointer is over the table or for 2s after keyboard navigation (`useFrozenOrder` hook). |
| L-03 | P0 | NAV-04, RA-05 | `features/tasks/StateFilter.tsx`, `Dashboard.tsx` | Filter per 4.30: measured chip row with "+N" overflow menu at main >= 900; "State" menu button below 900; needs-you states first; counts follow search. |
| L-04 | P1 | RA-26, RA-28, RA-23 (table part) | `TaskTable.tsx` | Whole-row click target (stretched button), row hover/selected/selection-active styles, roving tabindex on rows, Enter opens workspace, `i` details, Shift+F10 menu. |
| L-05 | P1 | NAV-21 | `TaskBoard.tsx`, `Dashboard.tsx` | Board per 4.26: full-height columns with own scroll, "Show finished" toggle, filter drives columns, needs-you left border, edge fades, single-column mode < 768. |
| L-06 | P1 | NAV-09, NAV-11, NAV-14 | `features/tasks/TaskDetail.tsx`, `TaskEditForm.tsx`, `CompareTries.tsx`, `FanOutForm.tsx` | Details per 4.27: 40px header, action row, banners, collapsible remembered sections in the new order, Notes section with empty state, edit mode replaces title/notes with focus in Title and sticky Save/Cancel footer, new copy for ports/tries/keep. |
| L-07 | P1 | NAV-12, WS-32 | `features/tasks/Timeline.tsx`, `lib/tasks.ts`, `features/tasks/task-events.ts`, all `formatAge` callers | One `formatAge(date, now, 'short' \| 'long')` per 5.7 with exact-time tooltip; Activity uses relative times, groups consecutive identical events ("×4"), actor "You"/"Agent"/"Daemon". |
| L-08 | P1 | NAV-13 (details menu) | `TaskDetail.tsx` | "..." menu: Save as template, Run more tries..., separator, Cancel task, Delete task (both danger, confirm). Anchored under the button. |
| L-09 | P1 | NAV-15, WS-24, RA-30 | `Dashboard.tsx`, `WorkspaceList.tsx`, `TaskTable.tsx`, `components/ui/SearchField.tsx` | SearchField with clear button; Esc clears then blurs; filter-miss empty states with exact copy from 4.19 ("No tasks match “zzzz”" + Clear filter; "No matches in natamux (8 hidden)"); state counts follow the search. |
| L-10 | P1 | NAV-17, RA-23 | `WorkspaceList.tsx`, `WorkspaceRow.tsx`, new `hooks/useRovingFocus.ts` | Tree is one tab stop; keys per 4.9 (arrows, Home/End, Left/Right groups, Enter, F2, Shift+F10, Delete, type-ahead). Row "..." button gets `tabIndex={-1}`. |
| L-11 | P1 | NAV-23, NAV-27 (anatomy) | `WorkspaceRow.tsx` | Row anatomy per 4.9: age on line 1 swapping with "..." on hover/focus; slug gets full line 2; rename field replaces only the title line (24px), select-all in both entry paths, focus returns to the row; finished rows use `--muted` color, no opacity. |
| L-12 | P1 | NAV-24 | `features/app/NeedsYouAnnouncer.tsx`, `features/app/Dashboard.tsx` | Needs you per 4.12: no sidebar section since T-0022; the tree's attention cards and the title bar pill show who needs the user. |
| L-13 | P1 | WS-23 | `Dashboard.tsx`, `features/workspaces/NoWorkspace.tsx` | Enter / double-click / palette open a workspace-less task in the No-workspace main view (EmptyState with [Create workspace] + branch Combobox + [Start agent]); after "Create workspace" in details, open the new workspace on Terminal (same as the dialog path). |
| L-14 | P1 | NAV-31 | `Dashboard.tsx` (view header), `AgentPanes.tsx` | View titles 15/600 + count badge; view header 44 per 4.29. |
| L-15 | P2 | NAV-25, NAV-26 | `WorkspaceList.tsx`, `lib/workspace-groups.ts` | Pinned header count; repo prefix on pinned rows; collapsed group shows a state dot when it hides the current or a needs-you workspace. |
| L-16 | P2 | NAV-27 (handle) | `ui/ResizeHandle.tsx` | Double-click and Enter reset to default; Home/End = min/max; Shift+arrow 64px; `aria-valuetext`; drag below 180 collapses the sidebar. |
| L-17 | P2 | — | `WorkspaceRow.tsx`, `lib/view-store/` | Unseen marker: title 600 weight when the task changed state since it was last opened (`lastSeen` map). |

### PR 5 — Keyboard, palettes, deep links, dialogs, polish

| Id | Pri | Fixes | Files | Change |
|---|---|---|---|---|
| K-01 | P1 | principle 3, RA-23 | new `lib/commands/`, `Dashboard.tsx` (replace the keydown `switch`), `TerminalView.tsx` | Command registry per 4.17 and keymap per 5.1, with `when` contexts (list focused, workspace open, review state). One global dispatcher; xterm `attachCustomKeyEventHandler` lets through only Mod chords on macOS, and F1/F6/Ctrl+Shift chords on Windows. |
| K-02 | P1 | principle 1 and 3 | new `features/palette/CommandPalette.tsx`, `TitleBar.tsx` | Mod+K / F1 palette and Mod+J jump palette per 4.17 (frozen order, groups, `>`/`#` prefixes, Alt+1..9, Shift+Enter to Agents, create fallback rows, fuzzy highlight). Title bar command center and needs-you pill open them. |
| K-03 | P1 | NAV-19 | new `features/app/ShortcutSheet.tsx` | `?` sheet per 4.18, generated from `lib/commands/`. |
| K-04 | P1 | WS-22 | new `lib/route.ts`, `lib/view-store/`, `main.tsx`, `packages/daemon/src/http/server.ts` (SPA fallback) | Routes per 5.5 with history API; store <-> URL sync; back/forward; daemon serves `index.html` for unknown GET paths that accept HTML. |
| K-05 | P1 | NAV-18 | `features/projects/IssueImport.tsx` | Error state per 4.19 (danger icon, friendly sentence "GitHub CLI is not signed in. Run `gh auth login` in a terminal, then try again.", [Retry], "Show details" with the raw text); button label "Import" disabled with reason until something is picked, then "Import 3 issues". |
| K-06 | P1 | NAV-28, WS-28 | new `ui/Combobox.tsx`, `features/workspaces/NewWorkspaceDialog.tsx`, `TaskDetail.tsx` | Branch picker Combobox: groups "Recent" (default, labelled "(default)", then current + last used; the picker starts on the default branch), "Branches", "Task branches" (natamux/* collapsed by default); type to filter. Start task button `aria-disabled` with reason "Enter a title". Dialog "Start a task", two `<fieldset>` groups, each named by a `sectionTitle` heading: "Context" (Title, Context from when the project offers more than one source, Notes 6 rows for "My notes" or Reference for any other source, Attachments; "An agent skill" in Context from when the agents have a skill, with a Skill field before Reference: a button "Pick a skill" (then the skill name) that opens the SkillPicker; Start reasons "Pick a skill" and "Pick an agent to run the skill") under a 12 muted line that says what the agent reads for the picked source; then "Workspace and agent" (Project, Branch from, Agent). Title gets focus; Mod+Enter. |
| K-07 | P2 | NAV-29 (part) | `features/projects/RepoPanel.tsx` | Folder field with validation message; in the desktop app a **Browse…** button opens the native folder dialog (`window.natamux.pickFolder`); dialog fixes come from F-05. Project remove is in the sidebar group menu (`DELETE /repos/:id`); rename and Projects panel actions come later. |
| K-08 | P2 | NAV-33 | `features/app/SignIn.tsx` | Show/hide token toggle (`Eye`/`EyeOff` icon button), helper "It is `NATAMUX_TOKEN` of the daemon. Without it, the daemon makes one and prints it when it starts.", busy state on Connect, inline error with `aria-describedby`. No logo work (no brand asset yet). |
| K-09 | P2 | RA-29, NAV-32 | `index.html`, `public/favicon.svg` (new), `TerminalView.tsx`, `Dashboard.tsx` | Favicon (simple SVG, theme-safe); skeletons on first load (table, sidebar); guard xterm dispose before `open` finished (`dimensions` error); close WebSocket only after it opened (no "closed before established" warning). |
| K-10 | P2 | WS-29 | — | Dropped for now: split direction for new shells waits for real pane splitting (drag-to-edge) in a later feature. |

### Audit coverage

- All P0: NAV-01 (F-05), NAV-02 (S-01), NAV-03 (L-01), NAV-04 (L-03), NAV-05 (L-02), NAV-06 (L-01), WS-01 (F-03), WS-02 (W-01), WS-03 (F-10, W-02), WS-04 (W-02), WS-05 (W-03), WS-06 (W-04), WS-07 (S-01, W-03), WS-08 (W-05), RA-01 (S-01), RA-02 (S-02), RA-03 (S-03).
- All P1: NAV-07..08 (L-01), NAV-09 (L-06), NAV-10 (F-07), NAV-11 (L-06), NAV-12 (L-07), NAV-13 (F-11, L-08), NAV-14 (L-06), NAV-15 (L-09), NAV-16 (S-06), NAV-17 (L-10), NAV-18 (K-05), NAV-19 (S-05, K-03, F-08), NAV-20 (F-12), NAV-21 (L-05), NAV-22 (F-06, F-10), NAV-23 (L-11), NAV-24 (L-12); WS-09 (W-06), WS-10 (W-07), WS-11 (F-04), WS-12 (W-08), WS-13 (W-09), WS-14 (W-10), WS-15 (W-02), WS-16 (S-02), WS-17 (W-14), WS-18 (W-11), WS-19 (W-12), WS-20 (W-08), WS-21 (W-08), WS-22 (K-04), WS-23 (L-13), WS-24 (L-09), WS-25 (L-01, W-02), WS-26 (F-06), WS-27 (W-03, F-08), WS-28 (K-06); RA-04 (S-05), RA-05 (L-03), RA-06 (L-01), RA-07 (W-03), RA-09 (S-07), RA-10/11/12 (F-01), RA-13 (F-07), RA-15 (F-01), RA-18 (S-09, partial), RA-21 (F-07), RA-23 (S-08, L-04, L-10), RA-24 (F-06), RA-25 (F-05), RA-30 (L-09).
- P2 included: NAV-25/26 (L-15), NAV-27 (L-11, L-16), NAV-29 (F-05, K-07 part), NAV-30 (F-01), NAV-31 (L-14), NAV-32 (K-09), NAV-33 (K-08), NAV-34 (W-08), WS-30 (W-15, W-16), WS-32 (L-07), WS-33/35 (F-15), WS-34 (S-05), RA-08 (S-10), RA-14 (F-01), RA-16 (F-01), RA-17 (F-03, F-04), RA-19/20/22 (F-02), RA-26/28 (L-04), RA-27 (F-14), RA-29 (F-13, K-09), RA-31 (W-13), RA-32 (F-08), RA-33 (K-08).
- Dropped or partial, with reason:
  - RA-18 (P1) partial: body stays 13px by default (density matches VS Code and keeps 2-line rows on a 900px screen); a Large UI size (14px) is one click away in the account menu.
  - WS-31 (P2) dropped: needs a CJK mono font package.
  - WS-29 (P2) split-direction part dropped until pane splitting exists; the hint part is done (W-13).
  - NAV-29 (P2) repo rename, and remove in the Repos panel, wait for a rename endpoint. Remove is in the sidebar group menu.
  - NAV-33 (P2) logo part dropped: no brand asset yet.
  - W-16 (terminal find and links) waits for the owner's OK on two packages.

### Done checks for every PR
- Light and dark screenshots at 1440, 1280, 1024, 768 and 390 with `playwright-cli` (All tasks, workspace Terminal, Changes, Agents, a dialog, the palette).
- Keyboard-only pass: reach and use every control touched by the PR; Tab order and focus return as in 5.2.
- Contrast check of any new color pair with `tools/contrast.py`.
- `pnpm` typecheck, lint and tests run in Codespaces or CI only (not on the local machine).
