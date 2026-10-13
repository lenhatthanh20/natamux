---
name: web-ui
description: Conventions for the Natamux dashboard (packages/web, React + Tailwind v4). Use before adding or changing UI - components, styles, colors, dark mode, keyboard shortcuts, accessibility, phone layout - and when writing web tests or checking a UI change visually.
---

# Dashboard UI

The spec is `docs/ui-spec.md` (tokens, layout, components). Read the section for what you change. Update it when you add a token or a shared component.

## Layout of the code

| Path | What |
|---|---|
| `src/components/` | Screens and parts. `Dashboard.tsx` holds the state and wires everything |
| `src/components/ui/` | Shared controls: `Button`, `IconButton`, `Menu`, `Dialog`, `Tabs`, `Segmented`, `Select`, `Combobox`, `Tooltip`, `Toaster`, … |
| `src/components/ui/styles.ts` | Class builders `button()`, `iconButton()`, `input`, and the `ICON` sizes. Use them; do not restyle controls by hand |
| `src/lib/` | Logic without React: `api.ts`, `commands.ts` (palette and shortcuts), `dispatch.ts` (keys), `tasks.ts` (labels, row names), `theme.ts`, `route.ts`, … Each has a `*.test.ts` |
| `src/styles.css` | Color tokens on `:root`, redefined for dark. `@theme inline` maps them to Tailwind colors |

## Rules

- **Colors only through tokens:** `bg-surface`, `bg-raised`, `text-ink`, `text-ink-2`, `text-muted`, `border-line`, `text-accent`, `text-danger`, `text-signal`, `bg-hover`, and so on. Never a hex value in a component. xterm and Monaco need real colors, so `lib/terminal-theme.ts` and `lib/monaco-setup.ts` hold matching values.
- **Dark mode:** `data-theme="dark"` or `"light"` on `<html>` (from `lib/theme.ts`). Tokens switch by themselves. Check both themes.
- **Accessible names are the API.**
  - Every control has one: `IconButton` requires `label`.
  - Task rows and cards are named `"<title>, <state>, <id>, <age>"` (`lib/tasks.ts`).
  - Tests in `src/**/*.test.tsx` and in `e2e/` query by role and name. Renaming a label means updating those tests in the same change.
- **Keyboard first.** A new action gets a command in `lib/commands.ts`, so the palette (Mod+K) and the shortcut sheet (`?`) show it. Keys go through `lib/dispatch.ts`. A terminal keeps its keys unless `keyBelongsToShell` says otherwise.
- **Phone width.**
  - No sideways scroll at 390 px.
  - Labels may become aria-labels on narrow widths (for example `Segmented` under 560 px). The accessible name must stay the same.
- **Text in the UI:** sentence case, plain verbs. A button says what it does ("Start task", not "Submit"). An error says what failed and what to do.
- **Icons:** one size per job, from `ICON`.
- **Motion:** only for what changed. It respects reduced motion (`styles.css` has the media query).
  - An endless animation moves `transform` or `opacity` only; `background-position` repaints every frame.
  - `lib/motion-ticker.ts` moves endless animations 8 times a second, so a smooth look needs no extra frames. Do not drive endless motion from JS with requestAnimationFrame.
  - List it under `:root[data-window="inactive"]` too, so it stops while the window has no focus (`window-focus.test.ts` checks).
- **Dependencies:** ask before adding a package.

## Tests

- Vitest with jsdom and Testing Library. A `X.test.tsx` sits next to `X.tsx`.
- Call `cleanup()` (and `vi.restoreAllMocks()`) in `afterEach`.
- Query with `screen.getByRole(role, { name })`, not CSS or test ids. The code has no `data-testid`.
- Pure logic goes into `lib/` with a plain unit test, so components stay thin.
- Run the tests in the codespace or CI only (see the `validate` skill).

## Check a change visually

- CI's `E2E` workflow runs the dashboard from the PR's code. On a failure its artifact has screenshots.
- `scripts/e2e.sh up` plus `playwright-cli` shows **main** on localhost:8080. See the `e2e` skill.
- To see a branch before it merges, use the codespace (`scripts/dev.sh demo` or `seed`, port 5173), and take screenshots there.
- Look at: dark and light, 1440×900 and 390×844, keyboard focus visible, long titles truncated, empty states.
