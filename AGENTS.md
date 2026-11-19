# AGENTS.md

Natamux: an Agent Development Environment (ADE). Run many coding agents in parallel, manage the work as tasks, see every task's state in one native app.

## Read first

- `docs/plan-features.md` — what to build, and MVP / v1 / Later tags.
- `docs/plan-tech-stack.md` — stack, architecture, build order, open decisions.

## Rules

- Independent product. Never copy code, text or structure from any employer repo, and never use its product names.
- Other ADEs are reference for ideas only.
- Write code, comments and docs in simple English. Comment only the why.
- Only `site/` uses `pnpm`, pinned in `site/package.json`. Never `npm`, `npx`, `yarn`, `bun`.
- Run `pnpm` and `node` only in Codespaces or CI, never on the local machine. Local work is editing files only. The Claude Code settings (`.claude/settings.json`) deny these commands.
- Keep dependencies few. Ask before adding a package.
- Follow `docs/plan-tech-stack.md` section 10 for build order. Do not skip ahead.

## Code layout

- Folder by feature. Same plural names in every package: `tasks`, `stages`, `events`, `workspaces`, `terminals`, `git`, `github`, `agents`, `projects`. Put new code in the feature it belongs to.
- Rust crates live in `crates/<name>/src/<feature>/`. The app uses `app/`, `daemon/` and feature folders; the CLI uses `crates/natamux-cli/src/`.
- No folder with a single file, unless it is a feature root.
- No import cycles between files.
- Size guide: a file of 500 lines or less, a function of 80 lines or less. Split by concern before you pass it.
- Moving a file: `git mv`, keep its name, and move its tests with it.

## Rust

The move to Rust: `docs/plan-rust-migration.md`. Rules from phase 0 on:

- Every Rust PR targets `main`.
- `cargo` runs in CI only. Never on the owner's Mac, also not in a Natamux task terminal there. Optional: the hardened Docker command, or a `natamux-dev` user where the plan allows one (plan, "Safe local setup").
- Before a PR, `cargo xtask ci` must pass: fmt, clippy, tests, layers. CI runs it (`.github/workflows/rust.yml`).
- No `unwrap`, `expect` or `panic` outside tests. Clippy denies them.
- Every bug fix gets a regression test.
- Layers: only L4 crates (`natamux-app`, `natamux-cli`, `natamux-theme`, `natamux-ui`, `natamux-update`) may use `gpui`. `cargo xtask layers` checks it.
- UI code reads theme tokens, never hard-coded colours.
- A UI change: check screenshots in the light and the dark theme, at more than one window size.
- Dependencies: ask first, as for npm packages. Commit `Cargo.lock`; build with `--locked`. A new source (registry or git) must be allowed in `deny.toml`.
- Layout: crates in `crates/<name>/`, folder by feature inside `src/`. Unit tests in a `#[cfg(test)]` module, integration tests in `crates/<name>/tests/`.

## Skills (.claude/skills)

Each skill is `.claude/skills/<name>/SKILL.md`. Agents that do not load these skills: read the file before that kind of work.

- `validate` — run checks in the codespace or CI.
- `terminal` — how terminals work, and known Claude Code issues.
- `docs` — which docs a change needs, and the writing style.
