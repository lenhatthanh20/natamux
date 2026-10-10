# End-to-end test of task stages

How to test stages, context, issues and pull requests on a real repository. Run in a codespace. First run: 2026-10-09 on `lenhatthanh20/natamux-test`, with the fake agent and with GitHub Copilot.

## Set up

1. A GitHub repository with one commit and two issues; one issue has a comment.
2. In the codespace: `env -u GITHUB_TOKEN gh auth login` (HTTPS, let Git use it) and `copilot login`.
3. Clone the repository, build the CLI (`pnpm --filter @natamux/cli build`) and put a `natamux` script that runs `packages/cli/dist/main.js` on the `PATH`.
4. Start a daemon on the clone with:
   - `NATAMUX_REPO_DIR` = the clone, `NATAMUX_DATA_DIR` = a new folder.
   - `NATAMUX_CLAUDE_COMMAND=scripts/fake-agent.sh` for the fake agent.
   - `NATAMUX_COPILOT_ARGS=--allow-all-tools` and `COPILOT_ALLOW_ALL=true`, so Copilot works in the task's worktree without asking.
   - `GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=credential.https://github.com.helper GIT_CONFIG_VALUE_0='!gh auth git-credential'`, so `git push` works from a shell without a token.
5. Start the dashboard against it (`NATAMUX_DAEMON_URL`) and drive it with `playwright-cli`.

## The fake agent

`scripts/fake-agent.sh` does a stage prompt for real: it writes the file the prompt names with its sections, changes code in code stages and commits only when the prompt asks, fixes failed checks, runs `natamux done`, and waits for the next prompt. Marks in the task title make it go wrong on purpose:

| Mark | What it does |
|---|---|
| `[miss-section]` | Leaves out the last section the first time |
| `[no-commit]` | Does not commit the first time, even when asked. Fails a check only in a stage with `requireCommit` |
| `[no-done]` | Ends its turn without `natamux done` |
| `[ask]` | Asks a question and waits for the answer |

## Cases

| # | Case | Expected |
|---|---|---|
| E1 | Start a task with an agent and notes | `context.md` from the notes; Context passes by itself, Spec starts |
| E2 | Send back the spec with a reason | Spec runs; after send back it runs again (×2, reason on the card) |
| E3 | Approve Spec and Plan | Test spec in terminal 2 and Implement in terminal 1 at once; Implement leaves its changes uncommitted and passes |
| E4 | Approve Test spec and Review | Testing runs and passes; Release is ready |
| E5 | The four marks; `requireCommit: true` on `implement` for `[no-commit]` | One failed check goes back to the agent; `[no-done]` and `[ask]` wait for you |
| E5b | Mark as done; answer the agent | Checks run; the agent goes on after the answer |
| E7 | Skip a stage | The next stage starts |
| E8 | Project settings: a pipeline without spec and plan, a `security` stage, `node --test` for testing | New tasks follow it; earlier tasks show as "earlier pipeline" |
| E9 | Restart the daemon; start one on a copy of the data without `index.sqlite` | Same tasks and events |
| E10 | Edit `spec.md` in the Docs tab | The file changes |
| G1 | Start a task, context from issue #1, agent Copilot | `context.md` has the issue and its comment; the task links the issue |
| G2 | Approve the spec of a task with typed context | A GitHub issue opens with the spec as its body |
| G3 | Open the pull request from the page; merge it on GitHub | The body ends with `Closes #N`; the issue closes; the task is done within a minute |
| C1 | Copilot through every stage | It writes each file, leaves code changes uncommitted, runs `natamux done`; two Copilot sessions run side by side |

## Found and fixed in the first run

- A stage prompt was pasted into a plain shell, which ran its lines (and `` `natamux done` ``). Prompts now go only to a running agent.
- Closing a terminal left its agent running. Every process under the shell is now ended.
- Changing `.natamux.json` stopped tasks at stages it no longer had. Default stages keep their default settings.
- Context read from an issue was written to the archive when the workspace was made meanwhile.
- The bell Copilot rings as it starts read as a question. A bell in the first 15 seconds is ignored.
- Cases were counted with their own lists; review and release had no start time; dialogs focused the first field instead of the marked one; `task show` had no stage.
