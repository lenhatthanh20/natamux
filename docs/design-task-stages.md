# Design: task stages, task context and issue links

Status: draft for review. Nothing here is built yet.

## 1. Goal

A task moves through the steps of real work, not only "the agent runs, then I review":

```
                        ┌─ Test spec ─┐
Context → Spec → Plan → ┤             ├→ Review → Testing → Release → (done)
                        └─ Implement ─┘
```

- This is the default for every project. A project can change the list: drop, reorder, add, or run stages side by side (section 6).
- Test spec and Implement run at the same time. Review starts when both are finished (section 3.6).
- Each stage makes something you can read: `context.md`, `spec.md`, `plan.md`, `test-spec.md`, a diff, test results, a pull request.
- Some stages wait for your approval (a gate). Others move on by themselves.
- Context can come from several places: typed by hand, a GitHub issue, or an agent that uses a skill or an MCP server (Jira, Linear, Notion, …).
- A task made in the app can open a GitHub issue for itself. The pull request links to the issue.

This builds the v1 feature "Approval gates" in `plan-features.md` 5.2, and extends "Import tasks from GitHub issues".

## 2. Two axes: stage and state

A task has two fields that answer two questions:

| Field | Question | Values |
|---|---|---|
| `stage` | Where in the work is it? | The stage ids of its project, by default `context`, `spec`, `plan`, `test-spec`, `implement`, `review`, `testing`, `release` |
| `state` | What is happening in this stage now? | The existing states: `draft`, `ready`, `running`, `needs-input`, `review`, `blocked`, `failed`, `done`, `cancelled` |

The state list does not change. Each state now means "in the current stage":

| State | Meaning now | Shown as |
|---|---|---|
| `draft` | The stage cannot start yet; something is missing (for example no context source) | Not started |
| `ready` | The stage can start | Ready |
| `running` | An agent or a command works on the stage | Running |
| `needs-input` | The agent asks a question | Needs input |
| `review` | The stage output waits for your approval | Waiting for approval |
| `blocked`, `failed` | As before | Blocked, Failed |
| `done` | The last stage is approved. Only reachable from the last stage | Done |
| `cancelled` | As before | Cancelled |

"Done" is not a stage. It is the state after the last stage.

Why keep the state list: the "Needs me" strip, the notifications, the agent hooks and the CLI all work on states and keep working. A task in `review` at stage `plan` shows in "Needs me" as "Plan waits for approval".

In a parallel step each stage has its own state, and `state` is the most urgent of them (section 3.6).

## 3. Stages

### 3.1 Stage kinds

Every stage has a kind. The kind decides what runs and when the stage ends.

| Kind | What runs | Output | Ends when |
|---|---|---|---|
| `context` | The context sources of the task (section 4) | `context.md` | Sources are read; then the gate |
| `agent` | The task's agent with the stage prompt | A file named by the stage (`spec.md`, `plan.md`), or code changes | The agent finishes its turn (`agent.finished`); then the gate |
| `review` | Nothing. You read the diff, comment, approve or send back | Review comments | You approve, or send it back to an earlier stage |
| `testing` | The agent with the stage prompt when there is one, then a command (for example `pnpm test`) in a task terminal | New tests, the command output | The command exits: 0 passes, other fails |
| `release` | Push and open a pull request (one click, or on entry when the gate is `auto`) | The pull request | The pull request is merged |
| `manual` | Nothing. A step you do yourself | — | You approve |

### 3.2 The default pipeline

| Stage | Kind | Gate | Prompt or command (default) |
|---|---|---|---|
| `context` | context | manual | — |
| `spec` | agent | manual | Read `context.md`. Write `spec.md`: goal, scope, out of scope, acceptance criteria. Do not change code |
| `plan` | agent | manual | Read `context.md` and `spec.md`. Write `plan.md`: the steps, the files to change, the tests to add. Do not change code |
| `test-spec` (parallel) | agent | manual | Read `context.md` and `spec.md`, not the code. Write `test-spec.md`: under a Cases heading, test cases with input, expected result and edge cases. Do not change code |
| `implement` (parallel) | agent | auto | Read `spec.md` and `plan.md`. Write the tests first, then the code that makes them pass. Commit when done |
| `review` | review | manual | — |
| `testing` | testing | auto | An agent turns each case in `test-spec.md` into a test that is not there yet, then the project's test command runs all tests |
| `release` | release | manual | — |

