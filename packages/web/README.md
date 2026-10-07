# @helmsman/web

The dashboard. It shows every task, and the tasks that need you come first.

## Run it in a codespace

Two terminals, after `pnpm build`:

```sh
# 1: the daemon. It prints a token.
pnpm --filter @helmsman/daemon dev

# 2: the dashboard
pnpm --filter @helmsman/web dev
```

Open the forwarded port 5173 and paste the token. The dev server sends `/api` to the daemon at `127.0.0.1:4317` (set in `vite.config.ts`), so the browser never needs the daemon's address. The token is kept in the browser's local storage.

## What it shows

- A band at the top: how many tasks need you, longest wait first. States that need you: needs input, in review, blocked, failed.
- Counts per state. Click one to filter the table.
- The task table, newest update first, updating live over the daemon's WebSocket.
- Details of one task with the actions that fit its state (approve, request changes, retry, unblock, cancel). Cancel asks twice.
- A workspace field: create a git worktree and a branch for the task, or remove it (asks twice).

## Keyboard

| Key | Action |
|---|---|
| `j` / `k` or arrows | Move the selection |
| `Enter` | Open the details |
| `Esc` | Close the details, or leave a field |
| `/` | Search |
| `n` | New task title |
| `a` / `r` | Approve / request changes on the selected task in review |

## Not done yet

- Board view, desktop notifications, diff size and CI status columns.
- The daemon does not serve the dashboard yet; for now Vite does.
- A page for tasks of several repositories.

## Look

Light and dark follow the system. System fonts only. Colors are variables at the top of `src/styles.css`.
