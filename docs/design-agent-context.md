# Design: agent context

Status: proposal, 2026-10-15. Nothing here is built yet. Code references are to `main` at d3f1cee.

## 1. Goal

Each agent should get what its stage needs, and little else:

- a small context at the start of each stage;
- the work of earlier stages as files, not as a long chat;
- the reason for a send back in full, but without whole logs or long text in the prompt;
- the same project rules for every agent, not only for Claude Code.

The rule we take from [ECC](https://github.com/affaan-m/ECC) (read on 2026-10-15) is: *keep the context window small; persist everything else in files.* Section 7 lists what we do not take from it and why.

## 2. How context flows today

| What | How it reaches the agent | Where in the code |
|---|---|---|
| Stage prompt | The command line at launch. Or, when an agent already runs in the shell, pasted into it | `StageRunner.launch`, `AgentRunner.start` |
| Earlier stages | File paths only: "Read {dir}/spec.md and {dir}/plan.md". `{dir}` is `.natamux-task` | `core/src/stages/stage-catalog.ts` |
| Task context | `context.md`, written by the Context stage. For a GitHub issue it includes the body and **all** comments | `issueContext` in `daemon/src/github/issue-ref.ts` |
| Send back | The stage prompt plus `What to change: <reason>`. The reason is not capped | `promptOf`, `sendBackReason` |
| CI failure | The reason lines plus the last 3000 chars of each failed log. Agent stages only | `checkOnCi` (`LOG_TAIL_CHARS`), `sendBackForCi` |
| Failed stage check | Reasons pasted into the terminal. A `check` command's output is cut to the last 2000 chars | `failChecks`, `stage-checks.ts` |
| Review comment from the dashboard | Pasted into Shell 1. Not capped | `POST /tasks/:id/agent/message` in `daemon/src/http/routes/agents.ts` |
| Project rules | Natamux does nothing. Each agent reads its own file from the worktree. This repo has only `CLAUDE.md` | — |

Sessions:

- **Shell 1 keeps one agent session for the whole main line.** Context, Spec, Plan and Implement add to one transcript. Natamux never clears or compacts it; it relies on the agent's own auto-compact. [design-task-stages.md §10](design-task-stages.md#10-open-points) left `session: "new"` as an open point.
- A stage with `ownShell`, `agent` or `args`, and each extra lane of a group, gets its own shell and a new session.
- When an agent already runs in a stage's shell, `launch` pastes the prompt into it. This is also true for an `ownShell` stage that runs a second time.

## 3. Changes

Each change can ship alone. The order in section 8 is from the least risky to the most risky.

### C1. A new session per stage (`session`)

**Problem.** In Shell 1 the transcript grows from Spec to the last round of Implement. Old exploration, abandoned ideas and tool output stay in the window. Auto-compact then runs at a random point, and sometimes in the middle of an edit. ECC's advice is the same as ours: compact at a phase boundary, never in the middle of the code work.

**Setting.** Add `session: "keep" | "new"` to `stageSchema` (`core/src/stages/schema.ts`), so it can be set in `.natamux.json` and on the Stages tab.

| Value | Meaning |
|---|---|
| not set | `"new"` for an `ownShell` stage (see C2); `"keep"` for every other stage. Today's behavior stays as it is |
| `"keep"` | Paste the prompt into the running agent, as today |
| `"new"` | End the running agent of the stage's shell, then start a new agent with the stage prompt |

**When it applies.** The setting applies only when the stage starts *forward*: its first start, or a start after a stage ended (`ENDS_STAGE`). It does **not** apply to:

- **A send back to the stage** (`stage.reopened`, `review.changes-requested`). The prompt goes into the running session of that stage, so the agent still knows what it just did.
- **`agent.resumed`.** This already has its own path: `relaunch` and `lastSession`.

**Steps** (in `StageRunner.startAgent` / `launch`):

1. When `session` is `"new"`, the start is forward, and `agentRuns(task, terminal)` is true:
   1. **Wait for the agent's turn to end.** The stage that ended may still be printing after `natamux done`. Wait for the hook's turn-end event for that terminal, or at most 30 s.
   2. **End the agent.** Close the shell with `terminals.closeOne(task, terminal)`; it sends SIGHUP, then SIGKILL after `KILL_GRACE_MS` (2 s). Wait until the shell is gone. `closeOne` gets an awaitable form, like `closeAndWait` does for a whole task.
   3. **Clear the per-terminal state.** Reset `launched` and `reported` for that terminal, so a late turn end of the old agent cannot end the new stage.
2. **Start the agent** in the same terminal number with `agents.start`. The full stage prompt goes on the command line.
3. **Record it.** `stage.started` gets `fresh: true`. The stage card shows **New session**, and the terminal shows a line saying that the old session ended.

**Failure paths.**

| Failure | What happens |
|---|---|
| The shell does not end within `KILL_GRACE_MS` | SIGKILL (existing). If the pty is still there after that, the stage fails with "Could not end the agent in Shell N" and waits for you |
| The new agent fails to start | `failStage`, as today |
| The turn end never arrives | After 30 s, close anyway. Record `stage.session-forced` so the card says why |
| The daemon restarts between step 1 and step 2 | `resumeAgents` sees a running stage with no agent. `relaunch` then starts it. Claude Code has no session for the new stage yet, so it gets the prompt |

**Claude Code resume.** With a session per stage, `lastSession` finds the right session after a restart more often than today. Today a stage's id often points into a session that several stages shared.

**Tests.**

- Stage runner:
  - `"new"` closes Shell 1 and starts an agent with the stage prompt.
  - A send back to that stage pastes into it.
  - `"keep"`, and not set, paste as today.
  - A late turn end from the old agent does not end the new stage.
- Settings: the schema accepts `"keep"` and `"new"` and rejects anything else.
- E2E: a three-stage task with `session: "new"` on Implement, run against the e2e repo.

### C2. Review always starts fresh

**Problem.** Agent Review is `ownShell` so that it reviews "with fresh eyes" (comment on `ownShell` in `core/src/stages/schema.ts`). After a send back, Implement fixes the code and Review runs again. Its agent in Shell 5 still runs, so `launch` pastes the new prompt into the old session. The second review remembers the first one and its own findings.

**Change.** An `ownShell` stage defaults to `session: "new"` (C1). Every forward start of Review then gets a new agent. No new code beyond C1.

**Test.** A review → send back → implement → review cycle starts two Review agents, and the second one gets the full stage prompt.

### C3. Point a send back to the report

**Problem.** Agent Review writes `review.md`, then runs `natamux stage back --reason "<one line per problem>"`. Implement gets only the reason. It is not told that `review.md` holds the files and the "why".

**Change.** In `promptOf`, when the cause is a `stage.reopened` with `from`, and the `from` stage has an `output` file that exists in `{dir}`, add one line after `What to change:`:

```
The full report is in {dir}/review.md.
```

The `from` field is already set when an agent sends back (`agentSendsBack`). A CI send back sets `from` to the stage itself, which has no `output`, so nothing is added. A person's `review.changes-requested` has no `from`, so nothing is added.

**Test.** A send back from Agent Review adds the line. A CI send back, and a send back by a person, do not.

### C4. Long text goes into files

**Problem.** Some input reaches the agent with no limit:

- **Issue comments.** `issueContext` writes all of them into `context.md`. Spec, Plan and Test spec read that file.
- **Send-back reasons.** Neither `promptOf` nor `sendBackReason` caps them.
- **Review comments from the dashboard.** They are pasted into Shell 1 in full.

**Change.**

- **Issue context.**
  - Keep the issue body.
  - Keep the newest 20 comments. Each one is capped at 4000 chars, with "… (cut)" at the end.
  - When comments are left out, add the line: `N older comments are not here; read them with gh issue view <number> --comments`.
- **Send-back reason and review comment.** Text a person or an agent wrote is never cut. When it is longer than 2000 chars:
  - write it to `{dir}/send-back-<seq>.md`, where `<seq>` is the event number. The name fits the stage-file rule in `stage-files.ts`, so the file is archived with the task;
  - put the first line in the prompt or paste, then `The full text is in {dir}/send-back-<seq>.md.`

**Failure path.** When the file cannot be written (disk full, worktree gone), fall back to today's behavior: put the whole text in the prompt.

**Tests.**

- `issueContext`:
  - 25 comments keep the newest 20 and add the note.
  - A 10 000-char comment is cut.
- A 3000-char reason makes the file and a short prompt. A 500-char reason does not.

### C5. Implement writes a handoff

**Problem.** Implement is the only main-line stage with no output file. If its agent ends, nothing on disk says what was done and what is left:

- Claude Code can resume its session.
- Codex, Gemini CLI, Copilot CLI and Cursor start over from the stage prompt (`relaunch`).

**Change.** The Implement prompt in the catalog adds:

```
Before you run natamux done, write {dir}/handoff.md: what you changed and why, what you checked,
what is left, and the next step.
```

Restart and send-back prompts for Implement add `Read {dir}/handoff.md when it is there.`

The stage does **not** set `output: "handoff.md"`. `codeStage` picks the group stage with no `output` as the stage that keeps Shell 1. A required output would move Implement out of Shell 1. A missing `handoff.md` is therefore not a failed check.

**Test.** The catalog prompt and the restart prompt hold the new lines. `codeStage` still picks Implement.

### C6. Better CI logs

**Problem.** `checkOnCi` keeps the last 3000 chars of each failed log. In a long job the real error is often earlier: a test failure, then a long coverage or cleanup tail.

A testing stage's CI failure goes back to the code stage without the log at all (`sendBackForCi`). The comment there says this is on purpose, because the testing stage "only runs the code". But the code stage is the one that has to fix it, so it needs the log.

**Change.**

- **Filter the log.** Add `failedLines(log)` in `ci-check.ts`:
  - keep each line that matches `/\b(error|fail(ed|ure)?|panic|exception|assert(ion)?)\b/i`, with 2 lines before and after, until 2000 chars;
  - then add the last 1000 chars;
  - if nothing matches, keep today's 3000-char tail.
- **Send the log back.** `sendBackForCi` passes the logs to the code stage for a testing stage too.

**Tests.**

- A log with an early "FAIL" line and a 10 KB tail keeps the FAIL line.
- A log with no match keeps the tail.
- A testing-stage send back has the log.

### C7. Keep line breaks in the launch prompt

**Problem.** `AgentRunner.start` turns the launch prompt into one line (`.replace(/\s+/g, ' ')`). A pasted prompt keeps its line breaks. So a custom prompt with steps or a list reads differently, depending on whether an agent was already running.

**Change.**

- **macOS and Linux.** Keep line breaks. The command goes into a sourced script, and `quoteArgument` puts it in single quotes, which keep newlines.
- **Windows.** Keep flattening, because PowerShell gets the command typed in.

**Test.** `launchCommand` with a two-line prompt on `darwin` holds the newline inside the quotes. On `win32` the prompt is one line.

### C8. One rules file for every agent

**Problem.** This repo has only `CLAUDE.md`. Codex, Copilot CLI and Cursor read `AGENTS.md`; Gemini CLI reads `GEMINI.md`. None of them see our rules when Natamux runs them on this repo.

**Change** (repo files, not daemon code):

- Move the content of `CLAUDE.md` to `AGENTS.md`. `CLAUDE.md` becomes one line: `@AGENTS.md`.
- Gemini CLI: check its docs for a setting that adds `AGENTS.md` to its context files. If one exists, set it in the settings file Natamux already passes (`GEMINI_CLI_SYSTEM_SETTINGS_PATH`).
- Skills live in `.claude/skills`. Codex reads `.agents/skills` (`SKILL_SOURCES` in `agent-skills.ts`). A symlink `.agents/skills → ../.claude/skills` gives Codex the same skills. Check that Windows checkouts keep the link (`core.symlinks`).

For other projects, `concepts/agents-and-hooks.md` in the user docs can recommend the same layout. This is a docs change only.

## 4. Risks of C1

C1 changes what the agent remembers. Its risks need care before `"new"` becomes the default of any stage.

| # | Risk | Cause | What we do |
|---|---|---|---|
| 1 | **The agent forgets the code it read.** Implement reads files again that Plan already read: more tokens and more time | A new session has only the prompt and the files | Spec has a **Code map** for this. Measure how much Implement re-reads (section 5). Keep `"keep"` as the default until we know |
| 2 | **Decisions that only live in the chat are lost.** Answers you gave in Spec or Plan, or ideas the agent dropped and why | Today they stay in the transcript; a new session never sees them | Spec and Plan prompts say: "Write every decision, and every answer from the user, into the file." Add a `Decisions` section to `plan.md` |
| 3 | **Programs started in Shell 1 die.** The agent may have started a dev server or a watcher in the background; it ends with the shell | `closeOne` ends the whole shell, not only the agent | Close only when the shell's foreground program is the agent (`terminals.foreground`). The card says Shell 1 was restarted. The docs tell you to run long programs in another shell |
| 4 | **Shell 1's scrollback is gone.** You can no longer scroll back to the Plan talk | A new shell has a new screen | The terminal prints "Session of <stage> ended" before closing. The stage files and the task's event log keep what matters. Later: keep the old screen as a read-only snapshot |
| 5 | **The old agent's work is cut off.** It may still write a file or print after `natamux done` | The next stage starts on the same event | Wait for the turn end, at most 30 s (C1 step 1.1) |
| 6 | **Each stage pays a start-up cost.** The agent loads its rules, skill list and MCP tool list again, and the prompt cache of the old session is lost | Every new session does this | Small tasks lose more than they win, which is why `"new"` is per stage and off by default. The measurement includes this cost |
| 7 | **More start-up prompts.** A login, an update notice or a trust prompt can show again and stop the stage | The other agents ask at start | `trustWorkspace` and `prepareWorkspace` already run at each start. The existing **Needs you** signal shows a stopped stage |
| 8 | **State of the old agent leaks.** A late hook event of the old session ends the new stage, or `lastSession` picks the old session | `launched` and `reported` are keyed by terminal | Clear both when closing (C1 step 1.3). `lastSession` already filters by stage id; add a test with two sessions in one shell |
| 9 | **People expect the agent to remember.** You type "as we said in the plan…" into Shell 1 | The UI shows no break today | The **New session** badge on the stage card, and the terminal line from risk 4 |
| 10 | **Send back across stages.** A send back to Plan, after Implement started a new session, pastes into Implement's session | Send backs always paste into the running agent | Accepted. The prompt names the stage, and the files hold the plan. If it causes problems, a send back to an earlier stage can use that stage's `session` value |

## 5. Rollout and measurement for C1

1. **Ship C1 with no default change.** Every catalog stage stays `"keep"` except the `ownShell` stages (C2).
2. **Try it on this repo.** Set `"session": "new"` on Implement in Natamux's own `.natamux.json` for about 10 tasks. Compare with about 10 tasks on `"keep"` of similar size, and record for each task:

   | Measure | Source |
   |---|---|
   | Send backs (review, CI, person) | The task's events |
   | Findings in each `review.md` | The stage files |
   | Time from Implement start to Review passed | The task's events |
   | Tokens of the Implement session (Claude Code only) | The `usage` records in Claude Code's transcript, read by a small script |
   | Files Implement read that Spec's Code map already listed | The transcript's Read tool calls |

3. **Decide.**
   - If `"new"` does not raise send backs and lowers tokens or time, make it the catalog default for Implement.
   - If it raises send backs, keep `"keep"` and look at risk 2 first.

## 6. What ECC does, in short

The repo is large: about 4600 files, many skills, hooks and translations. The parts about context are:

| ECC mechanism | What it does | Taken here? |
|---|---|---|
| strategic-compact | Says when to compact: after research, after planning, after a failed approach. Never in the middle of code work. Advisory only | Yes, as C1. A new session at a stage boundary does the same for every agent, without `/compact` |
| Handoff format (Memory Vault) | Objective, state, evidence, remaining work, next action. No raw transcripts | Yes, as C5 |
| Small always-loaded set | Rules are always loaded, so keep them few. Skills load on demand. Keep MCP tools under about 80 | Yes, as C8 and section 9 |
| Subagents for wide reading | Cheap models read many files and return only a summary | Already done: the `workflow` stage option |
| Hard caps on injected text | 8000 chars at session start, with a cut marker | Yes, as C4. But text is moved into a file, not cut |

ECC has a bench for its "Lean" and "Full" profiles (`docker/context-profiles`). The repo holds only its design: the first runs scored 100 % on every arm, and no token or quality result is published. So no number from ECC is used here.

## 7. Not taken from ECC

| ECC feature | Why not |
|---|---|
| Learned "instincts", injected at each session start | A background model writes rules into every session, unreviewed. ECC's own docs call this "unreviewed context". It opens a path for prompt injection, and nothing measures a gain |
| An LLM session summary, injected at the next start | Summaries are lossy and go stale. ECC had a bug where agents re-ran old commands from a summary, and it now wraps them in a "historical, do not run" warning. Our stage files are written on purpose and checked |
| Context-percent and cost warnings in hooks | Claude Code only, and estimates. A context meter in the dashboard can come later, from the same `usage` records as section 5 |
| Installing the ECC plugin | It lists hundreds of skills to the model at every start. That is the opposite of a small context |
| `--system-prompt` per mode (`contexts/*.md`) | Claude Code only. Our stage prompt already sets the mode |

## 8. Build order

| Step | Change | Size | Why in this order |
|---|---|---|---|
| 1 | C3 (point to the report), C7 (line breaks) | Small | Prompt text only. No change to sessions or files |
| 2 | C6 (CI logs) | Small | `ci-check.ts` and one line in `sendBackForCi` |
| 3 | C4 (long text into files) | Medium | New file writes, with the fallback |
| 4 | C5 (handoff) | Small | Catalog prompt only. Useful before C1 |
| 5 | C8 (one rules file) | Small | Repo files only. Can go at any time |
| 6 | C1 and C2 (`session`) | Large | Ends agents. Needs C5, the measurement in section 5, and an E2E test |

Each step is its own pull request. A user-visible step (C1, C2, C4) also updates `concepts/stages.md`, `reference/configuration.md` and the next release notes, as the `docs` skill says. It does that in the release pull request, not in the feature pull request.

## 9. For every project, not only Natamux

- **Keep one rules file.** Use `AGENTS.md`; make `CLAUDE.md` just `@AGENTS.md`. Keep it short, and move long how-tos into skills.
- **Keep few tools.** Enable only the MCP servers a project uses. Check the start-up cost with `/context` (Claude Code) or `/status` (Codex).
- **Start new sessions at boundaries.** After research, after a plan is written, after a failed approach: write it to a file, then start a new session.
- **Hand off by file.** Use objective, state, evidence, what is left and the next step. Do not paste transcripts.
- **Let subagents read widely.** Give them the goal, not only the question, so they return the right slice.

## 10. Open points

- **Default prompts have only `Task: {title}`.** The description reaches the agent only through `context.md`. Should Spec also get `{description}`?
- **A context meter in the dashboard.** It would read the `usage` records of Claude Code's transcript per shell. That is Claude Code only; other agents would show nothing.
- **Risk 10.** Should a send back to an earlier stage use that stage's `session` value, instead of always pasting?
