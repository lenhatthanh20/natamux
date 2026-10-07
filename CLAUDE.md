# CLAUDE.md

Helmsman: an Agent Development Environment (ADE). Run many coding agents in parallel, manage the work as tasks, see every task's state on one dashboard.

## Read first

- `docs/plan-features.md` — what to build, and MVP / v1 / Later tags.
- `docs/plan-tech-stack.md` — stack, architecture, build order, open decisions.

## Rules

- Independent product. Never copy code, text or structure from Garoon, garoon-atlas or any employer repo. Do not use the names Orca or Atlas.
- Other ADEs (Orca, Superset, Emdash, …) are reference for ideas only.
- Write code, comments and docs in simple English. Comment only the why.
- Package manager: `pnpm` only, version pinned in `package.json`. Never `npm`, `npx`, `yarn`, `bun`.
- Run `pnpm` and `node` only in Codespaces or CI, never on the local machine. Local work is editing files only. `.claude/settings.json` denies these commands.
- Keep dependencies few. Ask before adding a package.
- Confirm with the user before deleting anything.
- Follow `docs/plan-tech-stack.md` section 10 for build order. Do not skip ahead.
