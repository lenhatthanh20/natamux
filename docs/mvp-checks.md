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

Pass: the same tasks, states, workspaces, pull requests and activity. The agents of running, waiting and in-review tasks start again in their terminals when the daemon starts. The other shells of a task come back, with the same names, when the task is opened: an agent on its last session, the rest as plain shells (see `docs/claude-code-adapter.md`, "Resume after a restart"). The screens of plain shells and the programs in them do not survive.

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

### 2026-10-10, regression before v0.1.3

Base: main at fa9333e (v0.1.2 plus #89–#99). First pass: the diff since v0.1.2 read by hand, and the fake agent run in a shell. Every case of the test spec that a unit test can cover has one: C8 got a new one, the others had them. The checks in a codespace and in the macOS app are still to run.

| Change | Result |
|---|---|
| #90 loopback past a proxy | Read: fine. To run with `HTTP_PROXY` set |
| #91 auto permission mode | Read: fine. To run with a real agent |
| #92 login shells | Read: fine (zsh, bash, fish, nu; Windows unchanged). To run from Finder |
| #93 macOS Apple silicon only | Bug: the dry run started no app and checked no update, since both steps ran on Linux only. Fixed: both run on macOS too. Dry run passed: the app starts signed in; acting as 0.0.0 it downloads v0.1.2, the checksum matches, and the unpacked app is 0.1.2 ([run](https://github.com/lenhatthanh20/natamux/actions/runs/38066833212)) |
| #89 wording, notes | Read: fine |
| #94 details hidden | Read: fine |
| #95 no commit by default | Bug: the fake agent still committed, and in Testing it wrote over `test-spec.md`. Fixed, with `scripts/fake-agent.test.sh` in CI. `docs/e2e-stages-test.md` updated |
| #96 remove a project | Bug: tasks without a workspace in a removed project failed every action with "Repository … not found", and their pull request status stopped. Fixed: they get the default settings |
| #97 workspace whose worktree is gone | Read: fine |
| #98 Windows worktree tests | CI |
| #99 Homebrew folders on the PATH | Read: fine |
| CI coverage job | Bug: `App.test.tsx` "opens the changes of a workspace…" timed out at 5 s with coverage on, in 4 of the last 8 failed runs, on main too. Fixed: the web tests get 20 s |

Left for later:
- v0.1.2 on an Intel Mac or Linux finds a newer release with no file for it. Timed checks fail quietly; **Check for Updates…** says "SHA256SUMS.txt has no line for …". No change in the app can reach those copies; the release notes should say the builds are paused.
- Still to run: MVP checks 1–4, check 2.1 with a real agent (`agents.permissionMode` set to `default`), the cases of `docs/e2e-stages-test.md`, and the swap on **Restart Now** (the dry run stops before it).
