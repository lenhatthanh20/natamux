---
name: helmsman-task
description: Use when the environment has HELMSMAN_TASK_ID, which means this session works on a Helmsman task. Covers reading the task, asking the user a question through Helmsman, reporting the work as done, and filing follow-up tasks with the helmsman CLI.
---

# Working on a Helmsman task

Helmsman runs this session in the workspace (a git worktree) of one task. `HELMSMAN_TASK_ID` names it. The user watches all tasks on the Helmsman dashboard, not this terminal.

## Read the task first

```sh
helmsman task show "$HELMSMAN_TASK_ID"
```

- The title and notes are the request. Notes may end with `Imported from <url>`: the GitHub issue it came from.
- `Depends on` lists tasks that must be done first. Do not start work that needs them.

## Ask when blocked

When a decision belongs to the user, ask through Helmsman, then stop and wait for the answer in this terminal:

```sh
helmsman ask "Keep the old CSV columns, or drop them?"
```

- One question, short, with the options.
- With Claude Code hooks on (Helmsman starts Claude Code with them), a permission prompt already tells the dashboard. Use `ask` for questions in plain text.

## Report done

When the work is finished and committed, say so:

```sh
helmsman done
```

The task moves to review. The user reviews the diff in the dashboard and may send comments back as a new prompt.

## File follow-up work

Work that is out of scope becomes a new task, not a bigger change:

```sh
helmsman task create "Add paging to the export" --description "Found while fixing T-0004." --label follow-up
```

## Rules

- Stay in the workspace folder. Other folders belong to other tasks.
- `PORT` is this task's own port for a dev server.
- Commit on the task branch. Do not push or open a pull request unless the user asks; the dashboard has buttons for both.
