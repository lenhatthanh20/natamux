# MVP checks

How to check the success criteria of `plan-features.md` section 8 by hand, in a codespace. Run them before calling the MVP done, and again after big changes.

## Setup

```sh
scripts/dev.sh demo
```

Open port 5173, token `devtoken`. Claude Code must be installed and logged in where the daemon runs (`HELMSMAN_CLAUDE_COMMAND` if it is not `claude` on the `PATH`).

## 1. Start 3 agents on 3 tasks from one repo in under 2 minutes

1. Start a timer.
2. Add three tasks with the title field (`n`, type, Enter).
3. For each: select it, **Start Claude Code**. Helmsman makes the workspace and opens its terminal.
4. Stop the timer when the third agent shows its first answer.

Pass: under 2 minutes, three tasks in Running, each in its own worktree (`git worktree list`). A quicker path for the same work: one task, then **Fan out** with 2 more tries and "start Claude Code in each".

## 2. The dashboard shows the right state within 2 seconds of a change

1. In one agent's terminal, ask something that needs a permission (for example a command it must approve).
2. Watch the table. Pass: the task moves to Needs input within 2 seconds, and the Needs me strip lists it.
3. Approve in the terminal. Pass: back to Running within 2 seconds; when the agent ends its turn, In review.

## 3. A user can name which task needs them from the dashboard alone

Pass: with three tasks in different states, a person who has not watched the terminals names the waiting task from the Needs me strip alone. The tab title shows the count, and a system notification arrives when the tab is in the background (**Notify me** first).

## 4. Closing and reopening the app restores all tasks and states

1. `scripts/dev.sh down`, then `scripts/dev.sh up`.
2. Reload the dashboard.

Pass: the same tasks, states, workspaces, pull requests and activity. Shells do not survive a daemon restart; **Start a new shell** opens a new one in the same worktree.
