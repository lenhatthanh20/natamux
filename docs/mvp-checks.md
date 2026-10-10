# MVP checks

How to check the success criteria of `plan-features.md` section 8 by hand, in a codespace. Run them before calling the MVP done, and again after big changes.

## Setup

```sh
scripts/dev.sh demo
```

Open port 5173, token `devtoken`. Claude Code must be installed and logged in where the daemon runs (`NATAMUX_CLAUDE_COMMAND` if it is not `claude` on the `PATH`). Without it, `scripts/dev.sh seed` starts the daemon with the fake agent (`scripts/fake-agent.sh`), which calls the same hooks.

## 1. Start 3 agents on 3 tasks from one repo in under 2 minutes

1. Start a timer.
2. Add three tasks with the title field (`n`, type, Enter).
3. For each: select it, **Start agent**. Natamux makes the workspace, starts the Spec stage and opens its terminal.
4. Stop the timer when the third agent shows its first answer.

Pass: under 2 minutes, three tasks each with its own worktree (`git worktree list`) and one agent each (one `agent.launched` in the task's activity). The tasks are Running, or In review once the agent wrote the spec. A quicker path for the same work: one task, then **Fan out** with 2 more tries and "start Claude Code in each".

## 2. The dashboard shows the right state within 2 seconds of a change

1. In one agent's terminal, ask something that needs a permission (for example a command it must approve).
2. Watch the table. Pass: the task moves to Needs input within 2 seconds, and the Needs me strip lists it.
3. Approve in the terminal. Pass: back to Running within 2 seconds; when the agent ends its turn without `natamux done`, Needs input; after `natamux done` and passing checks, In review (manual gate).

## 3. A user can name which task needs them from the dashboard alone

Pass: with three tasks in different states, a person who has not watched the terminals names the waiting task from the Needs me strip alone. The tab title shows the count, and a system notification arrives when the tab is in the background (**Notify me** first).

## 4. Closing and reopening the app restores all tasks and states

1. `scripts/dev.sh down`, then `scripts/dev.sh up`.
2. Reload the dashboard.

Pass: the same tasks, states, workspaces, pull requests and activity. Shells do not survive a daemon restart; **Start a new shell** opens a new one in the same worktree.

## Results

### 2026-10-09, codespace, fake agent

Claude Code cannot log in in the codespace, so the fake agent stood in for it. Times were taken in the browser with Playwright, through the codespace port forward.

| Check | Result | Measured |
|---|---|---|
| 1. Three agents in under 2 minutes | Pass | 15.4 s from the first key to the third agent's terminal (agents at 1.5 s, 6.7 s, 11.8 s); three worktrees, one launch each |
| 2. State within 2 seconds | Pass | 224 to 481 ms from an agent event at the daemon to the new state in the table (3 rounds of Needs input and back to Running) |
| 3. Name the waiting task | Pass | The Needs you strip and the count in the tab title ("(18) Natamux"); a background tab got the notification "T-0006: Needs input" |
| 4. Restart keeps everything | Pass | `down`, then `up`: all 40 tasks kept their state, stage, worktree, branch, pull request and agent; the activity was there after a reload |

Found and fixed while checking:
- Typing the next title while the last one was being added lost it: the Add task button was busy, which stops Enter, and the field was cleared only when the daemon answered.
- Every new backlog task waited in Needs you: a context of the task's own words waited for approval. It now passes by itself.
- **Start agent** on a task with no workspace launched two agents: the new workspace started the Spec stage's agent, and the button started another, typed into the first. Starts by hand now go through the stage runner.

Still to check with a real agent: the permission prompt of step 2.1, and the time to the agent's first answer in check 1.