- **Every agent prompt** ends with "When you are done, run `helmsman done`" (section 3.5).
- **Gate `manual`:** when the stage output is ready, the state becomes `review` and the task waits for you.
- **Gate `auto`:** when the stage output is ready, the task moves to the next stage at once.
- **Gate `auto` and failures:** a failed `testing` stage goes to `failed`. You then retry it, or send the task back to `implement` with the test output as the next prompt.

### 3.3 Moving between stages

Four actions, each one an event:

| Action | Event | Who | Effect |
|---|---|---|---|
| Approve | `stage.approved` | human | `review` → the next stage, state `ready`. The last stage → state `done` |
| Pass | `stage.passed` | system | An `auto` gate: the next stage, state `ready` |
| Send back | `stage.reopened` (`payload.to`) | human | An earlier stage, state `ready`. The review comment becomes the next prompt |
| Skip | `stage.skipped` | human | The next stage, without output. For small tasks that need no spec or plan |

- **Entering a stage with an agent:** the daemon starts the stage at once.
  - When the agent runs already, the stage prompt is sent to it as the next prompt, as review comments are today, and the agent keeps what it learned in earlier stages.
  - When no agent runs, the daemon starts one with the stage prompt.
- **`review.approved`:** stays for old clients and means the same as `stage.approved`.
- **`state.overridden`:** still sets any state, and now also takes an optional `stage`.

### 3.4 Where stage files live

Stage files are `context.md`, `spec.md`, `plan.md`, `test-spec.md` and test output.

- **While the task has a workspace:** they are in `.helmsman-task/` inside the worktree.
  - The agent reads and writes there without leaving its folder, so no extra permission prompts.
  - The daemon adds `.helmsman-task/` to the repository's `.git/info/exclude`, so the files are never committed.
- **Without a workspace, and as a copy:** they are in `<data dir>/tasks/T-0001/`.
  - The daemon copies the worktree files there on each approval and before the workspace is removed.
  - A removed workspace does not lose them.
- **Editing:** you can edit a stage file in the app before you approve it, for example to fix the spec by hand. This edits the working copy.
- **Reading:** the daemon reads the working copy while the worktree exists, else the archived copy.
- **A task without a worktree yet:** stage files are written straight to `<data dir>/tasks/T-0001/`. When the workspace is made, the daemon copies them into `.helmsman-task/`.
- **Leaving the machine:** only `spec.md`, when `createIssue` is `on-spec-approved` (section 5). Nothing else is sent anywhere.

With the default folders (`<data dir>` = `<repo>/.helmsman/`):

```
<data dir>/
├── index.sqlite
├── tasks/
│   ├── T-0012.md                 the task and its state
│   ├── T-0012.events.jsonl       its history
│   └── T-0012/                   archived stage files
│       ├── context.md
│       ├── spec.md
│       ├── plan.md
│       └── test-spec.md
└── worktrees/
    └── <task folder>/            the task's git worktree
        ├── src/ …
        └── .helmsman-task/       working stage files, ignored by git
            ├── context.md
            ├── spec.md
            ├── plan.md
            └── test-spec.md
```

### 3.5 When a stage is finished

A stage is finished (✓) only after three steps, in this order: the stage reports it is done, the daemon checks the result, and the gate lets it through.

**Step 1: the report.** Who says "done" depends on the kind:

| Kind | Report |
|---|---|
| `context` | Manual: you save the text. GitHub issue: `gh issue view` succeeded. Skill or MCP: the agent runs `helmsman done` |
| `agent` | The agent runs `helmsman done`. The stage prompt ends with that instruction |
| `review`, `manual` | None; your approval is the report |
| `testing` | The command exits. With an agent part, the agent runs `helmsman done` first, then the command runs |
| `release` | The pull request watcher sees the pull request merged |

- `helmsman done` sends `stage.reported`. It reads the task from `HELMSMAN_TASK_ID` and the stage from `HELMSMAN_STAGE`, which the daemon sets in each stage terminal. Today it sends `agent.finished`; that changes.
- **The `Stop` hook is not a report.** Claude Code fires it after every turn, also when the agent stops to ask something in plain text. A `Stop` without `helmsman done` moves the stage to `needs-input`, with "The agent stopped without saying it is done" and a **Mark as done** button. That button counts as the report and runs the checks.

**Step 2: the checks.** After the report the daemon checks the result itself:

