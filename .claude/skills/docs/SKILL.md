---
name: docs
description: Which Natamux documents to update for a change, and how to write them. Use whenever a change is visible to users (UI, CLI, API, config, behavior, a fix), when preparing release notes or the changelog, or when the user asks to update docs, the site or the homepage.
---

# Documentation

## Where things live

| Place | Audience | Notes |
|---|---|---|
| `site/src/content/docs/docs/**` | Users | The Starlight site at https://natamux.github.io/docs/. Links look like `/docs/guides/terminal/` |
| `site/src/content/docs/docs/help/changelog.md` | Users | The top `## vX.Y.Z (date)` section is the next release, even before it is tagged. Check `packages/web/package.json` `version` |
| `docs/releases/vX.Y.Z.md` | Users | The body of the GitHub release. Sections: intro line, `## Fixes` / `## Terminal` / `## Dashboard`…, `## Download`, `## Before you open it`. The release workflow fails without it |
| `site/src/pages/index.astro` | Visitors | The homepage. It holds the download version string |
| `docs/*.md` | Developers and agents | Plans (`plan-features.md`, `plan-tech-stack.md`), designs, `ui-spec.md`, `docker.md`, `e2e.md`, `dev-setup.md` |
| `README.md`, `SECURITY.md`, `PRIVACY.md` | GitHub visitors | |

## Change → page

| Change | Update |
|---|---|
| Terminal, shells, fonts | `guides/terminal.md` |
| Board, table, panels, Needs you, dialogs | `guides/dashboard.md` |
| Changes and History tabs, commit, push, PR | `guides/changes-and-history.md`, `guides/github.md` |
| Several agents on one task | `guides/fan-out.md` |
| Desktop app, updates | `guides/desktop-app.md`, `docs/design-desktop-update.md` |
| Docker | `guides/self-host-docker.md`, `docs/docker.md` |
| Stages, tasks, workspaces, hooks | `concepts/*.md` |
| One agent (Claude Code, Codex, …) | `agents/<agent>.md` |
| `natamux` commands and flags | `reference/cli.md` |
| HTTP routes | `reference/http-api.md` |
| Settings, env vars | `reference/configuration.md`, `reference/environment-variables.md` |
| Shortcuts | `reference/shortcuts.md` (and `lib/commands.ts`, which the shortcut sheet reads) |
| Design tokens, components | `docs/ui-spec.md` |
| Install, first run | `getting-started/*.md` |
| Common problems | `help/troubleshooting.md`, `help/faq.md` |

Every user-visible change also gets:

- a bullet in the changelog's top section;
- a bullet in the matching `docs/releases/vX.Y.Z.md`. Create that file from the previous one when it does not exist.

## Style

- Simple English. Short sentences. Present tense. No marketing words ("powerful", "seamless").
- Say what the user sees and does. Write UI labels in **bold**, exactly as the UI spells them, such as **Account → Terminal font…**.
- A fix starts with "Fix:" in the changelog. It says what went wrong, then what happens now.
- One fact in one place. Link to it instead of repeating it.
- Comments and docs explain why, not what the code already says.
- Tables for options and mappings. Bullets over long paragraphs.
- Never write the names Orca or Atlas, and never copy text from an employer repo.
- Write dates as `YYYY-MM-DD`.

## Check

- A site change triggers the `Site` workflow (`site/**`), which runs `astro build`. A broken link or frontmatter fails it.
- `site/` has its own lockfile. Build it with `pnpm --dir site install --ignore-workspace` and `pnpm --dir site build`, in the codespace or CI only (see the `validate` skill).
- After a release, the homepage version and the newest `docs/releases` file must match the tag.
