# @natamux/web

The dashboard. It shows every task, and the tasks that need you come first.

## Run it in a codespace

Two terminals:

```sh
# 1: the Rust daemon. It prints a token.
cargo run -p natamux-daemon

# 2: the dashboard
pnpm --filter @natamux/web dev
```

Open the forwarded port 5173 and paste the token. The dev server sends `/api` to the daemon at `127.0.0.1:4317` (set in `vite.config.ts`), so the browser never needs the daemon's address. The token is kept in the browser's local storage.

## What it shows

- A band at the top: how many tasks need you, longest wait first. States that need you: needs input, in review, blocked, failed.
- Counts per state. Click one to filter the table.
- The task table, newest update first, updating live over the daemon's WebSocket.
- Details of one task with the actions that fit its state (approve, request changes, retry, unblock, cancel). Cancel asks twice.
- A workspace field: create a git worktree and a branch for the task, or remove it (asks twice).
- Views beside the table: a board, and a progress view of every task against its stages.
- A Docs tab (the stage files, editable, shown as Markdown or source) in each workspace. The details panel has a Progress view (the evidence of each stage) and a Details view. Project settings for the stage pipeline of a repository.
- Terminals, changes and history of a task's workspace, system notifications, a command palette, diff size of each task, and several repositories.

## Keyboard

| Key                 | Action                                                   |
| ------------------- | -------------------------------------------------------- |
| `j` / `k` or arrows | Move the selection                                       |
| `Enter`             | Open the workspace of the task                           |
| `i`                 | Show or hide the details                                 |
| `Esc`               | Close the open workspace or details, or leave a field    |
| `/`                 | Search                                                   |
| `n`                 | New task title                                           |
| `a` / `r`           | Approve / request changes on the selected task in review |

## Not done yet

- The daemon serves the dashboard itself only with `NATAMUX_WEB_DIR` (the desktop app); otherwise Vite or nginx serves it.

## Look

Light and dark follow the system. System fonts only. Colors are variables at the top of `src/styles.css`.

File type icons in the Changes tab come from [Material Icon Theme](https://github.com/material-extensions/vscode-material-icon-theme) (MIT, © Material Extensions). Only the icons named in `src/lib/file-icons.ts` are bundled; to add a type, add it there and to the list in `src/components/FileIcon.tsx`.