| Stage | Check | Default |
|---|---|---|
| Every stage with `output` | The file exists in `.helmsman-task/`, is not empty, and was changed after the stage started (so a file from an earlier round does not count) | On |
| Every stage with `sections` | The file has a heading for each listed section | See below |
| `context` | `context.md`, as above | On |
| `spec` | Sections: Goal, Scope, Out of scope, Acceptance criteria | On |
| `plan` | Sections: Steps, Files, Tests | On |
| `test-spec` | Section Cases, with at least one case under it (a `###` heading or a list item) | On |
| `implement` | At least one new commit on the task branch since the stage started (`requireCommit`) | On |
| `implement` | No uncommitted changes in the worktree (`requireClean`); `.helmsman-task/` is ignored, so it does not count | On |
| Any stage with `check` | The command (for example `pnpm typecheck`) exits with 0 | Off |
| `testing` | The test command exits with 0 | On |

- **A failed check, the first time:** the daemon sends the reasons to the agent as its next prompt, for example "plan.md has no Tests section" or "2 files are not committed". The stage stays `running`.
- **A failed check, the second time:** the stage moves to `needs-input` and lists the reasons. You can send it to the agent again, edit the file, or **Approve anyway**. Approve anyway is logged with the reasons it skipped.
- `testing` does not retry by itself: a failed command moves the stage to `failed` (section 3.2).

**Step 3: the gate.**
- `manual`: the stage moves to `review` ("waiting for approval"). It is finished when you approve.
- `auto`: it is finished as soon as the checks pass.

**Events and times.** Each step is an event, so the progress view (7.5) can show times and names:

| Event | When |
|---|---|
| `stage.started` | The stage begins (`startedAt`) |
| `stage.reported` | `helmsman done`, Mark as done, or the report of the kind |
| `stage.check-failed` | A check failed; `payload.reasons` lists why |
| `stage.approved` / `stage.passed` | The gate let it through (`endedAt`; `approvedBy` for `stage.approved`) |

**Settings.** Each stage can change its checks in `.helmsman.json`: `output`, `sections`, `requireCommit`, `requireClean`, `check`. An empty `sections` list turns the section check off.

### 3.6 Parallel stages

A step of the pipeline is one stage, or a group of stages that run at the same time. The default has one group: Test spec and Implement.

- **Lanes:** while a task is in a group, `stage` is the group id (`build`) and a new field `lanes` holds the state of each stage in it, for example `{ "test-spec": "review", "implement": "running" }`. Outside a group `lanes` is empty.
- **Gates per lane:** each stage keeps its own gate. Test spec waits for your approval; Implement passes by itself.
- **End of the group:** when every lane is finished (approved or passed), the task moves to the next step. Review then shows the diff and `test-spec.md` together.
- **Task state:** `state` is the most urgent lane state, in this order: `needs-input`, `failed`, `blocked`, `review`, `running`, `ready`. So "Needs me" shows the task when any lane needs you.
- **Agents:** each lane has its own agent session in its own terminal, in the same worktree. Implement keeps terminal 1 and the session of the earlier stages; Test spec gets a new terminal.
  - The terminal gets `HELMSMAN_STAGE`, and `helmsman hook` sends it with each hook, so the daemon knows which lane a hook is about.
  - Test spec writes only `.helmsman-task/test-spec.md`, which git ignores, so the two agents do not touch the same files.
- **Why Test spec does not read the code:** it is written from the spec alone, like a separate tester, so it checks what was asked, not what was built. Implement does not wait for it; it writes its own tests from the spec and the plan. In Testing, an agent adds a test for each case in `test-spec.md` that the code does not test yet.
- **Events name the lane:** `stage.approved`, `stage.passed`, `stage.reopened` and `stage.skipped` carry `payload.stage` inside a group.
- **Send back:** Review can send the task back to one stage of a group. That lane runs again; the other keeps its result.
- **Rules:** groups do not nest, hold 2 to 4 stages, and hold only `agent` and `manual` kinds.

### 3.7 Where task data lives

All of it is in the daemon's data folder (`HELMSMAN_DATA_DIR`, default `<repo>/.helmsman/`), on the machine that runs the daemon.

| Where | What | Role |
|---|---|---|
| `tasks/T-0001.md` | The task fields, `state`, `stage`, `stages`, `lanes`, `stageSince` | The source of truth |
| `tasks/T-0001.events.jsonl` (new) | Every event of the task, one JSON line each | The history; it survives a lost index |
| `index.sqlite` | The tasks and the events again, for fast lists and filters | A cache; rebuilt from the two files above |
| `tasks/T-0001/` | Copies of the stage files (section 3.4) | Kept after the workspace is gone |

