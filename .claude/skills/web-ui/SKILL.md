---
name: web-ui
description: Conventions for the Natamux dashboard (packages/web, React + Tailwind v4). Use before adding or changing UI - components, styles, colors, dark mode, keyboard shortcuts, accessibility, phone layout - and when writing web tests or checking a UI change visually.
---

# Dashboard UI

The spec is `docs/ui-spec.md` (tokens, layout, components). Read the section for what you change. Update it when you add a token or a shared component.

## Layout of the code

| Path | What |
|---|---|
| `src/features/<name>/` | One feature: its components, logic, feature-only hooks and `__tests__/`. `app` (shell; `Dashboard.tsx` renders the screens, its `use*.ts` hooks hold queries, mutations, layout and commands), `tasks`, `stages` (+ `editor/`), `projects`, `git`, `files`, `terminals`, `workspaces`, `sidebar`, `palette` |
| `src/components/` | Domain parts used by 2+ features (`StateBadge`, `ShapeGlyph`, `Markdown`, …) |
| `src/components/ui/` | Shared controls: `Button`, `IconButton`, `Menu`, `Dialog`, `Tabs`, `Segmented`, `Select`, `Combobox`, `Tooltip`, `Toaster`, … |
| `src/components/ui/styles.ts` | Class builders `button()`, `iconButton()`, `input`, and the `ICON` sizes. Use them; do not restyle controls by hand |
| `src/lib/` | Shared logic without React: `api/`, `commands/` (palette and shortcuts), `view-store/`, `dispatch.ts` (keys), `tasks.ts` (labels, row names), `theme.ts`, `route.ts`, … |
| `src/hooks/` | Generic hooks used by 2+ features |
| `src/styles.css` | Color tokens on `:root`, redefined for dark. `@theme inline` maps them to Tailwind colors |

## Rules

- **Where code goes:** new code goes in its feature folder. Move it to `components/`, `lib/` or `hooks/` only when 2 or more features use it. No feature-wide `index.ts`: it would pull lazy views (`MonacoDiff`, `TerminalView`, `MarkdownBody`) into the first load (`first-load.test.ts`).
- **Colors only through tokens:** `bg-surface`, `bg-raised`, `text-ink`, `text-ink-2`, `text-muted`, `border-line`, `text-accent`, `text-danger`, `text-signal`, `bg-hover`, and so on. Never a hex value in a component. xterm and Monaco need real colors, so `features/terminals/terminal-theme.ts` and `features/files/monaco-setup.ts` hold matching values.
- **Dark mode:** `data-theme="dark"` or `"light"` on `<html>` (from `lib/theme.ts`). Tokens switch by themselves. Check both themes.
- **Accessible names are the API.**
  - Every control has one: `IconButton` requires `label`.
  - Task rows and cards are named `"<title>, <state>, <id>, <age>"` (`lib/tasks.ts`).
  - Tests in `src/**/*.test.tsx` and in `e2e/` query by role and name. Renaming a label means updating those tests in the same change.
- **Keyboard first.** A new action gets a command in `lib/commands/`, so the palette (Mod+K) and the shortcut sheet (`?`) show it. Keys go through `lib/dispatch.ts`. A terminal keeps its keys unless `keyBelongsToShell` says otherwise.
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

## Live data and renders

The dashboard runs for hours with many agents. A change of one task must render only what shows it.

- **Push, do not poll.** The daemon sends changes on `/events`. `lib/live.ts` parses each message once (`parseLiveMessage`) and writes the cache once per frame (`applyLiveMessages`):
  - `activity.changed` updates `['activity']` with `setQueryData`.
  - `shells.changed` invalidates `['task-shells', id]`.
  - `workspace.dirty` invalidates the task's `changes`, `file-diff`, `graph` and `stage-files`, and `['diffstats']` at most once per `DIFFSTATS_EVERY_MS` (15 s) for all tasks together: at once after a quiet time, else once at the end of the 15 s. The daemon sends up to 3 in a row soon, then at most one per task every 10 s while changes go on. Not `task-shells`: the daemon sends `shells.changed` for a hook's shell state and for a new program in front.
  - `resync`, or a reconnect, invalidates everything.
- A `refetchInterval` is only a fallback: `FALLBACK_POLL_MS` (60 s); the per-row shells poll `SHELLS_EVERY_MS` is 5 min. New data the daemon changes gets a message and a line in `applyLiveMessages`, not a faster poll.
- **Store selectors.** `Dashboard` reads `useViewStore` with `useShallow` and only the fields it shows. Actions and values needed only in a handler come from `useViewStore.getState()`.
- **Data at the leaves.** `Dashboard` owns the activity and diffstats queries but does not read them (`notifyOnChangeProps: []`, or a `select`). Parts read them with `useActivity(select)` and `useDiffStats()` from `hooks/useActivity.ts`; these never fetch. A sidebar row reads its own activity and question.
- **Memo rows.** `WorkspaceRow` and `AgentRow` are `memo`. Give them stable props: callbacks through `useStableCallback`, strings and booleans instead of new objects, `tabStop` and `activate` instead of `roving.item(key)` (the row calls `rovingItem`), and no `children` when there is nothing under the row.
- `src/__tests__/render-scope.test.tsx` counts renders: one task's activity change renders `Dashboard` 0 times and only that task's row. Keep it green.
- **Long lists:** `cv-row` (in `styles.css`) adds `content-visibility: auto`. Set `--cv-h` to the row height, for example `[--cv-h:2.5rem]`. Not on rows with a focus ring outside the box, or with a menu that is not portaled.

## Tests

- Vitest with jsdom and Testing Library. A `X.test.tsx` sits in `__tests__/` next to `X.tsx`.
- Call `cleanup()` (and `vi.restoreAllMocks()`) in `afterEach`.
- Query with `screen.getByRole(role, { name })`, not CSS or test ids. The code has no `data-testid`.
- Pure logic goes into `lib/` with a plain unit test, so components stay thin.
- Run the tests in the codespace or CI only (see the `validate` skill).

## Check a change visually

- CI's `E2E` workflow runs the dashboard from the PR's code. On a failure its artifact has screenshots.
- `scripts/e2e.sh up` plus `playwright-cli` shows **main** on localhost:8080. See the `e2e` skill.
- To see a branch before it merges, use the codespace (`scripts/dev.sh demo` or `seed`, port 5173), and take screenshots there.
- Look at: dark and light, 1440×900 and 390×844, keyboard focus visible, long titles truncated, empty states.
