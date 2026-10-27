---
name: natamux-task
description: Use when the environment has NATAMUX_TASK_ID, which means this session works on a Natamux task. Covers reading the task, asking the user a question through Natamux, reporting each stage as done, and filing follow-up tasks with the natamux CLI.
---

# Working on a Natamux task

Natamux runs this session in the workspace (a git worktree) of one task. `NATAMUX_TASK_ID` names it. The user watches all tasks on the Natamux dashboard, not this terminal.

## Read the task first

```sh
natamux task show "$NATAMUX_TASK_ID"
```

- The title and notes are the request. Notes may end with `Imported from <url>`: the GitHub issue it came from.
- `Depends on` lists tasks that must be done first. Do not start work that needs them.

## Ask when blocked

When a decision belongs to the user, ask through Natamux, then stop and wait for the answer in this terminal:

```sh
natamux ask "Keep the old CSV columns, or drop them?"
```

- One question, short, with the options.
- With Claude Code hooks on (Natamux starts Claude Code with them), a permission prompt already tells the dashboard. Use `ask` for questions in plain text.

## Work stage by stage

A task goes through stages (for example Spec, Plan, Test spec, Implement). Natamux sends one prompt per stage. Do only the stage in the prompt.

- Stage files live in `.natamux-task/` in the workspace. Read the earlier ones the prompt names. Write the file the prompt asks for, with the sections it names.
- In the spec, plan and test-spec stages, do not change code.
- Git ignores `.natamux-task/`. Do not commit it.

## Report done

When the stage is finished (and committed, for a code stage), say so:

```sh
natamux done
```

- `done` ends the current stage only. Run it once per stage prompt.
- Natamux then checks the stage: the file and its sections, commits, a clean worktree, a check command.
- When a check fails, the failure comes back as a prompt. Fix it and run `natamux done` again.
- When the checks pass, the stage waits for the user to approve it, or moves on at once when its gate is automatic. The next stage arrives as a new prompt. The user may also send comments back as a prompt.

## Review comments

A prompt may say to read `.natamux-task/review-comments.md`: the user left comments on your diff.

- Fix each item under **New**. Items under **Still open** were sent before; check them too.
- An item shows its id (`c3`), file and lines, the quoted code and the request. A note "(the code changed; this may be done)" means the code moved; look before you change anything.
- When you finish, reply with what you changed per id, for example `c3: renamed the helper`.
- Do not edit `review-comments.md`. Do not change the state of a comment; the user resolves them.

## File follow-up work

Work that is out of scope becomes a new task, not a bigger change:

```sh
natamux task create "Add paging to the export" --description "Found while fixing T-0004." --label follow-up
```

## Rules

- Stay in the workspace folder. Other folders belong to other tasks.
- `PORT` is this task's own port for a dev server.
- Commit on the task branch. Do not push or open a pull request unless the user asks; the dashboard has buttons for both.