- **Write order:** an event is appended to the `.jsonl` file first, then to SQLite. A crash between the two loses nothing.
- **Rebuild:** at start the daemon compares the event count of each task with SQLite and reloads the tasks that differ. A missing `index.sqlite` is rebuilt whole.
- **Old tasks:** their history is only in SQLite. At the first start after this change the daemon writes their `.jsonl` files from it, once.
- **`stageSince`:** the time the current step started. Lists use it for "time in this stage" without reading events.

## 4. Task context

### 4.1 Sources

| Type | Who reads it | Input when the task is made | Built in this work |
|---|---|---|---|
| `manual` | Nobody; you type it | Text, links | Yes |
| `github-issue` | The daemon, with `gh issue view --comments` | Issue number or address | Yes |
| `skill` | The task's agent, with prompt `/<skill> <ref>` | The ticket key or address | Yes |
| `mcp` | The task's agent, told which MCP server to use | The ticket key or address | Yes |
| `url` | The daemon fetches the page | An address | Later |

Agent sources (`skill`, `mcp`):
- The agent runs in the task's worktree, with the prompt from the project settings.
- It writes `context.md`.
- Helmsman does not set up MCP servers or skills. The agent uses the ones it already has (its user settings or the repository's `.mcp.json`).

The `context` stage ends when `context.md` exists. With gate `manual` you read it, edit it if needed, and approve.

### 4.2 Task fields

```ts
context: { source: 'manual' | 'github-issue' | 'skill' | 'mcp'; ref: string } | null
issue: { provider: 'github' | string; key: string; url: string } | null
```

- `context` says where the context came from.
- `issue` is the link to the tracker item: `#42` on GitHub, `ABC-123` on Jira.
- A task imported from an issue gets both. Import now finds earlier imports by `issue.url`, not by the "Imported from" line in the notes. The old line is still read, for tasks made before this change.

## 5. Issue links (tracker)

### 5.1 When an issue is made

`tracker.createIssue` in the project settings:

| Value | When |
|---|---|
| `never` | Never |
| `manual` | You click "Create issue" on the task |
| `on-spec-approved` (default) | When you approve the `spec` stage. The issue body is `spec.md` |
| `on-pr` | When the pull request is opened, just before it |

- When the pipeline has no `spec` stage, `on-spec-approved` acts as `on-pr`.
- A task that has `issue` already never makes another one. "Create issue" becomes "Open issue".

### 5.2 Providers

- **`github`:** the daemon runs `gh issue create --title --body` in the repository and keeps the address.
- **Other trackers (Jira, Linear):** later.
  - The plan: an agent with that tracker's MCP server makes the item, then calls `helmsman task link T-0001 --issue ABC-123 --url …`.
  - The CLI command is part of this work, so an agent can already link an item by hand.

### 5.3 Pull request

When `tracker.linkInPr` is on (the default):
- The pull request body ends with `Closes #42` (GitHub).
- For another tracker, it ends with the key and address.

Comments on the issue when the stage changes are later.

## 6. Project settings

The app has one default pipeline: the eight stages of section 1, with Test spec and Implement side by side. A repository without settings uses it.

A project changes it in a file `.helmsman.json` at the root of the repository, next to `.helmsman-setup`. It is committed, so the team shares it. Every field is optional; a missing field takes the default.

The name is not `.helmsman/project.json`, because `.helmsman/` is the daemon's default data folder.

```json
{
  "stages": [
    { "id": "context", "kind": "context", "gate": "manual" },
    { "id": "spec", "kind": "agent", "gate": "manual", "output": "spec.md" },
    { "id": "plan", "kind": "agent", "gate": "manual", "output": "plan.md" },
    {
      "id": "build",
      "parallel": [
        { "id": "test-spec", "kind": "agent", "gate": "manual", "output": "test-spec.md" },
        { "id": "implement", "kind": "agent", "gate": "auto" }
      ]
    },
    { "id": "review", "kind": "review", "gate": "manual" },
    { "id": "testing", "kind": "testing", "gate": "auto", "command": "pnpm test" },
    { "id": "release", "kind": "release", "gate": "manual" }
  ],
  "context": {
    "sources": [
      { "type": "manual" },
      { "type": "github-issue" },
      { "type": "mcp", "name": "Jira", "prompt": "Use the jira MCP server to read {ref}, with its comments. Write a summary to {dir}/context.md" },
      { "type": "skill", "name": "Ticket", "skill": "fetch-ticket" }
    ]
  },
  "tracker": { "type": "github", "createIssue": "on-spec-approved", "linkInPr": true }
}
```

Stage fields:
- `id`, `kind`, `gate`: required in each stage entry. `id` is lower case words joined by "-".
- `name`: shown in the app, default from the id.
- `prompt`: replaces the default prompt. Placeholders: `{title}`, `{description}`, `{dir}` (the stage file folder), `{ref}` (the context reference).
- `output`: the file the stage must write (`agent` kind).
- `command`: the test command (`testing` kind).
- `sections`, `requireCommit`, `requireClean`, `check`: the checks of 3.5.
- A group is `{ "id", "name"?, "parallel": [stages] }` (section 3.6).

Other rules:
- **Custom stages:** a project can drop, reorder or add stages, for example a `security` stage of kind `agent` with its own prompt. It can also group stages, or take a stage out of a group so it runs on its own.
- **Snapshot:** a new task copies the list of stage ids to the task (`stages` field). Later edits to the list do not move tasks that are on the way. Prompts, gates and commands are read from the current settings, so a better prompt helps tasks that are running already. A stage id that is gone from the settings acts as kind `manual`.
- **Editing:** the app has a **Project settings** page that edits this file in the repository's main folder. You commit it yourself. "Reset to default" writes the default pipeline back.
- **Wrong settings:** a file that fails to parse shows an error on the page, and the daemon uses the defaults.

## 7. Changes per package

### 7.1 `core`

- `stage.ts`:
  - The `projectSettingsSchema` (zod) and the default pipeline.
  - `nextStep(steps, id)`, and `combinedState(lanes)` for the order in 3.6.
  - `applyTaskEvent(position, event, pipeline)`, where `position` is `{ stage, state, lanes }`.
- `task.ts`: new fields `stage`, `stages`, `lanes`, `stageSince`, `context`, `issue`.
- `progress.ts`: `stageProgress(task, events, pipeline)` for the progress view (7.5).
- `events.ts`: new events `stage.started`, `stage.reported`, `stage.check-failed`, `stage.approved`, `stage.passed`, `stage.reopened`, `stage.skipped`, `context.fetched`, `issue.linked`, `issue.created`.
- Tests for each move in 3.3, the gates, parallel lanes, the snapshot rule, and the migration in section 8.

### 7.2 `daemon`

- `project-settings.ts`: reads `.helmsman.json` of a repository, caches it, and watches the file.
- `stage-runner.ts`:
  - Enters a stage: starts the agent or sends it the prompt, or runs the test command, or opens the pull request.
  - Enters a group: one agent per lane, a new terminal for each lane after the first.
  - Listens to `stage.reported`, finds the lane by `HELMSMAN_STAGE`, runs the checks of 3.5, and passes or waits at the gate.
  - Listens to `agent.finished` (the `Stop` hook): without a report it moves the stage to `needs-input`.
- `stage-checks.ts`: the checks of 3.5 (file, sections, commits, clean worktree, `check` command).
- `stage-files.ts`: the folder rules of 3.4 and the `.git/info/exclude` entry.
- `context-reader.ts`: the `manual` and `github-issue` sources. `skill` and `mcp` go through `stage-runner`.
- `issue-linker.ts`: `gh issue create`, and the `Closes #N` line for the pull request body (`workspace-service.openPullRequest`).
- Routes:
  - `POST /tasks/:id/stage` with `{ action: 'approve' | 'reopen' | 'skip', stage?, to? }`.
  - `GET` and `PUT /tasks/:id/files/:name`.
  - `GET` and `PUT /repos/:id/settings`.
  - `POST /tasks/:id/issue`.
- `task-index.ts`: a `stage` column, so the board can group by it.
- `event-log.ts`: the `.jsonl` files, the write order and the rebuild of 3.7.
- Route `GET /tasks/:id/progress`: the result of `stageProgress` for one task.
- The testing command reports through the CLI:

  ```
  <command> && helmsman stage pass T-0001 || helmsman stage fail T-0001
  ```

### 7.3 `cli`

New commands:
- `helmsman stage approve|back|skip|pass|fail <task> [--stage <id>]`. Inside an agent terminal `--stage` comes from `HELMSMAN_STAGE`.
- `helmsman done` now sends `stage.reported` for the stage of its terminal.
- `helmsman task link <task> --issue <key> --url <url>`.
- `helmsman task create --context github-issue:42`.

### 7.4 `web`

- **Task header:** the progress strip of 7.5.
- **Approve and Send back:** buttons, with the existing `a` and `r` keys.
- **Docs tab:** for each task, `context.md`, `spec.md`, `plan.md`, `test-spec.md`, readable as markdown and editable before approval.
- **Table:** a "Stage" column. The state badge reads "Plan · Waiting for approval"; in a group, "Test spec · Waiting for approval, Implement · Running".
- **Board:** columns by step, plus Done. A group is one column. A switch keeps the old "by state" columns.
- **New task and New workspace dialogs:** a context source picker (from the project settings) and a reference field (`#42`, `ABC-123`).
- **Project settings page:** the stage list (order, on or off, gate, prompt, "run alongside the stage above"), the context sources, and the tracker.
- **Issue:** a link on the task, and "Create issue" when there is none.
- **Needs me:** keeps working on states. The text names the stage.

### 7.5 Progress view

Two places show how far tasks are: a strip in each task, and a view across all tasks.

**Markers.** One marker per stage, the same everywhere:

| Marker | Meaning |
|---|---|
| Check, green | Done: approved or passed |
| Ring in the state color | Current: running, waiting for approval, needs input, failed, blocked |
| Hollow circle | Not reached yet |
| Circle with a dash | Skipped |
| "×2" beside a marker | The stage ran twice, because it was sent back |

**Task progress strip** (task header):
- The steps from left to right, each with its marker, its name, and its time: time spent for a done stage, time so far for the current one.
- A parallel group is one column with its stages stacked, each with its own marker and time.
- Under the strip, one line per current stage: the stage, its state, the time so far, and its actions (Approve, Send back, open its file).
- Hover a done stage: who approved it and when. Click it: its file in the Docs tab, or the diff for Implement.

**Progress view** (dashboard, a third view beside Table and Board):
- A row per task and a column per step, one section per repository, because each repository has its own pipeline.
- Each cell holds the marker. The current cell also shows the state and the time in the stage (from `stageSince`).
- Column headers count the tasks at that step and how many wait for you.
- Tasks that wait for you come first, then the ones furthest along. Done tasks fold at the bottom ("12 done"); cancelled tasks are hidden.
- Keys as in the table: `j`/`k` move, `enter` opens, `a` approves the current stage.
- Narrow windows: each row becomes the task name over a small strip of markers.

**Data.** `stageProgress` (core) turns the task and its events into one entry per stage: `{ stage, status: 'done' | 'current' | 'upcoming' | 'skipped', state?, rounds, startedAt?, endedAt?, approvedBy? }`. The task strip reads it from `GET /tasks/:id/progress`. The progress view needs no events: it uses `stage`, `stages`, `lanes` and `stageSince` from the task list.

## 8. Old tasks

Task files without `stage` are read like this. Nothing is rewritten until the task changes.

| Old state | Stage | State |
|---|---|---|
| `review` | `review` | `review` |
| `done` | the last stage | `done` |
| any other | `implement` | unchanged |

- **`stages`:** old tasks get `implement`, `review`, `release`. That is the work they had before, so no old task suddenly waits for a spec.
- **`issue`:** an old task with an "Imported from <url>" line gets `issue` from that line.
- **Agents started before the change** were not told to run `helmsman done`. Their next `Stop` moves the task to `needs-input` with Mark as done, instead of to review.

## 9. Build order

Separate pull requests, in this order:

1. `core`: stages, groups and lanes, settings schema, events, migration, tests.
2. `daemon` and `cli`: settings file, stage runner, stage files, event log files, routes, CLI commands.
3. Context sources: `manual`, `github-issue`, `skill`, `mcp`.
4. Tracker: GitHub issue on spec approval, manual "Create issue", `Closes #N` in the pull request.
5. `web`: progress strip and progress view, Docs tab, stage column and board, source picker, Project settings page.
6. Docs: `plan-features.md` rows, README, CLI docs.

Step 5 needs the UI pull requests #54 to #58 merged first; it builds on their components.

## 10. Open points

- **One agent session for all stages, or a new one per stage?** The plan above keeps one session for the main line (less to set up, the agent remembers), and a new one for each extra lane of a group. Bigger tasks may need a fresh session for `implement`. That can be a later stage setting (`session: "new"`).
- **Two agents in one worktree:** safe in the default group, because Test spec writes only an ignored file. A custom group of two code-writing stages would clash; the settings page warns when a group has more than one stage without `output`.
- **`testing` runs before the pull request,** so CI on the pull request is not part of it. CI stays visible in `release`, as today.
- **Later:**
  - Jira and Linear issue creation.
  - Comments on the issue at each stage.
  - The `url` source.
