# Design: task stages, task context and issue links

Status: built in the stage pull requests (#60 to #63). The notes at the end say where the code differs from this text.

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
| `needs-input` | The agent asks a question | Needs your input |
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
| `implement` (parallel) | agent | auto | Read `spec.md` and `plan.md`. Write the tests first, then the code that makes them pass. Do not commit: you read the changes first |
| `review` | review | manual | — |
| `testing` | testing | auto | An agent turns each case in `test-spec.md` into a test that is not there yet, then the project's test command runs all tests. The agent does not commit |
| `release` | release | manual | — |

- **Every agent prompt** ends with "When you are done, run `natamux done`" (section 3.5).
- **Commits:** the `implement` and `testing` prompts tell the agent not to commit. You read the changes in the Changes tab and commit them there. A stage with `requireCommit` asks the agent to commit instead.
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

- **While the task has a workspace:** they are in `.natamux-task/` inside the worktree.
  - The agent reads and writes there without leaving its folder, so no extra permission prompts.
  - The daemon adds `.natamux-task/` to the repository's `.git/info/exclude`, so the files are never committed.
- **Without a workspace, and as a copy:** they are in `<data dir>/tasks/T-0001/`.
  - The daemon copies the worktree files there on each approval and before the workspace is removed.
  - A removed workspace does not lose them.
- **Editing:** you can edit a stage file in the app before you approve it, for example to fix the spec by hand. This edits the working copy.
- **Reading:** the daemon reads the working copy while the worktree exists, else the archived copy.
- **A task without a worktree yet:** stage files are written straight to `<data dir>/tasks/T-0001/`. When the workspace is made, the daemon copies them into `.natamux-task/`.
- **Leaving the machine:** only `spec.md`, when `createIssue` is `on-spec-approved` (section 5). Nothing else is sent anywhere.

With the default folders (`<data dir>` = `<repo>/.natamux/`):

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
        └── .natamux-task/        working stage files, ignored by git
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
| `context` | Manual: you save the text. GitHub issue: `gh issue view` succeeded. Skill or MCP: the agent runs `natamux done` |
| `agent` | The agent runs `natamux done`. The stage prompt ends with that instruction |
| `review`, `manual` | None; your approval is the report |
| `testing` | The command exits. With an agent part, the agent runs `natamux done` first, then the command runs |
| `release` | The pull request watcher sees the pull request merged |

- `natamux done` sends `stage.reported`. It reads the task from `NATAMUX_TASK_ID` and the terminal from `NATAMUX_TERMINAL`, which the daemon sets in each terminal; the terminal tells the stage inside a group.
- **The `Stop` hook is not a report.** Claude Code fires it after every turn, also when the agent stops to ask something in plain text. A `Stop` without `natamux done` moves the stage to `needs-input`, with "The agent ended its turn. Reply in the terminal, or resume it." and a **Mark as done** button. That button counts as the report and runs the checks.
- **A paused turn is not a stop.** When the `Stop` lists background work that will wake the agent again (a background shell, a monitor, a scheduled prompt), the stage stays `running` with "Waiting for …" as its summary (`agent.waiting`, see `docs/claude-code-adapter.md`).

**Step 2: the checks.** After the report the daemon checks the result itself:

| Stage | Check | Default |
|---|---|---|
| Every stage with `output` | The file exists in `.natamux-task/`, is not empty, and was changed after the stage started (so a file from an earlier round does not count) | On |
| Every stage with `sections` | The file has a heading for each listed section | See below |
| `context` | `context.md`, as above | On |
| `spec` | Sections: Goal, Scope, Out of scope, Acceptance criteria | On |
| `plan` | Sections: Steps, Files, Tests | On |
| `test-spec` | Section Cases, with at least one case under it (a `###` heading or a list item) | On |
| Any stage with `requireCommit` | At least one new commit on the task branch since the stage started | Off |
| Any stage with `requireClean` | No uncommitted changes in the worktree; `.natamux-task/` is ignored, so it does not count | Off |
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
| `stage.reported` | `natamux done`, Mark as done, or the report of the kind |
| `stage.check-failed` | A check failed; `payload.reasons` lists why |
| `stage.approved` / `stage.passed` | The gate let it through (`endedAt`; `approvedBy` for `stage.approved`) |

**Settings.** Each stage can change its checks in `.natamux.json`: `output`, `sections`, `requireCommit`, `requireClean`, `check`. An empty `sections` list turns the section check off.

### 3.6 Parallel stages

A step of the pipeline is one stage, or a group of stages that run at the same time. The default has one group: Test spec and Implement.

- **Lanes:** while a task is in a group, `stage` is the group id (`build`) and a new field `lanes` holds the state of each stage in it, for example `{ "test-spec": "review", "implement": "running" }`. Outside a group `lanes` is empty.
- **Gates per lane:** each stage keeps its own gate. Test spec waits for your approval; Implement passes by itself.
- **End of the group:** when every lane is finished (approved or passed), the task moves to the next step. Review then shows the diff and `test-spec.md` together.
- **Task state:** `state` is the most urgent lane state, in this order: `needs-input`, `failed`, `blocked`, `review`, `running`, `ready`. So "Needs me" shows the task when any lane needs you.
- **Agents:** each lane has its own agent session in its own terminal, in the same worktree. Implement keeps terminal 1 and the session of the earlier stages; Test spec gets a new terminal.
  - The terminal gets `NATAMUX_TERMINAL`, and each hook sends it (`X-Natamux-Terminal`), so the daemon knows which lane a hook is about.
  - Test spec writes only `.natamux-task/test-spec.md`, which git ignores, so the two agents do not touch the same files.
- **Why Test spec does not read the code:** it is written from the spec alone, like a separate tester, so it checks what was asked, not what was built. Implement does not wait for it; it writes its own tests from the spec and the plan. In Testing, an agent adds a test for each case in `test-spec.md` that the code does not test yet.
- **Events name the lane:** `stage.approved`, `stage.passed`, `stage.reopened` and `stage.skipped` carry `payload.stage` inside a group.
- **Send back:** Review can send the task back to one stage of a group. That lane runs again; the other keeps its result.
- **Rules:** groups do not nest, hold 2 to 4 stages, and hold only `agent` and `manual` kinds.

### 3.7 Where task data lives

All of it is in the daemon's data folder (`NATAMUX_DATA_DIR`, default `<repo>/.natamux/`), on the machine that runs the daemon.

| Where | What | Role |
|---|---|---|
| `tasks/T-0001.md` | The task fields, `state`, `stage`, `stages`, `lanes`, `stageSince`, `workflowStages` | The source of truth |
| `tasks/T-0001.events.jsonl` (new) | Every event of the task, one JSON line each | The history; it survives a lost index |
| `index.sqlite` | The tasks and the events again, for fast lists and filters | A cache; rebuilt from the two files above |
| `tasks/T-0001/` | Copies of the stage files (section 3.4) | Kept after the workspace is gone |

- **Write order:** an event is appended to the `.jsonl` file first, then to SQLite. A crash between the two loses nothing.
- **Rebuild:** at start the daemon compares the event count of each task with SQLite and reloads the tasks that differ. A missing `index.sqlite` is rebuilt whole.
- **Old tasks:** their history is only in SQLite. At the first start after this change the daemon writes their `.jsonl` files from it, once.
- **`stageSince`:** the time the current step started. Lists use it for "time in this stage" without reading events.

### 3.8 Workflow stages

A hard stage can run as a Claude Code workflow: the agent splits the work across sub-agents and checks the result. More accurate, many more tokens.

- **Offer:** the stage field `workflow: true` (section 6). Default: `plan` and `implement`. In Project settings, the details of an agent stage have **Tasks can run it as a workflow** (Default or Changed against the catalog).
- **Choice:** per task, the field `workflowStages` (stage ids), default empty. Set with `PUT /tasks/:id/workflow` `{ stage, on }` or `natamux stage workflow <task> on|off --stage <id>`; each change logs `stage.workflow-set` (`payload.stage`, `payload.on`). The Progress view shows the checkbox on each upcoming or current stage that offers it.
- **Refused:** turning it on for a stage that does not offer it, or for a task whose agent is not Claude Code. Turning it off always works.
- **Prompt:** when an `agent` stage starts with the offer, the choice and Claude Code as its agent, `stagePrompt` adds one line with the keyword `ultracode` before the `natamux done` line. Claude Code runs a workflow only when the prompt asks for it in words. The line is added on every start (new agent, pasted prompt, send back), not to the failed-check prompt.
- **Running stage:** a change does not reach the agent; it counts at the next start. `stage.started` carries `workflow: true` when the line was sent, and the progress view marks such a stage with a "Workflow" label.
- **Waiting:** a workflow runs in the background, so the `Stop` hook lists it in `background_tasks` (type `workflow`). The stage stays `running` (`agent.waiting`), as for other background work.
- **Rules chosen:**
  - `workflow: false` on a stage of any kind parses and does nothing; only `true` needs an `agent` stage.
  - A done or skipped stage takes the choice, since it only matters for a later start (send back). Choosing the same value again writes no event.
  - The "Workflow" label follows how the stage last started (`stage.started` payload), not the current choice or settings.
  - A `workflowStages` value that is not a list of stage ids reads as empty, so a hand-edited task file still loads.
  - A default stage that is gone from the settings keeps its default settings (section 6), so `plan` and `implement` keep offering it there.

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
- Natamux does not set up MCP servers or skills. The agent uses the ones it already has (its user settings or the repository's `.mcp.json`).

The `context` stage ends when `context.md` exists. With gate `manual` you read it, edit it if needed, and approve. A `manual` source is the exception: its context is only the task's own title and notes, so there is nothing new to read, and the stage passes by itself (`stage.passed`). Otherwise every new backlog task would wait in Needs you.

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
  - The plan: an agent with that tracker's MCP server makes the item, then calls `natamux task link T-0001 --issue ABC-123 --url …`.
  - The CLI command is part of this work, so an agent can already link an item by hand.

### 5.3 Pull request

When `tracker.linkInPr` is on (the default):
- The pull request body ends with `Closes #42` (GitHub).
- For another tracker, it ends with the key and address.

Comments on the issue when the stage changes are later.

## 6. Project settings

The app has one default pipeline: the eight stages of section 1, with Test spec and Implement side by side. A repository without settings uses it.

A project changes it in a file `.natamux.json` at the root of the repository, next to `.natamux-setup`. It is committed, so the team shares it. Every field is optional; a missing field takes the default.

The name is not `.natamux/project.json`, because `.natamux/` is the daemon's default data folder.

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
- `prompt`: replaces the default prompt. Placeholders: `{title}`, `{description}`, `{dir}` (the stage file folder), `{ref}` (the context reference), `{skill:<name>}` (a skill, called the way the task's agent calls it; see "Skills in a prompt" below).
- `enabled`: `false` turns the stage off. It stays in the file with its prompt and gate, but new tasks do not get it. The rules (context first, release last, groups of 2 to 4) count only stages that are on.
- `output`: the file the stage must write (`agent` kind).
- `command`: the test command (`testing` kind).
- `sections`, `requireCommit`, `requireClean`, `check`: the checks of 3.5.
- `doneWhen`: the sentence the progress page shows before the stage is reached (7.5).
- `workflow`: the stage offers its tasks a "Run with a workflow" checkbox (3.8). `agent` stages only.
- A group is `{ "id", "name"?, "parallel": [stages] }` (section 3.6).

Other rules:
- **Custom stages:** a project can drop, reorder or add stages, for example a `security` stage of kind `agent` with its own prompt. It can also group stages, or take a stage out of a group so it runs on its own.
- **Snapshot:** a new task copies the list of stage ids to the task (`stages` field). Later edits to the list do not move tasks that are on the way. Prompts, gates and commands are read from the current settings, so a better prompt helps tasks that are running already. A stage id that is gone from the settings keeps the default settings of that id (spec, plan, …); a custom stage that is gone acts as kind `manual`.
- **Editing:** **Settings** in the menu of a project opens **Project settings** with a **Stages** tab and a **JSON** tab. Save writes the file in the repository's main folder. You commit it yourself. **Reset to default** puts the default pipeline in the editor. See "The Stages tab" below.
- **Stages left out:** when the pipeline equals the default, Save leaves `stages` out of the file, so the project follows later changes of the default. Save changes only `stages`; other fields and unknown keys stay.
- **Concurrent edits:** `GET /repos/:id/settings` returns a `version` (a hash of the file text, `none` without a file). `PUT` sends it in `If-Match` and gets 409 `conflict` when the file changed. The screen then offers **Reload** or **Overwrite**.

### The stage catalog

The catalog lives in `packages/core/src/stage-catalog.ts`, so the daemon and the web app read the same list. Each entry has a stage (id, name, kind, gate, output, sections), a phase (Understand, Design, Build, Check, Ship), a sentence for the picker, and a default prompt for agent and testing kinds. The default pipeline is built from 8 of its entries: `context`, `spec`, `plan`, `test-spec`, `implement`, `review`, `testing` and `release`. The table of all 27 entries is in the user docs (`reference/configuration.md`, "Stage catalog").

- A catalog stage that is gone from the settings keeps its catalog settings for a task on its way (`resolveSteps`).
- `lint` and `e2e` are testing stages that need the project's own `command`.
- Stages after the merge (deploy, monitoring, retrospective) are not in the catalog: a release ends the task.

### The Stages tab

- **Layout:** a pipeline preview (flow chart) on top, the stage list on the left, the details of the selected stage on the right, and a footer with **Reset to default**, **Cancel** and **Save** (`Mod+Enter`). Below 640 px it is one column; the details open in place of the list with **Back**.
- **List:** a row has a drag handle, an on/off switch, the name, and marks for "waits for approval" and "custom prompt". The row menu has **Move up**, **Move down** (`Alt+ArrowUp`, `Alt+ArrowDown`), **Run alongside the stage above**, **Run on its own** and **Remove**.
- **Details:** **Name**, **Wait for my approval** (with "Default" or "Changed" and **Use default**; always checked for `review` and `manual`), **Tasks can run it as a workflow** (agent stages, 3.8), **Prompt**, **Command**, **Output file**, **Required sections**, **Finished when**. `requireCommit`, `requireClean` and `check` are edited in the **JSON** tab.
- **Validation:** the browser runs the same schema as the daemon, counting only stages that are on. Save is off while there are problems.
- **Unsaved edits:** closing asks "Discard your changes?".

### Skills in a prompt

`{skill:<name>}` (plugin skills `<plugin>:<skill>`) is filled by the daemon when a stage starts, for the task's agent (`agent-skills.ts`):

1. The agent has the skill and a sure call: that call (`/name` for Claude Code and Copilot CLI, `$name` for Codex). Gemini CLI and Cursor CLI have no sure call.
2. Otherwise a `SKILL.md` of that name exists for any agent: `the skill in <path> (read that file and follow it)`.
3. Otherwise `the <name> skill`, and the stage card says "The skill <name> was not found for <agent>."

The `skill` context source follows the same rules as a whole line. The folders each agent reads are in `docs/agent-adapters.md`. `GET /repos/:id/skills` lists the skills of all agents for the **Insert skill** picker.
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

- `project-settings.ts`: reads `.natamux.json` of a repository, caches it, and watches the file.
- `stage-runner.ts`:
  - Enters a stage: starts the agent or sends it the prompt, or runs the test command, or opens the pull request.
  - Enters a group: one agent per lane, a new terminal for each lane after the first.
  - Listens to `stage.reported`, finds the lane by `NATAMUX_TERMINAL`, runs the checks of 3.5, and passes or waits at the gate.
  - Listens to `agent.finished` (the `Stop` hook): without a report it moves the stage to `needs-input`.
- `stage-checks.ts`: the checks of 3.5 (file, sections, commits, clean worktree, `check` command).
- `stage-files.ts`: the folder rules of 3.4 and the `.git/info/exclude` entry.
- The `manual` and `github-issue` context sources are read in `stage-runner.ts` (`manualContext` in `stage-prompts.ts`). `skill` and `mcp` go through an agent prompt (`contextPrompt`).
- `issue-linker.ts`: `gh issue create`, and the `Closes #N` line for the pull request body (`workspace-service.openPullRequest`).
- Routes:
  - `POST /tasks/:id/stage` with `{ action: 'approve' | 'reopen' | 'skip' | 'pass' | 'fail' | 'report' | 'start', stage?, terminal?, to?, reason? }`.
  - `GET` and `PUT /tasks/:id/files/:name`.
  - `GET` and `PUT /repos/:id/settings`.
  - `POST /tasks/:id/issue`.
- `task-index.ts`: a `stage` column, so the board can group by it.
- `event-log.ts`: the `.jsonl` files, the write order and the rebuild of 3.7.
- No progress route: the web app calls `stageProgress` (core) on the task and its events (`GET /tasks/:id/events`).
- The testing command reports through the CLI:

  ```
  <command> && natamux stage pass T-0001 || natamux stage fail T-0001
  ```

### 7.3 `cli`

New commands:
- `natamux stage approve|back|skip|pass|fail <task> [--stage <id>]`. Inside an agent terminal the stage comes from `NATAMUX_TERMINAL`.
- `natamux done` now sends `stage.reported` for the stage of its terminal.
- `natamux task link <task> --issue <key> --url <url>`.
- `natamux task create --context github-issue:42`.

### 7.4 `web`

- **Task header:** the progress strip of 7.5.
- **Approve and Send back:** buttons, with the existing `a` and `r` keys.
- **Docs tab:** for each task, `context.md`, `spec.md`, `plan.md`, `test-spec.md`, readable as markdown and editable before approval.
- **Table:** a "Stage" column. The state badge reads "Plan · Waiting for approval"; in a group, "Test spec · Waiting for approval, Implement · Running".
- **Board:** columns by step, plus Done. A group is one column. A switch keeps the old "by state" columns.
- **New task and New workspace dialogs:** a context source picker (from the project settings) and a reference field (`#42`, `ABC-123`).
- **Project settings:** the **Stages** tab (preview, stage list with order, on or off and "run alongside the stage above", details with approval and prompt, skill picker, see section 6) and the **JSON** tab for the whole file, including the context sources and the tracker.
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

**Motion.** Motion shows only what is alive; everything else is still.

| Marker | Look | Motion |
|---|---|---|
| Running | A ring with a faint track; an arc that fades along its tail; a dot in the middle | The arc turns once every 900 ms, linear. A soft halo grows out of the node and fades every 1.8 s |
| Waits for approval | A ring in the review color with a dot in the middle | The dot breathes (scale 0.72 to 1) every 2.4 s, ease in and out |
| Line into a running stage | A dashed line in the running color | The dashes move toward the stage, one dash every 0.9 s |
| Needs input, failed | A filled circle with `?` or `×` and a soft outer ring | None |
| Done | A filled circle with a check | None |

- **Reduced motion** (`prefers-reduced-motion`): nothing moves. Running shows a still three-quarter ring; the flowing line becomes a plain dashed line.
- **Window without focus:** the same still look as reduced motion (`data-window="inactive"`, set by `lib/window-focus.ts`). While any endless animation runs, the browser draws every frame, so a dashboard left open on another screen used about 20% CPU.
- **With focus,** `lib/motion-ticker.ts` pauses every endless CSS animation and moves them all to the same time 8 times a second (every 125 ms). The page draws 8 frames a second instead of 120 on a 120 Hz screen. A 900 ms spin moves 50° per tick.
- **Moving stripes and dashes** slide a `::before` with `transform`. Animating `background-position` repaints every frame.
- **Colors:** the state colors of the app (`running`, `review`, `needs-input`, `failed`, `done`), in light and dark.
- **Mock-up:** the canvas "Natamux stage progress" shows both views, with a light and dark switch.

**Task progress page** (the Progress view of the details panel, beside the workspace). It shows what each stage did and the evidence that it is finished. The panel is narrow (320-560), so everything is one column.

- **Overview:** stages done out of all, time since the task started, time it waited for you, times sent back. A bar with one segment per stage, as wide as the time spent; a parallel step is two thin bars on top of each other; a running segment has moving stripes.
- **Stage timeline:** one card per stage, top to bottom, on a rail of markers.
  - Done stage: time, who approved and when, and evidence chips. **Show evidence** opens the passed checks and, for a stage that ran twice, each round with the comment that sent it back.
  - Current stage: everything the checks of 3.5 will look at, live. A parallel step shows its stages side by side, each in its own card.
  - Stage not reached: what will finish it, in one sentence ("Finished when the pull request is merged").
- **Evidence per stage kind:**

| Kind | Evidence shown |
|---|---|
| `context` | The source (issue number and comment count, or the skill or MCP server), `context.md` and its size |
| `agent` with `output` | The file, its size or item count (cases, steps), the first items, the checks |
| `implement` | What the agent does now (from `PreToolUse`), the commits since the stage started, files and lines changed, the checks |
| `review` | Review comments sent to the agent, the approval |
| `testing` | The command, passed and failed counts, the log of a failed run |
| `release` | The pull request, its checks, the merge, the issue it closes |

- **One column:** the overview figures in a 2 × 2 grid and the time bar, then the stages. A done or skipped stage is one line until it is opened; the current stage is always open and scrolls into view; a parallel group stacks its cards. Approve is the first button of the current stage and spans the card.
- **Below the stages:** "Context and issue" (open), "Time per stage" and "Latest events" (closed), as sections that fold.
- **Stage files** open in the Docs tab of the workspace; the panel stays open.
- **Data:** the evidence of each stage entry (`checks`, `files`, `commits`, `rounds`) is built in the web app from the task's events and stage files (`GET /tasks/:id/events`, `GET /tasks/:id/files`).

**Custom pipelines in the progress views.** Both views are drawn from the pipeline; no stage is fixed in the web code.

- **Which list:** the task page uses the task's `stages` snapshot. Names, gates and checks come from the current settings, so a renamed stage shows its new name at once.
- **Colors and markers** follow the state, never the stage. A new stage needs no color or icon.
- **Board, one section per repository,** with that repository's columns. Tasks whose `stages` differ from the current settings (made before a change) go into a sub-section "Earlier pipeline", with columns of their own, so no row is out of line.
- **Groups** of 2 to 4 stages: their columns share a band and the "Side by side" label on the board; on the task page their cards wrap.
- **Long pipelines:** the board scrolls sideways; the Task and Next columns stay in place.
- **A stage id that is gone from the settings:** a default stage keeps its default settings; a custom one is shown with its id as its name, as kind `manual`.
- **Evidence comes from the kind and the checks of a stage,** not its name. A custom `agent` stage with `output`, `sections` and `check` shows its file, its sections and its command result. A `manual` stage shows only who approved it and when.
- **"Finished when …"** for a stage not reached yet is built from its checks; the stage field `doneWhen` replaces it.

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

**Data.** `stageProgress` (core) turns the task and its events into one entry per stage: `{ stage, status: 'done' | 'current' | 'upcoming' | 'skipped', state?, rounds, startedAt?, endedAt?, approvedBy? }`. The task strip computes it from the task's events. The progress view needs no events: it uses `stage`, `stages`, `lanes` and `stageSince` from the task list.

## 8. Old tasks

Task files without `stage` are read like this. Nothing is rewritten until the task changes.

| Old state | Stage | State |
|---|---|---|
| `review` | `review` | `review` |
| `done` | the last stage | `done` |
| any other | `implement` | unchanged |

- **`stages`:** old tasks get `implement`, `review`, `release`. That is the work they had before, so no old task suddenly waits for a spec.
- **`issue`:** an old task with an "Imported from <url>" line gets `issue` from that line.
- **Agents started before the change** were not told to run `natamux done`. Their next `Stop` moves the task to `needs-input` with Mark as done, instead of to review.

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

## 11. Notes on the build

Where the code differs from the text above, or is not built yet:

- **Terminal, not stage, in the environment:** the main agent session keeps terminal 1 across stages, so the daemon maps a terminal to a stage (`NATAMUX_TERMINAL`) instead of setting the stage in the environment.
- **Release starts as ready, not as "waits for you":** it waits for its pull request to be merged, which is not shown under "Needs me". `stage.passed` or a merge finishes it.
- **Without `.natamux.json`** a repository uses the default pipeline. Tasks made before stages keep the legacy pipeline (implement, review, release).
- **Not built yet:**
  - The `url` context source, Jira and Linear issue creation, comments on the issue at each stage.
  - The Implement card does not list its commits yet; the History tab has them.
  - Stage files show as plain text, not rendered markdown.
  - A mark for stages that wait for a person, in every view.
